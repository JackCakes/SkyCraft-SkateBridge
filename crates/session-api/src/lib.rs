//! Stable adapter boundary between SkyrimSkateHost and a skating session.
//!
//! The shape intentionally follows the proven IW4L Skate adapter:
//! install collision/rails, activate at a spawn, collect controller input,
//! advance, and publish root/bones/camera/velocity.  No retail data or
//! reverse-engineered game assets live in this crate.

pub const MAX_BONES: usize = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControllerFrame {
    pub buttons: u16,
    pub triggers: [u8; 2],
    pub left: [i16; 2],
    pub right: [i16; 2],
    pub packet: u32,
}

#[derive(Clone, Debug, Default)]
pub struct WorldGeometry {
    pub revision: u64,
    /// Session-space triangles. The Skyrim adapter currently treats one
    /// SkyCraft block as one metre and keeps Y as up.
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Grind rails as polylines in the same coordinate space.
    pub rails: Vec<Vec<[f32; 3]>>,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraPose {
    pub position: [f32; 3],
    /// Orthonormal forward/up basis in session space.
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub fov_deg: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BoneTransform {
    pub name_hash: u64,
    pub matrix: [f32; 16],
}

#[derive(Clone, Debug)]
pub struct SessionPose {
    /// Column-major skater root transform in session space.
    pub root: [f32; 16],
    pub bones: Vec<BoneTransform>,
    pub camera: Option<CameraPose>,
    pub velocity: [f32; 3],
    pub tick: u64,
    /// Compact diagnostics only; presentation may map this to a readable state.
    pub state_code: u32,
}

impl SessionPose {
    pub fn position(&self) -> [f32; 3] {
        [self.root[12], self.root[13], self.root[14]]
    }

    pub fn finite(&self) -> bool {
        self.root.iter().all(|v| v.is_finite())
            && self.velocity.iter().all(|v| v.is_finite())
            && self.bones.len() <= MAX_BONES
            && self
                .bones
                .iter()
                .all(|b| b.name_hash != 0 && b.matrix.iter().all(|v| v.is_finite()))
            && self.camera.is_none_or(|c| {
                c.position.iter().all(|v| v.is_finite())
                    && c.forward.iter().all(|v| v.is_finite())
                    && c.up.iter().all(|v| v.is_finite())
                    && c.fov_deg.is_finite()
                    && (1.0..179.0).contains(&c.fov_deg)
            })
    }
}

/// Backend implemented by either the diagnostic harness or the real Skate
/// session adapter. Authority/liveness policy remains outside this trait.
pub trait SessionBackend {
    fn install_world(&mut self, world: WorldGeometry) -> Result<(), String>;

    fn activate(
        &mut self,
        spawn: [f32; 3],
        heading_radians: f32,
        aspect_ratio: f32,
    ) -> Result<SessionPose, String>;

    fn step(
        &mut self,
        input: ControllerFrame,
        frame_seconds: f32,
        aspect_ratio: f32,
    ) -> Result<SessionPose, String>;

    fn suspend(&mut self);
}

pub fn fnv1a64(name: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    #[test]
    fn pose_validation_accepts_finite_named_bones() {
        let pose = SessionPose {
            root: identity(),
            bones: vec![BoneTransform {
                name_hash: fnv1a64("root"),
                matrix: identity(),
            }],
            camera: Some(CameraPose {
                position: [0.0, 1.0, -2.0],
                forward: [0.0, 0.0, 1.0],
                up: [0.0, 1.0, 0.0],
                fov_deg: 70.0,
            }),
            velocity: [0.0; 3],
            tick: 1,
            state_code: 0,
        };
        assert!(pose.finite());
    }

    #[test]
    fn pose_validation_rejects_nonfinite_output() {
        let mut root = identity();
        root[12] = f32::NAN;
        let pose = SessionPose {
            root,
            bones: Vec::new(),
            camera: None,
            velocity: [0.0; 3],
            tick: 1,
            state_code: 0,
        };
        assert!(!pose.finite());
    }
}
