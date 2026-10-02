#[cfg(not(windows))]
compile_error!("skyrim-skate-host currently targets Windows only.");

#[cfg(windows)]
mod windows_host {
    use skycraft_skate_geometry::{self as geometry, Triangle};
    use skycraft_skate_protocol as proto;
    use std::{
        collections::HashMap,
        ffi::c_void,
        mem::size_of,
        ptr::{self, NonNull},
        slice,
        sync::atomic::{AtomicU32, AtomicU64, Ordering},
        thread,
        time::{Duration, Instant},
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

    enum CollisionEvent {
        Clear(u32),
        Region(proto::CollisionRegion, Vec<proto::CollisionTri>),
    }

    #[derive(Default)]
    struct CollisionWorld {
        epoch: u32,
        world_id: u32,
        regions: HashMap<(i32, i32, i32), Vec<proto::CollisionTri>>,
        dirty: bool,
        triangle_count: usize,
        rail_count: usize,
    }

    impl CollisionWorld {
        fn clear(&mut self, epoch: u32) {
            self.epoch = epoch;
            self.world_id = 0;
            self.regions.clear();
            self.dirty = true;
            self.triangle_count = 0;
            self.rail_count = 0;
        }

        fn replace_region(
            &mut self,
            region: proto::CollisionRegion,
            tris: Vec<proto::CollisionTri>,
        ) {
            // A clear normally precedes a new world. Be defensive if the producer
            // races a reconnect and a region arrives first.
            if self.epoch == 0 || region.epoch != self.epoch || self.world_id == 0 {
                if self.epoch != 0 && region.epoch != self.epoch {
                    self.regions.clear();
                    self.triangle_count = 0;
                    self.rail_count = 0;
                }
                self.epoch = region.epoch;
                self.world_id = region.world_id;
            }

            if region.epoch != self.epoch || region.world_id != self.world_id {
                return;
            }

            let finite: Vec<_> = tris
                .into_iter()
                .filter(|t| t.v.iter().all(|v| v.is_finite()))
                .collect();

            let key = (region.rx, region.ry, region.rz);
            if let Some(old) = self.regions.insert(key, finite) {
                self.triangle_count = self.triangle_count.saturating_sub(old.len());
            }
            self.triangle_count += self.regions.get(&key).map_or(0, Vec::len);
            self.dirty = true;
        }

        fn rebuild_rails(&mut self) -> geometry::RailResult {
            let mut input = Vec::with_capacity(self.triangle_count);
            for tris in self.regions.values() {
                for tri in tris {
                    input.push(Triangle {
                        p: [
                            glam_vec(tri.v[0], tri.v[1], tri.v[2]),
                            glam_vec(tri.v[3], tri.v[4], tri.v[5]),
                            glam_vec(tri.v[6], tri.v[7], tri.v[8]),
                        ],
                        flags: tri.flags,
                    });
                }
            }
            let result = geometry::find_rails(&input);
            self.rail_count = result.rails.len();
            self.dirty = false;
            result
        }
    }

    fn glam_vec(x: f32, y: f32, z: f32) -> glam::Vec3 {
        glam::Vec3::new(x, y, z)
    }

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
            let magic = unsafe {
                (&*ptr::addr_of!(h.magic).cast::<AtomicU32>()).load(Ordering::Acquire)
            };
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

        fn skyrim_heartbeat(&self) -> u64 {
            let h = unsafe { &*self.at::<proto::Header>(proto::OFF_HEADER) };
            unsafe {
                (&*ptr::addr_of!(h.skyrim_heartbeat_ms).cast::<AtomicU64>())
                    .load(Ordering::Acquire)
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

        fn write_host_state(&self, state: proto::SkateState) {
            let dst = unsafe { self.at::<proto::SkateState>(proto::OFF_SKATE_STATE) };
            let seq = unsafe { &*ptr::addr_of!((*dst).seq).cast::<AtomicU32>() };
            let s = seq.load(Ordering::Relaxed);
            seq.store(s.wrapping_add(1), Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Release);

            unsafe {
                ptr::copy_nonoverlapping(
                    (&state as *const proto::SkateState).cast::<u8>().add(4),
                    dst.cast::<u8>().add(4),
                    size_of::<proto::SkateState>() - 4,
                );
            }

            seq.store(s.wrapping_add(2), Ordering::Release);
        }

        fn drain_collision(&self) -> Result<Vec<CollisionEvent>, String> {
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
            let mut events = Vec::new();

            while tail < head {
                let pos = (tail as usize) % proto::COLLISION_RING_DATA_BYTES;
                let hdr = unsafe {
                    ptr::read_unaligned(data.add(pos).cast::<proto::CollisionMessageHeader>())
                };

                if hdr.kind == proto::COL_PAD {
                    tail += (proto::COLLISION_RING_DATA_BYTES - pos) as u64;
                    continue;
                }

                let total = size_of::<proto::CollisionMessageHeader>()
                    .checked_add(hdr.payload_bytes as usize)
                    .ok_or("collision message length overflow")?;
                if total > proto::COLLISION_RING_DATA_BYTES {
                    return Err(format!("collision message too large: {total}"));
                }
                if pos + total > proto::COLLISION_RING_DATA_BYTES {
                    return Err("collision message crossed ring end without pad".into());
                }

                let payload = unsafe { data.add(pos + size_of::<proto::CollisionMessageHeader>()) };
                match hdr.kind {
                    proto::COL_CLEAR => {
                        if hdr.payload_bytes != size_of::<u32>() as u32 {
                            return Err("bad collision clear payload".into());
                        }
                        let epoch = unsafe { ptr::read_unaligned(payload.cast::<u32>()) };
                        events.push(CollisionEvent::Clear(epoch));
                    }
                    proto::COL_REGION_TRIS => {
                        if hdr.payload_bytes < size_of::<proto::CollisionRegion>() as u32 {
                            return Err("short collision region payload".into());
                        }
                        let region = unsafe {
                            ptr::read_unaligned(payload.cast::<proto::CollisionRegion>())
                        };
                        let tri_bytes =
                            hdr.payload_bytes as usize - size_of::<proto::CollisionRegion>();
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
                        events.push(CollisionEvent::Region(region, tris.to_vec()));
                    }
                    other => return Err(format!("unknown collision message type {other}")),
                }

                tail += proto::align8(total) as u64;
            }

            tail_atomic.store(tail, Ordering::Release);
            Ok(events)
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

            let mut world = CollisionWorld::default();
            let mut last_report = Instant::now();
            let mut last_rail_build = Instant::now() - Duration::from_secs(2);

            loop {
                mapping.heartbeat();

                let Some(sky) = mapping.read_sky_state() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };

                // Safe placeholder until retail-backed Skate Session is wired in:
                // publish the current Skyrim pose but never move it.
                let mut state = proto::SkateState::default();
                state.flags = proto::HOST_READY;
                state.x = sky.x;
                state.y = sky.y;
                state.z = sky.z;
                state.quat = [0.0, 0.0, 0.0, 1.0];
                if sky.requested_mode == proto::MODE_SKATE {
                    state.flags |= proto::HOST_ACTIVE;
                }
                mapping.write_host_state(state);

                match mapping.drain_collision() {
                    Ok(events) => {
                        for event in events {
                            match event {
                                CollisionEvent::Clear(epoch) => {
                                    world.clear(epoch);
                                    eprintln!(
                                        "SkyrimSkateHost: collision clear epoch={epoch}"
                                    );
                                }
                                CollisionEvent::Region(region, tris) => {
                                    world.replace_region(region, tris);
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("SkyrimSkateHost: collision error: {e}"),
                }

                if world.dirty && last_rail_build.elapsed() >= Duration::from_secs(1) {
                    let rails = world.rebuild_rails();
                    eprintln!(
                        "SkyrimSkateHost: rail scan tris={} edges={} lips={} runs={} rails={}",
                        rails.census.input_triangles,
                        rails.census.walkable_edges,
                        rails.census.lips,
                        rails.census.runs,
                        rails.census.rails
                    );
                    last_rail_build = Instant::now();
                }

                if last_report.elapsed() >= Duration::from_secs(2) {
                    eprintln!(
                        "SkyrimSkateHost: world={:#x} epoch={} mode={} regions={} triangles={} rails={}",
                        sky.world_id,
                        sky.collision_epoch,
                        sky.requested_mode,
                        world.regions.len(),
                        world.triangle_count,
                        world.rail_count
                    );
                    last_report = Instant::now();
                }

                let now = unsafe { GetTickCount64() };
                let skyrim_beat = mapping.skyrim_heartbeat();
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
