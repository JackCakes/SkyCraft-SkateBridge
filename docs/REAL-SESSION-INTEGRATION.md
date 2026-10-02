# Real Skate Session integration boundary

This document defines the next replacement point after the synthetic harness.

## Reference implementation

The architectural reference is `chasmlol/2010-rust-rewrite-mashup` at research snapshot
`f608f85e407ff1b7689d54a9aafdd16e95711ac4`.

Its Skate worker uses:

- `Session::new(root, triangles, rails, spawn, heading)`
- `Session::install_collision(...)`
- `Session::activate(spawn, heading)`
- `Session::collect(InputFrame, dt)`
- repeated `Session::advance()`
- `Session::pose()`

The returned pose contains root transform, named bone transforms, camera, velocity,
simulation tick and state.

## SkyCraft adapter

`crates/session-api` is intentionally shaped around that loop.

The host already prepares and retains:

- bounded nearby Skyrim gameplay collision,
- exact session-ready triangle arrays,
- automatic grind rail polylines,
- XInput-shaped controller state,
- viewport aspect,
- activation position/yaw,
- root/camera/velocity output transport,
- named skeletal pose transport.

A real backend therefore replaces the synthetic backend without changing the
Skyrim shared-memory protocol or authority state machine.

## Coordinate boundary

SkyCraft bridge coordinates are X horizontal, Y up, Z horizontal. For the first
real-session integration, one SkyCraft block remains one metre. Any correction
to scale or handedness belongs in the real-session adapter only; SKSE collision
extraction and the shared-memory ABI stay unchanged.

## Retail-data boundary

No Skate 3 retail-derived files belong in Git. The real backend receives a local
prepared-data root produced from the user's own game copy, matching the reference
mashup's separation between source code and generated `skate-data`.

## Failure boundary

The real backend must return an error rather than emit a non-finite pose.
Skyrim authority remains outside the backend: host loss, bad transforms,
menus/loading, death, scripted takeover and world changes all fall back through
the already-tested SkyCraft handoff.
