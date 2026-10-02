//! Shared-memory ABI for Skyrim/SkyCraft <-> SkyrimSkateHost.
//!
//! This is intentionally small and fixed-layout. The C++ mirror in the
//! SkyCraft fork must stay byte-for-byte compatible with this file.

use core::mem::size_of;

pub const MAPPING_NAME: &str = r"Local\SkyCraftSkate_v1";
pub const MAGIC: u32 = 0x4B53_4353; // bytes: "SCSK"
pub const VERSION: u32 = 3;

pub const OFF_HEADER: usize = 0x0000;
pub const OFF_SKY_STATE: usize = 0x0100;
pub const OFF_SKATE_STATE: usize = 0x0200;
pub const OFF_INPUT_STATE: usize = 0x0300;
pub const OFF_COLLISION_RING: usize = 0x1000;

pub const COLLISION_RING_BYTES: usize = 16 << 20;
pub const OFF_POSE_FRAME: usize = OFF_COLLISION_RING + COLLISION_RING_BYTES;
pub const POSE_FRAME_BYTES: usize = 0x4000;
pub const COLLISION_RING_HEAD_OFF: usize = 0x00;
pub const COLLISION_RING_TAIL_OFF: usize = 0x40;
pub const COLLISION_RING_DATA_OFF: usize = 0x80;
pub const COLLISION_RING_DATA_BYTES: usize = COLLISION_RING_BYTES - COLLISION_RING_DATA_OFF;

pub const MAPPING_BYTES: usize = OFF_POSE_FRAME + POSE_FRAME_BYTES;

pub const SKY_IN_GAME: u32 = 1 << 0;
pub const SKY_MENU_OPEN: u32 = 1 << 1;
pub const SKY_LOADING: u32 = 1 << 2;

pub const MODE_MINECRAFT: u32 = 0;
pub const MODE_SKATE: u32 = 1;

pub const HOST_READY: u32 = 1 << 0;
pub const HOST_ACTIVE: u32 = 1 << 1;
pub const HOST_ERROR: u32 = 1 << 2;
pub const HOST_ON_GROUND: u32 = 1 << 3;
pub const HOST_GRINDING: u32 = 1 << 4;
pub const HOST_MANUAL: u32 = 1 << 5;
pub const HOST_BAIL: u32 = 1 << 6;
pub const HOST_CAMERA_VALID: u32 = 1 << 7;

pub const INPUT_VALID: u32 = 1 << 0;
pub const INPUT_KEYBOARD_FALLBACK: u32 = 1 << 1;

pub const POSE_VALID: u32 = 1 << 0;
pub const MAX_POSE_BONES: usize = 128;

pub const COL_PAD: u32 = 0;
pub const COL_CLEAR: u32 = 1;
pub const COL_REGION_TRIS: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Header {
    pub magic: u32,
    pub version: u32,
    pub skyrim_pid: u32,
    pub host_pid: u32,
    pub skyrim_heartbeat_ms: u64,
    pub host_heartbeat_ms: u64,
    pub mapping_bytes: u64,
    pub reserved: [u64; 3],
}
const _: [(); 64] = [(); size_of::<Header>()];

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct SkyState {
    /// Seqlock: odd while Skyrim writes, even when stable.
    pub seq: u32,
    pub flags: u32,
    pub world_id: u32,
    pub collision_epoch: u32,

    /// Feet position in SkyCraft MC-space: blocks, Y up.
    pub x: f64,
    pub y: f64,
    pub z: f64,

    /// Minecraft-style yaw degrees.
    pub yaw: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub aspect: f32,

    /// MODE_* requested by Skyrim input/state machine.
    pub requested_mode: u32,
}
const _: [(); 64] = [(); size_of::<SkyState>()];

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct SkateState {
    /// Seqlock: odd while host writes, even when stable.
    pub seq: u32,
    pub flags: u32,
    pub error_code: u32,
    pub reserved0: u32,

    /// Skater root in SkyCraft MC-space so the Skyrim side can reuse McToSky.
    pub x: f64,
    pub y: f64,
    pub z: f64,

    /// Root orientation quaternion in SkyCraft MC-space.
    pub quat: [f32; 4],

    /// Root velocity, blocks/second.
    pub velocity: [f32; 3],
    pub reserved1: u32,

    /// Camera in SkyCraft MC-space.
    pub camera_pos: [f64; 3],
    pub camera_forward: [f32; 3],
    pub camera_up: [f32; 3],
    pub fov_deg: f32,

    /// Runtime-defined compact state id for diagnostics.
    pub state_code: u32,
}
const _: [(); 128] = [(); size_of::<SkateState>()];

/// XInput-shaped controller snapshot from Skyrim. The raw layout intentionally
/// matches what the retail-backed Skate Session eventually consumes.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct InputState {
    /// Seqlock: odd while Skyrim writes, even when stable.
    pub seq: u32,
    pub flags: u32,
    pub buttons: u16,
    pub triggers: [u8; 2],
    pub left: [i16; 2],
    pub right: [i16; 2],
    pub packet: u32,
    pub frame_seconds: f32,
    pub reserved: [u32; 9],
}
const _: [(); 64] = [(); size_of::<InputState>()];

/// One named Skate bone transform. Names are represented by stable FNV-1a
/// hashes so high-rate pose frames never need to copy variable-length strings.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct BonePose {
    pub name_hash: u64,
    /// Column-major 4x4 transform in the Skate session's pose space.
    pub matrix: [f32; 16],
}
const _: [(); 72] = [(); size_of::<BonePose>()];

/// High-rate host -> Skyrim animation pose, separate from SkateState so the
/// root/camera control path stays tiny and robust.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PoseFrame {
    /// Seqlock: odd while host writes, even when stable.
    pub seq: u32,
    pub flags: u32,
    pub tick: u64,
    pub bone_count: u32,
    /// Changes if the ordered bone-name set changes.
    pub name_set_id: u32,
    pub reserved: [u32; 2],
    pub bones: [BonePose; MAX_POSE_BONES],
}
impl Default for PoseFrame {
    fn default() -> Self {
        Self {
            seq: 0,
            flags: 0,
            tick: 0,
            bone_count: 0,
            name_set_id: 0,
            reserved: [0; 2],
            bones: [BonePose::default(); MAX_POSE_BONES],
        }
    }
}
const _: [(); 9248] = [(); size_of::<PoseFrame>()];
const _: () = assert!(size_of::<PoseFrame>() <= POSE_FRAME_BYTES);

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CollisionMessageHeader {
    pub kind: u32,
    pub payload_bytes: u32,
}
const _: [(); 8] = [(); size_of::<CollisionMessageHeader>()];

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CollisionRegion {
    pub rx: i32,
    pub ry: i32,
    pub rz: i32,
    pub count: u32,
    pub epoch: u32,
    pub world_id: u32,
}
const _: [(); 24] = [(); size_of::<CollisionRegion>()];

/// Exact Skyrim collision triangle copied from SkyCraft's MC-space triangle stream.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CollisionTri {
    pub v: [f32; 9],
    pub flags: u32,
}
const _: [(); 40] = [(); size_of::<CollisionTri>()];

pub fn align8(n: usize) -> usize {
    (n + 7) & !7
}
