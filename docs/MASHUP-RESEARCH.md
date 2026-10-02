# Research: how Skate mode works in the MW2/Minecraft mashup

Research snapshot: 2026-10-02.

## Source baseline

Primary implementation source:

- `chasmlol/2010-rust-rewrite-mashup`
- observed research commit: `f608f85e407ff1b7689d54a9aafdd16e95711ac4`

Key files:

- `docs/SKATE.md`
- `crates/render_anim/src/skate.rs`
- `crates/render_anim/src/skate/collision.rs`
- `crates/render_anim/src/skate/rails.rs`
- `crates/frame/src/skate.rs`
- `skate/converter/iw4l_skate_convert.py`

## What it actually does

The mashup does **not** approximate Skate movement with MW2 movement. While Skate mode is active, a Skate simulation session owns skating physics. The host runtime gives that simulation world collision and controller input, then consumes the returned root pose, bones and camera.

The host-side worker protocol is deliberately small:

- `Activate(epoch, spawn, yaw, aspect)`
- `Step(epoch, dt, input, aspect)`
- `Suspend`

Replies include:

- `Ready`
- `Activated(epoch, Pose, load_ms)`
- `Pose(epoch, Pose)`
- `Error`

The map session is retained when skating is toggled off so decoded animation/state data and collision do not need to be rebuilt every time.

## Collision handoff

The adapter uses **gameplay collision**, not render meshes. It extracts solid triangles from MW2 clip meshes, brushes and static-model collision, filters bad/duplicate triangles, converts coordinate systems and units, then builds Skate collision.

That is the critical precedent for Skyrim: SkyCraft already walks Skyrim's Havok collision world, so Skyrim does not need a render-mesh conversion layer.

## Automatic grind rails

The mashup can skate arbitrary MW2 maps without authored Skate rails.

Its rail detector:

1. considers edges of sufficiently upward/walkable faces,
2. samples along each edge,
3. probes just outside the edge,
4. rejects it when ground continues immediately outside,
5. rejects it when a wall rises immediately outside,
6. keeps surviving edges as lips,
7. merges collinear fragments across seams/T-junctions,
8. chains gentle turns into polylines,
9. rejects very short/steep rails.

Current MW2-side thresholds are host-unit specific and must not be copied numerically into Skyrim without conversion.

## Minecraft proves dynamic geometry also works

The same mashup lets Skate mode operate inside its Minecraft world. Nearby block collision is converted to triangles around the skater and rebuilt as the voxel world streams or changes. The same automatic rail detector is run on those block faces.

That is especially relevant to SkyCraft because Skyrim collision is already streamed around the player and Minecraft blocks can later be merged into the Skate world too.

## Owned Skate 3 data

The mashup does not ship Skate 3 assets. The user selects an extracted owned `default.xex` with its `data/` folder. A converter extracts only the runtime data needed by the skating implementation into a local generated folder.

This repo should preserve the same boundary: source code and adapters are versioned; retail-derived assets never are.
