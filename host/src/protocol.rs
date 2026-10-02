#![allow(dead_code)]

pub const MAGIC: u32 = 0x4B53_4B53; // "SKSK"
pub const VERSION: u32 = 1;
pub const MAPPING_NAME: &str = "Local\\SkyCraftSkate_v1";

pub const OFF_HEADER: usize = 0x0000;
pub const OFF_SKY_STATE: usize = 0x0100;
pub const OFF_HOST_STATE: usize = 0x0200;
pub const OFF_COLLISION_RING: usize = 0x1000;
pub const COLLISION_RING_BYTES: usize = 32 << 20;
pub const MAPPING_BYTES: usize = OFF_COLLISION_RING + COLLISION_RING_BYTES;

pub const SKY_IN_GAME: u32 = 1 << 0;
pub const SKY_MENU_OPEN: u32 = 1 << 1;
pub const SKY_LOADING: u32 = 1 << 2;
pub const SKY_SKATE_REQUESTED: u32 = 1 << 3;
pub const SKY_MINECRAFT_PRESENT: u32 = 1 << 4;

pub const HOST_READY: u32 = 1 << 0;
pub const HOST_SKATE_ACTIVE: u32 = 1 << 1;
pub const HOST_HAS_POSE: u32 = 1 << 2;
pub const HOST_HAS_CAMERA: u32 = 1 << 3;
pub const HOST_ERROR: u32 = 1 << 4;

pub const RING_HEAD_OFF: usize = 0x00;
pub const RING_TAIL_OFF: usize = 0x40;
pub const RING_DATA_OFF: usize = 0x80;
pub const RING_DATA_BYTES: usize = COLLISION_RING_BYTES - RING_DATA_OFF;

pub const COLLISION_CLEAR: u32 = 1;
pub const COLLISION_TRIANGLES: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub magic: u32,
    pub version: u32,
    pub skyrim_pid: u32,
    pub host_pid: u32,
    pub skyrim_heartbeat_ms: u64,
    pub host_heartbeat_ms: u64,
    pub reserved: [u8; 0x40 - 0x20],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SkyState {
    pub seq: u32,
    pub flags: u32,
    pub collision_epoch: u32,
    pub toggle_seq: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub aspect_ratio: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub reserved: [u8; 0x80 - 0x3C],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostState {
    pub seq: u32,
    pub flags: u32,
    pub collision_epoch: u32,
    pub toggle_ack: u32,
    pub tick: u64,
    pub root: [f32; 16],
    pub velocity: [f32; 3],
    pub camera_pos: [f32; 3],
    pub camera_basis: [f32; 9],
    pub fov_deg: f32,
    pub state: [u8; 32],
    pub reserved: [u8; 0xC0 - 0xB8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CollisionMessageHeader {
    pub bytes: u32,
    pub kind: u32,
    pub epoch: u32,
    pub sequence: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TriangleBatchHeader {
    pub region_x: i32,
    pub region_y: i32,
    pub region_z: i32,
    pub count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Triangle {
    pub v: [f32; 9],
    pub flags: u32,
}

const _: () = assert!(core::mem::size_of::<Header>() == 0x40);
const _: () = assert!(core::mem::size_of::<SkyState>() == 0x80);
const _: () = assert!(core::mem::size_of::<HostState>() == 0xC0);
const _: () = assert!(core::mem::size_of::<CollisionMessageHeader>() == 16);
const _: () = assert!(core::mem::size_of::<TriangleBatchHeader>() == 16);
const _: () = assert!(core::mem::size_of::<Triangle>() == 40);

pub fn finite_root(root: &[f32; 16]) -> bool {
    root.iter().all(|v| v.is_finite())
}

pub fn finite_triangle(t: &Triangle) -> bool {
    t.v.iter().all(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_sizes_match_cpp_contract() {
        assert_eq!(core::mem::size_of::<Header>(), 0x40);
        assert_eq!(core::mem::size_of::<SkyState>(), 0x80);
        assert_eq!(core::mem::size_of::<HostState>(), 0xC0);
        assert_eq!(core::mem::size_of::<Triangle>(), 40);
    }
}
