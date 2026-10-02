# Skate engine research

Research snapshot: 2026-10-02.

## The repository supplied by the project owner

Primary engine:

- https://github.com/SK8-ENGINE/skate-3-rust-engine
- current inspected commit: `60efdef86600d8d8d4feb4b7c608fa0efd0643d7`
- mashup-pinned source revision: `cb7968930f14dad38457e98720d1a274e469eec2`

The upstream engine contains the reconstructed Skate systems themselves: board/world contact, player state, tricks, grinds, off-board movement, cameras, animation, scoring and owned-game-data loaders.

## Important discovery: the mashup adds a host adapter

The exact `Session` API used by the MW2/Minecraft mashup is **not** a public crate in the upstream engine workspace at the pinned revision. The mashup carries an extra crate:

`skate/crates/skate-host`

Its `physics/bridge.rs` wraps the engine internals into a small embeddable API:

- `Session::new(root, triangles, rails, spawn, heading)`
- `Session::collision_builder()`
- `Session::install_collision(...)`
- `Session::activate(...)`
- `Session::collect(input, dt)`
- `Session::advance()`
- `Session::pose()`
- `Session::suspend_input()`

The returned pose contains:

- skater root transform,
- full bone transforms + names,
- camera position/basis/FOV,
- board velocity,
- physics tick,
- current Skate state.

This is almost exactly the boundary Skyrim needs.

## What the host adapter does

The wrapper converts arbitrary triangles + rail polylines into a temporary `SkateMap`, builds a `BoardWorld`, creates a grind provider, loads the user's converted retail settings/graphs/animation data, and runs fixed-step skating.

It also exposes `CollisionBuilder` so collision can be rebuilt on another thread and atomically installed while the skater is running. This is how the Minecraft world version handles changing block collision.

## Consequence for Skyrim integration

Do **not** try to embed the entire Bevy Skate game application inside Skyrim.

The practical route is:

1. pin the Skate engine revision used by the proven mashup,
2. reuse/adapt the mashup's `skate-host` wrapper,
3. run that wrapper in a separate `SkyrimSkateHost.exe`,
4. feed it exact SkyCraft Havok triangles + generated rail polylines,
5. send the resulting root pose/camera back to the SKSE side.

This keeps the integration surface narrow and lets us update the core engine independently later.

## Source-data boundary

The host wrapper expects already converted private assets. The retail Skate 3 files remain user-supplied and local. No `default.xex`, disc image, or converted private asset should ever be committed to this repository.
