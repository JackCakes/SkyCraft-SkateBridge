use skycraft_skate_protocol as proto;
use skycraft_skate_real_session::RealSkateSession;
use skycraft_skate_session_api as session;
use session::SessionBackend;
use std::path::PathBuf;

pub struct RealController {
    backend: RealSkateSession,
    active: bool,
    blocked_until_release: bool,
    world_id: u32,
    epoch: u32,
    installed_revision: u64,
    error_code: u32,
    notice: Option<String>,
}

impl RealController {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            backend: RealSkateSession::new(data_root),
            active: false,
            blocked_until_release: false,
            world_id: 0,
            epoch: 0,
            installed_revision: 0,
            error_code: 0,
            notice: None,
        }
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    fn safe_sky(sky: &proto::SkyState) -> bool {
        let blocked = proto::SKY_MENU_OPEN | proto::SKY_LOADING;
        (sky.flags & proto::SKY_IN_GAME) != 0
            && (sky.flags & blocked) == 0
            && sky.x.is_finite()
            && sky.y.is_finite()
            && sky.z.is_finite()
            && sky.yaw.is_finite()
            && sky.aspect.is_finite()
            && sky.aspect > 0.0
            && sky.world_id != 0
    }

    fn stop(&mut self, reason: &str, block_until_release: bool) {
        if self.active {
            self.backend.suspend();
            self.notice = Some(format!("real Skate authority released ({reason})"));
        }
        self.active = false;
        self.blocked_until_release |= block_until_release;
    }

    fn fail(&mut self, message: String) {
        self.backend.suspend();
        self.active = false;
        self.blocked_until_release = true;
        self.error_code = session::fnv1a64(&message) as u32;
        self.notice = Some(format!("real Skate backend error: {message}"));
    }

    fn idle_state(&self, sky: &proto::SkyState) -> proto::SkateState {
        let mut state = proto::SkateState::default();
        state.flags = proto::HOST_READY;
        if self.error_code != 0 {
            state.flags |= proto::HOST_ERROR;
            state.error_code = self.error_code;
        }
        state.x = sky.x;
        state.y = sky.y;
        state.z = sky.z;
        state.quat = [0.0, 0.0, 0.0, 1.0];
        state
    }

    fn controller_frame(input: Option<&proto::InputState>) -> session::ControllerFrame {
        let Some(input) = input.filter(|input| (input.flags & proto::INPUT_VALID) != 0) else {
            return session::ControllerFrame::default();
        };
        session::ControllerFrame {
            buttons: input.buttons,
            triggers: input.triggers,
            left: input.left,
            right: input.right,
            packet: input.packet,
        }
    }

    fn root_quat(root: &[f32; 16]) -> [f32; 4] {
        // Column-major affine matrix -> normalized quaternion.
        let m00 = root[0];
        let m11 = root[5];
        let m22 = root[10];
        let (x, y, z, w) = if m00 + m11 + m22 > 0.0 {
            let s = (m00 + m11 + m22 + 1.0).sqrt() * 2.0;
            ((root[6] - root[9]) / s, (root[8] - root[2]) / s, (root[1] - root[4]) / s, 0.25 * s)
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            (0.25 * s, (root[4] + root[1]) / s, (root[8] + root[2]) / s, (root[6] - root[9]) / s)
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            ((root[4] + root[1]) / s, 0.25 * s, (root[9] + root[6]) / s, (root[8] - root[2]) / s)
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            ((root[8] + root[2]) / s, (root[9] + root[6]) / s, 0.25 * s, (root[1] - root[4]) / s)
        };
        let len = (x * x + y * y + z * z + w * w).sqrt();
        if len.is_finite() && len > 1.0e-6 {
            [x / len, y / len, z / len, w / len]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        }
    }

    fn protocol_pose(pose: &session::SessionPose) -> (proto::SkateState, proto::PoseFrame) {
        let mut state = proto::SkateState::default();
        state.flags = proto::HOST_READY | proto::HOST_ACTIVE;
        let position = pose.position();
        state.x = f64::from(position[0]);
        state.y = f64::from(position[1]);
        state.z = f64::from(position[2]);
        state.quat = Self::root_quat(&pose.root);
        state.velocity = pose.velocity;
        state.state_code = pose.state_code;

        if let Some(camera) = pose.camera {
            state.flags |= proto::HOST_CAMERA_VALID;
            state.camera_pos = camera.position.map(f64::from);
            state.camera_forward = camera.forward;
            state.camera_up = camera.up;
            state.fov_deg = camera.fov_deg;
        }

        let mut frame = proto::PoseFrame::default();
        frame.flags = proto::POSE_VALID;
        frame.tick = pose.tick;
        frame.bone_count = pose.bones.len().min(proto::MAX_POSE_BONES) as u32;

        let mut name_set = 0x811c9dc5_u32;
        for (dst, src) in frame
            .bones
            .iter_mut()
            .zip(pose.bones.iter())
            .take(frame.bone_count as usize)
        {
            dst.name_hash = src.name_hash;
            dst.matrix = src.matrix;
            for byte in src.name_hash.to_le_bytes() {
                name_set ^= u32::from(byte);
                name_set = name_set.wrapping_mul(0x01000193);
            }
        }
        frame.name_set_id = name_set;
        (state, frame)
    }

    fn install_if_needed(&mut self, world: &session::WorldGeometry) -> Result<(), String> {
        if world.triangles.is_empty() {
            return Err("Skyrim collision working set is still empty".into());
        }
        if world.revision == self.installed_revision {
            return Ok(());
        }
        self.backend.install_world(world.clone())?;
        self.installed_revision = world.revision;
        self.notice = Some(format!(
            "real Skate collision installed revision {} ({} triangles, {} rails)",
            world.revision,
            world.triangles.len(),
            world.rails.len()
        ));
        Ok(())
    }

    pub fn update(
        &mut self,
        sky: &proto::SkyState,
        input: Option<&proto::InputState>,
        world: &session::WorldGeometry,
        host_dt: f32,
    ) -> (proto::SkateState, proto::PoseFrame) {
        let requested = sky.requested_mode == proto::MODE_SKATE;

        if !requested {
            self.stop("mode returned to Minecraft", false);
            self.blocked_until_release = false;
            self.error_code = 0;
            return (self.idle_state(sky), proto::PoseFrame::default());
        }

        if self.active && !Self::safe_sky(sky) {
            self.stop("Skyrim state is not safe", true);
        } else if self.active && (self.world_id != sky.world_id || self.epoch != sky.collision_epoch) {
            self.stop("world/epoch changed", true);
        }

        if self.blocked_until_release {
            return (self.idle_state(sky), proto::PoseFrame::default());
        }

        if !Self::safe_sky(sky) {
            return (self.idle_state(sky), proto::PoseFrame::default());
        }

        if !self.active {
            if let Err(error) = self.install_if_needed(world) {
                // Collision streaming is allowed to be briefly empty while entering
                // a cell. Wait silently rather than latching a permanent error.
                if world.triangles.is_empty() {
                    return (self.idle_state(sky), proto::PoseFrame::default());
                }
                self.fail(error);
                return (self.idle_state(sky), proto::PoseFrame::default());
            }
            let spawn = [sky.x as f32, sky.y as f32, sky.z as f32];
            let heading = session::mc_yaw_to_session_heading(sky.yaw);
            match self.backend.activate(spawn, heading, sky.aspect) {
                Ok(pose) => {
                    self.active = true;
                    self.world_id = sky.world_id;
                    self.epoch = sky.collision_epoch;
                    self.notice = Some(format!(
                        "real Skate authority acquired world={:#x} epoch={} spawn=({:.3},{:.3},{:.3}) yaw={:.1}",
                        self.world_id, self.epoch, sky.x, sky.y, sky.z, sky.yaw
                    ));
                    return Self::protocol_pose(&pose);
                }
                Err(error) => {
                    self.fail(error);
                    return (self.idle_state(sky), proto::PoseFrame::default());
                }
            }
        }

        if let Err(error) = self.install_if_needed(world) {
            self.fail(error);
            return (self.idle_state(sky), proto::PoseFrame::default());
        }

        let controller = Self::controller_frame(input);
        let dt = input
            .filter(|input| (input.flags & proto::INPUT_VALID) != 0)
            .map(|input| input.frame_seconds)
            .filter(|dt| dt.is_finite() && *dt > 0.0)
            .unwrap_or(host_dt)
            .clamp(0.0, 0.1);

        match self.backend.step(controller, dt, sky.aspect) {
            Ok(pose) => Self::protocol_pose(&pose),
            Err(error) => {
                self.fail(error);
                (self.idle_state(sky), proto::PoseFrame::default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_to_quaternion_identity_is_identity() {
        let root = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let q = RealController::root_quat(&root);
        assert!((q[0]).abs() < 1.0e-6);
        assert!((q[1]).abs() < 1.0e-6);
        assert!((q[2]).abs() < 1.0e-6);
        assert!((q[3] - 1.0).abs() < 1.0e-6);
    }
}
