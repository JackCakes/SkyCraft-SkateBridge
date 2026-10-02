# Research sources

Research date: 2026-10-02.

## SkyCraft

Repository: https://github.com/chasmlol/SkyCraft

Observed research commit: `bfcaf178524b92c2cdeb88e4ce0f13ef9ded6f32`

Important files inspected:

- `README.md`
- `docs/DESIGN.md`
- `skse/src/Collision.h`
- `skse/src/Collision.cpp`
- `skse/src/Game.cpp`
- `protocol/skycraft_protocol.h`

Findings:

- Skyrim and Minecraft run as separate games joined by shared memory.
- Minecraft can already be authoritative for the Skyrim player's movement/camera.
- exact Skyrim Havok triangles are already extracted and streamed.
- SkyCraft has an exact-triangle collision message (`kColTris`).

## 2010 Rust Rewrite Mashup

Repository: https://github.com/chasmlol/2010-rust-rewrite-mashup

Observed research commit: `f608f85e407ff1b7689d54a9aafdd16e95711ac4`

Important files inspected:

- `README.md`
- `docs/SKATE.md`
- `crates/render_anim/src/skate.rs`
- `crates/render_anim/src/skate/collision.rs`
- `crates/render_anim/src/skate/rails.rs`
- `crates/frame/src/skate.rs`
- `skate/converter/iw4l_skate_convert.py`

Findings:

- Skate simulation remains authoritative while skating.
- arbitrary host collision is converted into Skate collision,
- automatic grind rails are inferred from collision,
- Minecraft voxel collision is dynamically rebuilt around the skater,
- owned Skate 3 data is converted locally and not shipped.

The mashup's `docs/SKATE.md` identifies `SK8-ENGINE/skate-3-rust-engine` commit `cb79689` as the Skate simulation snapshot used by that integration.

## Skate 3 Rust Engine

Repository: https://github.com/SK8-ENGINE/skate-3-rust-engine

Current project scope includes skating, tricks, grinds, offboard movement and custom map/collision support. Before copying substantial code directly, audit the exact pinned revision's license/NOTICE state. Prefer linking to or depending on upstream code when practical.
