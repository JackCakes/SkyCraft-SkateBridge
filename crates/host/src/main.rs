#[cfg(not(windows))]
compile_error!("skyrim-skate-host currently targets Windows only.");

#[cfg(windows)]
mod windows_host {
    use skycraft_skate_geometry::{self as geometry, Triangle};
    use skycraft_skate_protocol as proto;
    use std::{
        collections::HashMap,
        ffi::c_void,
        fs::{File, OpenOptions},
        io::Write,
        mem::size_of,
        path::PathBuf,
        ptr::{self, NonNull},
        slice,
        sync::{
            atomic::{AtomicU32, AtomicU64, Ordering},
            Mutex, OnceLock,
        },
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Memory::{
                FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
            },
            SystemInformation::GetTickCount64,
            Threading::GetCurrentProcessId,
        },
    };

    static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

    fn log_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|dir| dir.join("SkyrimSkateHost.log")))
            .unwrap_or_else(|| PathBuf::from("SkyrimSkateHost.log"))
    }

    fn init_log() {
        let path = log_path();
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .ok();
        let _ = LOG_FILE.set(Mutex::new(file));

        log_line(format_args!(
            "SkyrimSkateHost: log file {}",
            path.display()
        ));
    }

    fn log_line(args: std::fmt::Arguments<'_>) {
        eprintln!("{args}");
        if let Some(lock) = LOG_FILE.get() {
            if let Ok(mut slot) = lock.lock() {
                if let Some(file) = slot.as_mut() {
                    let _ = writeln!(file, "{args}");
                    let _ = file.flush();
                }
            }
        }
    }

    macro_rules! host_log {
        ($($arg:tt)*) => {
            log_line(format_args!($($arg)*))
        };
    }

    enum CollisionEvent {
        Clear(u32),
        Region(proto::CollisionRegion, Vec<proto::CollisionTri>),
    }

    // SkyCraft collision regions are 8x8x8 blocks. Keep a bounded local working
    // set around the skater instead of retaining every region ever visited.
    const REGION_BLOCKS: f64 = 8.0;
    const KEEP_RADIUS_XZ_REGIONS: i32 = 7; // ~56 blocks each horizontal direction
    const KEEP_RADIUS_Y_REGIONS: i32 = 4;  // ~32 blocks up/down

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

        fn prune_around(&mut self, centre: [f64; 3]) -> usize {
            if !centre.iter().all(|v| v.is_finite()) {
                return 0;
            }
            let cr = (
                (centre[0] / REGION_BLOCKS).floor() as i32,
                (centre[1] / REGION_BLOCKS).floor() as i32,
                (centre[2] / REGION_BLOCKS).floor() as i32,
            );
            let before_regions = self.regions.len();
            let before_tris = self.triangle_count;
            self.regions.retain(|&(rx, ry, rz), _| {
                (rx - cr.0).abs() <= KEEP_RADIUS_XZ_REGIONS
                    && (rz - cr.2).abs() <= KEEP_RADIUS_XZ_REGIONS
                    && (ry - cr.1).abs() <= KEEP_RADIUS_Y_REGIONS
            });
            if self.regions.len() != before_regions {
                self.triangle_count = self.regions.values().map(Vec::len).sum();
                self.dirty = true;
            }
            before_tris.saturating_sub(self.triangle_count)
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

    fn glam_vec(x: f32, y: f32, z: f32) -> geometry::Vec3 {
        geometry::Vec3::new(x, y, z)
    }

    fn fnv1a64(name: &str) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    fn identity_matrix() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    fn synthetic_pose_frame(state: &proto::SkateState, input: Option<&proto::InputState>) -> proto::PoseFrame {
        let mut pose = proto::PoseFrame::default();
        if (state.flags & proto::HOST_ACTIVE) == 0 {
            return pose;
        }

        pose.flags = proto::POSE_VALID;
        pose.tick = input.map_or(0, |i| u64::from(i.packet));
        pose.bone_count = 2;
        let root_hash = fnv1a64("synthetic_root");
        let board_hash = fnv1a64("synthetic_board");
        pose.name_set_id = (root_hash ^ board_hash) as u32;
        pose.bones[0] = proto::BonePose {
            name_hash: root_hash,
            matrix: identity_matrix(),
        };
        let mut board = identity_matrix();
        board[13] = -0.05;
        pose.bones[1] = proto::BonePose {
            name_hash: board_hash,
            matrix: board,
        };
        pose
    }

    /// Diagnostic controller used only to prove the complete transport/authority
    /// path before retail-backed Skate physics is available. It consumes the same
    /// XInput-shaped state the real Session expects, but movement is intentionally
    /// simple and hard-bounded around the activation point.
    struct SyntheticController {
        active: bool,
        world_id: u32,
        epoch: u32,
        origin: [f64; 3],
        pos: [f64; 3],
        yaw_deg: f32,
        camera_yaw_deg: f32,
        camera_pitch_deg: f32,
        blocked_until_release: bool,
    }

    impl SyntheticController {
        const SPEED_BLOCKS_PER_SEC: f64 = 0.60;
        const MAX_RADIUS_BLOCKS: f64 = 1.25;
        const CAMERA_DISTANCE_BLOCKS: f64 = 2.5;
        const CAMERA_HEIGHT_BLOCKS: f64 = 0.85;

        fn new() -> Self {
            Self {
                active: false,
                world_id: 0,
                epoch: 0,
                origin: [0.0; 3],
                pos: [0.0; 3],
                yaw_deg: 0.0,
                camera_yaw_deg: 0.0,
                camera_pitch_deg: 8.0,
                blocked_until_release: false,
            }
        }

        fn safe_sky(sky: &proto::SkyState) -> bool {
            let blocked = proto::SKY_MENU_OPEN | proto::SKY_LOADING;
            (sky.flags & proto::SKY_IN_GAME) != 0
                && (sky.flags & blocked) == 0
                && sky.x.is_finite()
                && sky.y.is_finite()
                && sky.z.is_finite()
                && sky.yaw.is_finite()
                && sky.world_id != 0
        }

        fn stop(&mut self, reason: &str, block_until_release: bool) {
            if self.active {
                host_log!("SkyrimSkateHost: synthetic authority released ({reason})");
            }
            self.active = false;
            self.blocked_until_release |= block_until_release;
        }

        fn update(
            &mut self,
            sky: &proto::SkyState,
            input: Option<&proto::InputState>,
        ) -> proto::SkateState {
            let requested = sky.requested_mode == proto::MODE_SKATE;

            if !requested {
                self.stop("mode returned to Minecraft", false);
                self.blocked_until_release = false;
            } else if self.active && !Self::safe_sky(sky) {
                self.stop("Skyrim state is not safe", true);
            } else if self.active
                && (self.world_id != sky.world_id || self.epoch != sky.collision_epoch)
            {
                self.stop("world/epoch changed", true);
            }

            if requested && Self::safe_sky(sky) && !self.active && !self.blocked_until_release {
                self.active = true;
                self.world_id = sky.world_id;
                self.epoch = sky.collision_epoch;
                self.origin = [sky.x, sky.y, sky.z];
                self.pos = self.origin;
                self.yaw_deg = sky.yaw;
                self.camera_yaw_deg = sky.yaw;
                self.camera_pitch_deg = 8.0;
                host_log!(
                    "SkyrimSkateHost: synthetic authority acquired world={:#x} epoch={} origin=({:.3},{:.3},{:.3}) yaw={:.1}",
                    self.world_id,
                    self.epoch,
                    self.origin[0],
                    self.origin[1],
                    self.origin[2],
                    self.yaw_deg
                );
            }

            let mut state = proto::SkateState::default();
            state.flags = proto::HOST_READY;
            state.x = sky.x;
            state.y = sky.y;
            state.z = sky.z;
            state.quat = [0.0, 0.0, 0.0, 1.0];
            state.state_code = 0x5359_4E54; // "SYNT"

            if self.active {
                let mut dt = 0.0_f64;
                let mut lx = 0.0_f64;
                let mut ly = 0.0_f64;
                let mut rx = 0.0_f32;
                let mut ry = 0.0_f32;
                if let Some(input) = input.filter(|i| (i.flags & proto::INPUT_VALID) != 0) {
                    if input.frame_seconds.is_finite() {
                        dt = f64::from(input.frame_seconds.clamp(0.0, 0.1));
                    }
                    lx = f64::from(input.left[0]) / 32767.0;
                    ly = f64::from(input.left[1]) / 32767.0;
                    rx = f32::from(input.right[0]) / 32767.0;
                    ry = f32::from(input.right[1]) / 32767.0;
                }

                let yaw = f64::from(self.yaw_deg).to_radians();
                let right = [yaw.cos(), yaw.sin()];
                let forward = [-yaw.sin(), yaw.cos()];
                let vx = (right[0] * lx + forward[0] * ly) * Self::SPEED_BLOCKS_PER_SEC;
                let vz = (right[1] * lx + forward[1] * ly) * Self::SPEED_BLOCKS_PER_SEC;

                let mut nx = self.pos[0] + vx * dt;
                let mut nz = self.pos[2] + vz * dt;
                let ox = nx - self.origin[0];
                let oz = nz - self.origin[2];
                let radius = (ox * ox + oz * oz).sqrt();
                if radius > Self::MAX_RADIUS_BLOCKS {
                    let k = Self::MAX_RADIUS_BLOCKS / radius;
                    nx = self.origin[0] + ox * k;
                    nz = self.origin[2] + oz * k;
                }
                self.pos = [nx, self.origin[1], nz];

                self.camera_yaw_deg =
                    (self.camera_yaw_deg + rx * 120.0 * dt as f32).rem_euclid(360.0);
                self.camera_pitch_deg =
                    (self.camera_pitch_deg + ry * 90.0 * dt as f32).clamp(-55.0, 55.0);

                state.x = self.pos[0];
                state.y = self.pos[1];
                state.z = self.pos[2];
                state.velocity = [vx as f32, 0.0, vz as f32];
                state.flags |= proto::HOST_ACTIVE | proto::HOST_CAMERA_VALID;

                let cy = f64::from(self.camera_yaw_deg).to_radians();
                let cp = f64::from(self.camera_pitch_deg).to_radians();
                let cos_pitch = cp.cos();
                let forward3 = [
                    -cy.sin() * cos_pitch,
                    cp.sin(),
                    cy.cos() * cos_pitch,
                ];
                let right3 = [cy.cos(), 0.0, cy.sin()];
                let up3 = [
                    forward3[1] * right3[2] - forward3[2] * right3[1],
                    forward3[2] * right3[0] - forward3[0] * right3[2],
                    forward3[0] * right3[1] - forward3[1] * right3[0],
                ];
                let target = [
                    self.pos[0],
                    self.pos[1] + Self::CAMERA_HEIGHT_BLOCKS,
                    self.pos[2],
                ];
                state.camera_pos = [
                    target[0] - forward3[0] * Self::CAMERA_DISTANCE_BLOCKS,
                    target[1] - forward3[1] * Self::CAMERA_DISTANCE_BLOCKS,
                    target[2] - forward3[2] * Self::CAMERA_DISTANCE_BLOCKS,
                ];
                state.camera_forward = [
                    forward3[0] as f32,
                    forward3[1] as f32,
                    forward3[2] as f32,
                ];
                state.camera_up = [up3[0] as f32, up3[1] as f32, up3[2] as f32];
                state.fov_deg = 70.0;
            }

            state
        }
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

        fn read_input_state(&self) -> Option<proto::InputState> {
            let src = unsafe { self.at::<proto::InputState>(proto::OFF_INPUT_STATE) };
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

        fn write_pose_frame(&self, frame: &proto::PoseFrame) {
            let dst = unsafe { self.at::<proto::PoseFrame>(proto::OFF_POSE_FRAME) };
            let seq = unsafe { &*ptr::addr_of!((*dst).seq).cast::<AtomicU32>() };
            let s = seq.load(Ordering::Relaxed);
            seq.store(s.wrapping_add(1), Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Release);

            unsafe {
                ptr::copy_nonoverlapping(
                    (frame as *const proto::PoseFrame).cast::<u8>().add(4),
                    dst.cast::<u8>().add(4),
                    size_of::<proto::PoseFrame>() - 4,
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
        init_log();
        host_log!("SkyrimSkateHost: waiting for {}", proto::MAPPING_NAME);

        loop {
            let Some(mapping) = Mapping::open() else {
                thread::sleep(Duration::from_millis(500));
                continue;
            };

            if let Err(e) = mapping.validate() {
                host_log!("SkyrimSkateHost: {e}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }

            mapping.announce_host();
            host_log!("SkyrimSkateHost: connected");

            let mut world = CollisionWorld::default();
            let mut synthetic = SyntheticController::new();
            let mut last_report = Instant::now();
            let mut last_rail_build = Instant::now() - Duration::from_secs(2);
            let mut last_prune_report = Instant::now() - Duration::from_secs(10);

            loop {
                mapping.heartbeat();

                let Some(sky) = mapping.read_sky_state() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };

                // Retail Skate data is not available yet. Use the intentionally tiny
                // diagnostic controller to prove movement authority and hand-back safely.
                let input = mapping.read_input_state();
                let state = synthetic.update(&sky, input.as_ref());
                mapping.write_host_state(state);
                let pose = synthetic_pose_frame(&state, input.as_ref());
                mapping.write_pose_frame(&pose);

                match mapping.drain_collision() {
                    Ok(events) => {
                        for event in events {
                            match event {
                                CollisionEvent::Clear(epoch) => {
                                    world.clear(epoch);
                                    host_log!(
                                        "SkyrimSkateHost: collision clear epoch={epoch}"
                                    );
                                }
                                CollisionEvent::Region(region, tris) => {
                                    world.replace_region(region, tris);
                                }
                            }
                        }
                    }
                    Err(e) => host_log!("SkyrimSkateHost: collision error: {e}"),
                }

                let centre = if (state.flags & proto::HOST_ACTIVE) != 0 {
                    [state.x, state.y, state.z]
                } else {
                    [sky.x, sky.y, sky.z]
                };
                let pruned_tris = world.prune_around(centre);
                if pruned_tris > 0 && last_prune_report.elapsed() >= Duration::from_secs(2) {
                    host_log!(
                        "SkyrimSkateHost: pruned {} distant collision triangles; retained regions={} triangles={}",
                        pruned_tris,
                        world.regions.len(),
                        world.triangle_count
                    );
                    last_prune_report = Instant::now();
                }

                if world.dirty && last_rail_build.elapsed() >= Duration::from_secs(1) {
                    let rails = world.rebuild_rails();
                    host_log!(
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
                    if let Some(input) = input.as_ref() {
                        host_log!(
                            "SkyrimSkateHost: world={:#x} epoch={} mode={} regions={} triangles={} rails={} input_packet={} buttons=0x{:04x} left=({}, {}) right=({}, {}) triggers=({}, {})",
                            sky.world_id,
                            sky.collision_epoch,
                            sky.requested_mode,
                            world.regions.len(),
                            world.triangle_count,
                            world.rail_count,
                            input.packet,
                            input.buttons,
                            input.left[0],
                            input.left[1],
                            input.right[0],
                            input.right[1],
                            input.triggers[0],
                            input.triggers[1]
                        );
                    } else {
                        host_log!(
                            "SkyrimSkateHost: world={:#x} epoch={} mode={} regions={} triangles={} rails={} input=unavailable",
                            sky.world_id,
                            sky.collision_epoch,
                            sky.requested_mode,
                            world.regions.len(),
                            world.triangle_count,
                            world.rail_count
                        );
                    }
                    last_report = Instant::now();
                }

                let now = unsafe { GetTickCount64() };
                let skyrim_beat = mapping.skyrim_heartbeat();
                if skyrim_beat == 0 || now.saturating_sub(skyrim_beat) > 3000 {
                    host_log!("SkyrimSkateHost: Skyrim heartbeat lost; reconnecting");
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
