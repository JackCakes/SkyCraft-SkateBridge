#[cfg(not(windows))]
compile_error!("skycraft-skate-fake-skyrim currently targets Windows only.");

#[cfg(windows)]
mod fake {
    use skycraft_skate_protocol as proto;
    use std::{
        mem::size_of,
        ptr::{self, NonNull},
        sync::atomic::{AtomicU32, AtomicU64, Ordering},
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Memory::{
                CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE,
                UnmapViewOfFile,
            },
            SystemInformation::GetTickCount64,
            Threading::GetCurrentProcessId,
        },
    };

    struct Mapping {
        handle: HANDLE,
        base: NonNull<u8>,
    }

    impl Mapping {
        fn create() -> Result<Self, String> {
            let mut name: Vec<u16> = proto::MAPPING_NAME.encode_utf16().collect();
            name.push(0);
            let size = proto::MAPPING_BYTES as u64;
            let handle = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    ptr::null(),
                    PAGE_READWRITE,
                    (size >> 32) as u32,
                    size as u32,
                    name.as_ptr(),
                )
            };
            if handle.is_null() {
                return Err("CreateFileMappingW failed".into());
            }
            let view = unsafe {
                MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, proto::MAPPING_BYTES)
            };
            let Some(base) = NonNull::new(view.Value.cast::<u8>()) else {
                unsafe { CloseHandle(handle) };
                return Err("MapViewOfFile failed".into());
            };
            unsafe { ptr::write_bytes(base.as_ptr(), 0, proto::MAPPING_BYTES) };
            Ok(Self { handle, base })
        }

        unsafe fn at<T>(&self, offset: usize) -> *mut T {
            unsafe { self.base.as_ptr().add(offset).cast::<T>() }
        }

        fn init(&self) {
            let header = unsafe { &mut *self.at::<proto::Header>(proto::OFF_HEADER) };
            header.magic = proto::MAGIC;
            header.version = proto::VERSION;
            header.skyrim_pid = unsafe { GetCurrentProcessId() };
            header.skyrim_heartbeat_ms = unsafe { GetTickCount64() };
            header.mapping_bytes = proto::MAPPING_BYTES as u64;

            let state = unsafe { &mut *self.at::<proto::SkyState>(proto::OFF_SKY_STATE) };
            *state = proto::SkyState {
                seq: 2,
                flags: proto::SKY_IN_GAME,
                world_id: 0x0000_003c,
                collision_epoch: 1,
                x: 0.0,
                y: 1.0,
                z: 0.0,
                yaw: 0.0,
                viewport_w: 1920,
                viewport_h: 1080,
                aspect: 16.0 / 9.0,
                requested_mode: proto::MODE_MINECRAFT,
            };
        }

        fn heartbeat(&self) {
            let header = unsafe { &mut *self.at::<proto::Header>(proto::OFF_HEADER) };
            unsafe {
                (&*ptr::addr_of_mut!(header.skyrim_heartbeat_ms).cast::<AtomicU64>())
                    .store(GetTickCount64(), Ordering::Release);
            }
        }

        fn host_connected(&self) -> bool {
            let header = unsafe { &*self.at::<proto::Header>(proto::OFF_HEADER) };
            unsafe {
                (&*ptr::addr_of!(header.host_pid).cast::<AtomicU32>())
                    .load(Ordering::Acquire)
                    != 0
            }
        }

        fn set_requested_mode(&self, mode: u32) {
            let state = unsafe { self.at::<proto::SkyState>(proto::OFF_SKY_STATE) };
            let seq = unsafe { &*ptr::addr_of!((*state).seq).cast::<AtomicU32>() };
            let s = seq.load(Ordering::Relaxed);
            seq.store(s.wrapping_add(1), Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Release);
            unsafe {
                (*state).requested_mode = mode;
            }
            seq.store(s.wrapping_add(2), Ordering::Release);
        }

        fn read_host_state(&self) -> Option<proto::SkateState> {
            let state = unsafe { self.at::<proto::SkateState>(proto::OFF_SKATE_STATE) };
            for _ in 0..64 {
                let seq = unsafe { &*ptr::addr_of!((*state).seq).cast::<AtomicU32>() };
                let s1 = seq.load(Ordering::Acquire);
                if s1 & 1 != 0 {
                    std::hint::spin_loop();
                    continue;
                }
                let snapshot = unsafe { ptr::read_volatile(state) };
                std::sync::atomic::fence(Ordering::Acquire);
                if seq.load(Ordering::Relaxed) == s1 {
                    return Some(snapshot);
                }
            }
            None
        }

        fn push(&self, kind: u32, payload: &[u8]) {
            let ring = unsafe { self.base.as_ptr().add(proto::OFF_COLLISION_RING) };
            let head = unsafe { &*ring.add(proto::COLLISION_RING_HEAD_OFF).cast::<AtomicU64>() };
            let old = head.load(Ordering::Relaxed);
            let pos = (old as usize) % proto::COLLISION_RING_DATA_BYTES;
            let total = proto::align8(size_of::<proto::CollisionMessageHeader>() + payload.len());
            assert!(pos + total <= proto::COLLISION_RING_DATA_BYTES);

            let data = unsafe { ring.add(proto::COLLISION_RING_DATA_OFF + pos) };
            unsafe {
                ptr::write_unaligned(
                    data.cast::<proto::CollisionMessageHeader>(),
                    proto::CollisionMessageHeader {
                        kind,
                        payload_bytes: payload.len() as u32,
                    },
                );
                ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    data.add(size_of::<proto::CollisionMessageHeader>()),
                    payload.len(),
                );
                if total > size_of::<proto::CollisionMessageHeader>() + payload.len() {
                    ptr::write_bytes(
                        data.add(size_of::<proto::CollisionMessageHeader>() + payload.len()),
                        0,
                        total - size_of::<proto::CollisionMessageHeader>() - payload.len(),
                    );
                }
            }
            head.store(old + total as u64, Ordering::Release);
        }

        fn publish_platform(&self) {
            let epoch = 1_u32;
            self.push(proto::COL_CLEAR, &epoch.to_le_bytes());

            let region = proto::CollisionRegion {
                rx: 0,
                ry: 0,
                rz: 0,
                count: 2,
                epoch,
                world_id: 0x0000_003c,
            };
            let tris = [
                proto::CollisionTri {
                    v: [-1.0, 0.0, -1.0, -1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
                    flags: 0,
                },
                proto::CollisionTri {
                    v: [-1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, 0.0, -1.0],
                    flags: 0,
                },
            ];
            let mut payload = vec![0u8; size_of::<proto::CollisionRegion>() + size_of_val(&tris)];
            unsafe {
                ptr::copy_nonoverlapping(
                    (&region as *const proto::CollisionRegion).cast::<u8>(),
                    payload.as_mut_ptr(),
                    size_of::<proto::CollisionRegion>(),
                );
                ptr::copy_nonoverlapping(
                    tris.as_ptr().cast::<u8>(),
                    payload.as_mut_ptr().add(size_of::<proto::CollisionRegion>()),
                    size_of_val(&tris),
                );
            }
            self.push(proto::COL_REGION_TRIS, &payload);
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                let _ = UnmapViewOfFile(
                    windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.base.as_ptr().cast(),
                    },
                );
                CloseHandle(self.handle);
            }
        }
    }

    pub fn run() -> Result<(), String> {
        let mapping = Mapping::create()?;
        mapping.init();
        mapping.publish_platform();
        println!("Fake Skyrim: published 2-triangle platform");

        let mut connected = false;
        for _ in 0..30 {
            mapping.heartbeat();
            if mapping.host_connected() {
                connected = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !connected {
            return Err("host did not announce itself".into());
        }

        mapping.set_requested_mode(proto::MODE_SKATE);
        println!("Fake Skyrim: requested synthetic Skate authority");

        let mut active_seen = false;
        let mut max_nudge = 0.0_f64;
        for _ in 0..25 {
            mapping.heartbeat();
            if let Some(state) = mapping.read_host_state() {
                if (state.flags & proto::HOST_ERROR) != 0 {
                    return Err(format!("host returned error {}", state.error_code));
                }
                if (state.flags & proto::HOST_ACTIVE) != 0 {
                    active_seen = true;
                    if !state.x.is_finite() || !state.y.is_finite() || !state.z.is_finite() {
                        return Err("host returned non-finite synthetic pose".into());
                    }
                    let nudge = (state.x * state.x + (state.y - 1.0) * (state.y - 1.0) + state.z * state.z).sqrt();
                    max_nudge = max_nudge.max(nudge);
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !active_seen {
            return Err("host never entered synthetic authority".into());
        }
        if !(0.40..=0.65).contains(&max_nudge) {
            return Err(format!("synthetic nudge out of bounds: {max_nudge:.3} blocks"));
        }

        mapping.set_requested_mode(proto::MODE_MINECRAFT);
        let mut released = false;
        for _ in 0..10 {
            mapping.heartbeat();
            if let Some(state) = mapping.read_host_state() {
                if (state.flags & proto::HOST_ACTIVE) == 0 {
                    released = true;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !released {
            return Err("host did not release synthetic authority".into());
        }

        // Keep the mapping alive long enough for the existing rail/collision smoke checks.
        for _ in 0..20 {
            mapping.heartbeat();
            thread::sleep(Duration::from_millis(100));
        }
        println!("Fake Skyrim: synthetic handoff + release passed; max nudge={max_nudge:.3}");
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), String> {
    fake::run()
}
