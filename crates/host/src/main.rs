#[cfg(not(windows))]
compile_error!("skyrim-skate-host currently targets Windows only.");

#[cfg(windows)]
mod windows_host {
    use skycraft_skate_protocol as proto;
    use std::{
        ffi::c_void,
        mem::size_of,
        ptr::{self, NonNull},
        slice,
        sync::atomic::{AtomicU32, AtomicU64, Ordering},
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Memory::{
                FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
            },
            Threading::{GetCurrentProcessId, GetTickCount64},
        },
    };

    struct Mapping {
        handle: HANDLE,
        base: NonNull<u8>,
    }

    impl Mapping {
        fn open() -> Option<Self> {
            let mut name: Vec<u16> = proto::MAPPING_NAME.encode_utf16().collect();
            name.push(0);

            let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name.as_ptr()) };
            if handle.is_null() {
                return None;
            }

            let view = unsafe {
                MapViewOfFile(
                    handle,
                    FILE_MAP_ALL_ACCESS,
                    0,
                    0,
                    proto::MAPPING_BYTES,
                )
            };
            let Some(base) = NonNull::new(view.Value.cast::<u8>()) else {
                unsafe { CloseHandle(handle) };
                return None;
            };

            Some(Self { handle, base })
        }

        unsafe fn at<T>(&self, offset: usize) -> *mut T {
            unsafe { self.base.as_ptr().add(offset).cast::<T>() }
        }

        fn validate(&self) -> Result<(), String> {
            let h = unsafe { &*self.at::<proto::Header>(proto::OFF_HEADER) };
            let magic = unsafe { (&*ptr::addr_of!(h.magic).cast::<AtomicU32>()).load(Ordering::Acquire) };
            if magic != proto::MAGIC {
                return Err(format!("mapping magic mismatch: {magic:#010x}"));
            }
            if h.version != proto::VERSION {
                return Err(format!(
                    "protocol mismatch: Skyrim={} host={}",
                    h.version,
                    proto::VERSION
                ));
            }
            if h.mapping_bytes as usize != proto::MAPPING_BYTES {
                return Err(format!(
                    "mapping size mismatch: Skyrim={} host={}",
                    h.mapping_bytes,
                    proto::MAPPING_BYTES
                ));
            }
            Ok(())
        }

        fn announce_host(&self) {
            let h = unsafe { &mut *self.at::<proto::Header>(proto::OFF_HEADER) };
            unsafe {
                (&*ptr::addr_of_mut!(h.host_pid).cast::<AtomicU32>())
                    .store(GetCurrentProcessId(), Ordering::Release);
                (&*ptr::addr_of_mut!(h.host_heartbeat_ms).cast::<AtomicU64>())
                    .store(GetTickCount64(), Ordering::Release);
            }
        }

        fn heartbeat(&self) {
            let h = unsafe { &mut *self.at::<proto::Header>(proto::OFF_HEADER) };
            unsafe {
                (&*ptr::addr_of_mut!(h.host_heartbeat_ms).cast::<AtomicU64>())
                    .store(GetTickCount64(), Ordering::Release);
            }
        }

        fn read_sky_state(&self) -> Option<proto::SkyState> {
            let src = unsafe { self.at::<proto::SkyState>(proto::OFF_SKY_STATE) };
            for _ in 0..64 {
                let seq = unsafe { &*ptr::addr_of!((*src).seq).cast::<AtomicU32>() };
                let s1 = seq.load(Ordering::Acquire);
                if s1 & 1 != 0 {
                    std::hint::spin_loop();
                    continue;
                }

                let snapshot = unsafe { ptr::read_volatile(src) };
                std::sync::atomic::fence(Ordering::Acquire);
                if seq.load(Ordering::Relaxed) == s1 {
                    return Some(snapshot);
                }
            }
            None
        }

        fn write_host_state(&self, mut state: proto::SkateState) {
            let dst = unsafe { self.at::<proto::SkateState>(proto::OFF_SKATE_STATE) };
            let seq = unsafe { &*ptr::addr_of!((*dst).seq).cast::<AtomicU32>() };
            let s = seq.load(Ordering::Relaxed);
            seq.store(s.wrapping_add(1), Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Release);

            state.seq = s.wrapping_add(1);
            unsafe {
                ptr::copy_nonoverlapping(
                    (&state as *const proto::SkateState).cast::<u8>().add(4),
                    dst.cast::<u8>().add(4),
                    size_of::<proto::SkateState>() - 4,
                );
            }

            seq.store(s.wrapping_add(2), Ordering::Release);
        }

        fn drain_collision(
            &self,
            mut on_clear: impl FnMut(u32),
            mut on_region: impl FnMut(proto::CollisionRegion, &[proto::CollisionTri]),
        ) -> Result<(), String> {
            let ring = unsafe { self.base.as_ptr().add(proto::OFF_COLLISION_RING) };
            let head = unsafe {
                (&*ring
                    .add(proto::COLLISION_RING_HEAD_OFF)
                    .cast::<AtomicU64>())
                .load(Ordering::Acquire)
            };
            let tail_atomic = unsafe {
                &*ring
                    .add(proto::COLLISION_RING_TAIL_OFF)
                    .cast::<AtomicU64>()
            };
            let mut tail = tail_atomic.load(Ordering::Relaxed);
            let data = unsafe { ring.add(proto::COLLISION_RING_DATA_OFF) };

            while tail < head {
                let pos = (tail as usize) % proto::COLLISION_RING_DATA_BYTES;
                let hdr = unsafe {
                    ptr::read_unaligned(data.add(pos).cast::<proto::CollisionMessageHeader>())
                };

                if hdr.kind == proto::COL_PAD {
                    tail += (proto::COLLISION_RING_DATA_BYTES - pos) as u64;
                    continue;
                }

                let payload = unsafe { data.add(pos + size_of::<proto::CollisionMessageHeader>()) };
                match hdr.kind {
                    proto::COL_CLEAR => {
                        if hdr.payload_bytes != size_of::<u32>() as u32 {
                            return Err("bad collision clear payload".into());
                        }
                        let epoch = unsafe { ptr::read_unaligned(payload.cast::<u32>()) };
                        on_clear(epoch);
                    }
                    proto::COL_REGION_TRIS => {
                        if hdr.payload_bytes < size_of::<proto::CollisionRegion>() as u32 {
                            return Err("short collision region payload".into());
                        }
                        let region = unsafe {
                            ptr::read_unaligned(payload.cast::<proto::CollisionRegion>())
                        };
                        let tri_bytes = hdr.payload_bytes as usize - size_of::<proto::CollisionRegion>();
                        if tri_bytes != region.count as usize * size_of::<proto::CollisionTri>() {
                            return Err(format!(
                                "collision region size mismatch: count={} bytes={tri_bytes}",
                                region.count
                            ));
                        }
                        let tris = unsafe {
                            slice::from_raw_parts(
                                payload
                                    .add(size_of::<proto::CollisionRegion>())
                                    .cast::<proto::CollisionTri>(),
                                region.count as usize,
                            )
                        };
                        on_region(region, tris);
                    }
                    other => return Err(format!("unknown collision message type {other}")),
                }

                let bytes = proto::align8(
                    size_of::<proto::CollisionMessageHeader>() + hdr.payload_bytes as usize,
                );
                tail += bytes as u64;
            }

            tail_atomic.store(tail, Ordering::Release);
            Ok(())
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                let _ = UnmapViewOfFile(
                    windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.base.as_ptr().cast::<c_void>(),
                    },
                );
                CloseHandle(self.handle);
            }
        }
    }

    pub fn run() {
        eprintln!("SkyrimSkateHost: waiting for {}", proto::MAPPING_NAME);

        loop {
            let Some(mapping) = Mapping::open() else {
                thread::sleep(Duration::from_millis(500));
                continue;
            };

            if let Err(e) = mapping.validate() {
                eprintln!("SkyrimSkateHost: {e}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }

            mapping.announce_host();
            eprintln!("SkyrimSkateHost: connected");

            let mut region_messages = 0_u64;
            let mut triangles = 0_u64;
            let mut last_report = GetTickCount64();

            loop {
                mapping.heartbeat();

                let Some(sky) = mapping.read_sky_state() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };

                let mut state = proto::SkateState::default();
                state.flags = proto::HOST_READY;
                // Until the real Skate Session is connected, echo the start pose.
                // This lets us validate ABI, heartbeats and authority plumbing first.
                state.x = sky.x;
                state.y = sky.y;
                state.z = sky.z;
                state.quat = [0.0, 0.0, 0.0, 1.0];

                if sky.requested_mode == proto::MODE_SKATE {
                    state.flags |= proto::HOST_ACTIVE;
                }
                mapping.write_host_state(state);

                if let Err(e) = mapping.drain_collision(
                    |epoch| {
                        region_messages = 0;
                        triangles = 0;
                        eprintln!("SkyrimSkateHost: collision clear epoch={epoch}");
                    },
                    |region, tris| {
                        region_messages += 1;
                        triangles += tris.len() as u64;
                        if !tris.iter().all(|t| t.v.iter().all(|v| v.is_finite())) {
                            eprintln!(
                                "SkyrimSkateHost: non-finite collision in region ({},{},{})",
                                region.rx, region.ry, region.rz
                            );
                        }
                    },
                ) {
                    eprintln!("SkyrimSkateHost: collision error: {e}");
                }

                let now = GetTickCount64();
                if now - last_report >= 2000 {
                    eprintln!(
                        "SkyrimSkateHost: world={:#x} epoch={} mode={} regions={} triangles={}",
                        sky.world_id,
                        sky.collision_epoch,
                        sky.requested_mode,
                        region_messages,
                        triangles
                    );
                    last_report = now;
                }

                let header = unsafe { &*mapping.at::<proto::Header>(proto::OFF_HEADER) };
                let skyrim_beat = unsafe {
                    (&*ptr::addr_of!(header.skyrim_heartbeat_ms).cast::<AtomicU64>())
                        .load(Ordering::Acquire)
                };
                if skyrim_beat == 0 || now.saturating_sub(skyrim_beat) > 3000 {
                    eprintln!("SkyrimSkateHost: Skyrim heartbeat lost; reconnecting");
                    break;
                }

                thread::sleep(Duration::from_millis(4));
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    windows_host::run();
}
