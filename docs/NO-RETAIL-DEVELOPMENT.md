# Development without Skate 3 retail data

The Skyrim integration can be developed and validated substantially before an owned Skate 3 dump is available.

## Works now without retail data

- Skyrim/SkyCraft shared-memory handshake
- heartbeats and reconnect behavior
- Skyrim player/world state transport
- exact Havok triangle streaming
- retained per-region collision cache
- world/cell epoch invalidation
- automatic grind-lip / rail discovery
- collision and rail diagnostics
- host executable packaging and CI

None of those systems need `default.xex` or the Skate 3 `data/` tree.

## Deliberately deferred

The retail-derived data becomes necessary when `skate-host::Session` is initialized for the real simulation. That includes the original game's:

- physics/settings tables,
- input/state graphs,
- animation banks,
- skater/board rig data used by the host adapter.

Until that point the bridge publishes a safe placeholder pose and **does not take movement authority**.

## Next no-retail milestone

The next useful milestone is a synthetic movement harness that exercises:

1. activation/deactivation,
2. position/camera output,
3. safe Skyrim <-> external-authority handoff,
4. collision-world updates while the external authority is active.

It will be explicitly marked as a diagnostic controller, not an approximation of Skate 3 physics, and will be replaced by the real `skate-host::Session` later.
