#[cfg(not(windows))]
compile_error!("skycraft-skate-fake-skyrim currently targets Windows only.");

#[cfg(windows)]
mod fake {
    use skycraft_skate_protocol as proto;
    use std::{
        mem::size_of,
        ptr::{self, NonNull},
        sync::atomic::{AtomicU64, Ordering},
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
            self.push(proto::COL_CLEAR, &epoch.to_ne_bytes());

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
        println!("Fake Skyrim: published 2-triangle platform; holding mapping for 5 seconds");
        for _ in 0..50 {
            mapping.heartbeat();
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), String> {
    fake::run()
}
