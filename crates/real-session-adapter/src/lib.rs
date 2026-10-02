//! Adapter from the pinned IW4L Skate session to SkyCraft's stable session API.
//!
//! This crate contains no Skate 3 retail data. Construction succeeds at runtime
//! only when the caller supplies a locally prepared data root from their own copy.

use skate_host::bridge::{InputFrame, Session};
use skycraft_skate_session_api as api;
use std::path::{Path, PathBuf};

pub struct RealSkateSession {
    data_root: PathBuf,
    session: Option<Session>,
    pending_world: Option<api::WorldGeometry>,
    installed_revision: u64,
    accumulated: f32,
}

impl RealSkateSession {
    /// Accept either the converter's output folder or its nested `assets` folder,
    /// and verify the stable minimum contract before the heavy Session load begins.
    pub fn resolve_data_root(path: &Path) -> Result<PathBuf, String> {
        let candidates = [path.to_path_buf(), path.join("assets")];
        let required = [
            "private/skater.glb",
            "private/game.json",
            "private/stock/physics-skeletons.json",
            "private/stock/skater-collections.json",
            "private/stock/data/config/input.cfg",
        ];
        for candidate in candidates {
            if required.iter().all(|relative| candidate.join(relative).is_file()) {
                return Ok(candidate);
            }
        }
        Err(format!(
            "prepared Skate data is incomplete at {} (expected private/game.json, skater.glb, stock physics/collections, and stock input.cfg; point SKYRIM_SKATE_DATA at the converter output or its assets folder)",
            path.display()
        ))
    }

    pub fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            session: None,
            pending_world: None,
            installed_revision: 0,
            accumulated: 0.0,
        }
    }

    pub fn preload(&self) -> Result<(), String> {
        Session::preload(&self.data_root)
    }

    fn convert_pose(pose: skate_host::bridge::Pose) -> api::SessionPose {
        let bones = pose
            .names
            .iter()
            .zip(pose.bones.iter())
            .take(api::MAX_BONES)
            .map(|(name, matrix)| api::BoneTransform {
                name_hash: api::fnv1a64(name),
                matrix: matrix.to_cols_array(),
            })
            .collect();

        let camera = pose.camera.map(|(position, basis, fov_deg)| api::CameraPose {
            position: position.to_array(),
            forward: basis.z_axis.to_array(),
            up: basis.y_axis.to_array(),
            fov_deg,
        });

        api::SessionPose {
            root: pose.root.to_cols_array(),
            bones,
            camera,
            velocity: pose.velocity.to_array(),
            tick: pose.tick,
            state_code: api::fnv1a64(&pose.state) as u32,
        }
    }

    fn input_frame(input: api::ControllerFrame) -> InputFrame {
        InputFrame::from_pad(
            input.buttons,
            input.triggers,
            input.left,
            input.right,
            input.packet,
        )
    }
}

impl api::SessionBackend for RealSkateSession {
    fn install_world(&mut self, world: api::WorldGeometry) -> Result<(), String> {
        if world.revision == self.installed_revision {
            return Ok(());
        }

        if let Some(session) = self.session.as_mut() {
            let builder = session.collision_builder();
            let prepared = builder.build(world.triangles, world.rails)?;
            session.install_collision(prepared)?;
            self.installed_revision = world.revision;
        } else {
            self.installed_revision = world.revision;
            self.pending_world = Some(world);
        }
        Ok(())
    }

    fn activate(
        &mut self,
        spawn: [f32; 3],
        heading_radians: f32,
        aspect_ratio: f32,
    ) -> Result<api::SessionPose, String> {
        if self.session.is_none() {
            let world = self
                .pending_world
                .take()
                .ok_or("real Skate session has no installed collision world")?;
            let mut session = Session::new(
                &self.data_root,
                world.triangles,
                world.rails,
                spawn,
                heading_radians,
            )?;
            session.set_aspect_ratio(aspect_ratio);
            self.session = Some(session);
        }

        let session = self.session.as_mut().expect("session initialized above");
        session.set_aspect_ratio(aspect_ratio);
        self.accumulated = 0.0;
        let pose = Self::convert_pose(session.activate(spawn, heading_radians)?);
        if !pose.finite() {
            return Err("real Skate session returned a non-finite activation pose".into());
        }
        Ok(pose)
    }

    fn step(
        &mut self,
        input: api::ControllerFrame,
        frame_seconds: f32,
        aspect_ratio: f32,
    ) -> Result<api::SessionPose, String> {
        let session = self
            .session
            .as_mut()
            .ok_or("real Skate session is not activated")?;

        session.set_aspect_ratio(aspect_ratio);
        let dt = frame_seconds.clamp(0.0, 0.1);
        session.collect(Self::input_frame(input), dt);
        self.accumulated = (self.accumulated + dt).min(0.15);

        while self.accumulated >= session.period() {
            self.accumulated -= session.period();
            session.advance()?;
        }

        let pose = Self::convert_pose(session.pose());
        if !pose.finite() {
            return Err("real Skate session returned a non-finite pose".into());
        }
        Ok(pose)
    }

    fn suspend(&mut self) {
        self.accumulated = 0.0;
        if let Some(session) = self.session.as_mut() {
            session.suspend_input();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_data_root_fails_before_session_load() {
        let missing = std::env::temp_dir().join(format!(
            "skycraft-skate-missing-{}",
            std::process::id()
        ));
        let error = RealSkateSession::resolve_data_root(&missing).unwrap_err();
        assert!(error.contains("prepared Skate data is incomplete"));
    }

    #[test]
    fn construction_does_not_touch_retail_data() {
        let adapter = RealSkateSession::new(PathBuf::from("not-present-in-ci"));
        assert!(adapter.session.is_none());
        assert!(adapter.pending_world.is_none());
    }
}
