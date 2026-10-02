# SkyCraft integration design

## Existing systems to reuse

Research baseline:

- upstream: `chasmlol/SkyCraft`
- observed research commit: `bfcaf178524b92c2cdeb88e4ce0f13ef9ded6f32`

SkyCraft already provides most of the difficult Skyrim-side infrastructure.

### Exact Skyrim collision

`skse/src/Collision.*` walks the loaded Havok world, extracts exact triangles/convex primitives around the player and already emits exact triangle messages (`kColTris`) in normalized Minecraft-space coordinates.

The Skate bridge should fan out those exact triangles. It should **not** reconstruct a skate world from SkyCraft's 1/8-block voxel collision.

### External movement authority

SkyCraft already treats Minecraft as authoritative for local-player physics. The real Skyrim `PlayerCharacter` remains in the world as a puppet so NPC AI, triggers, quests, detection and combat continue to work.

Skate should become another authority state rather than introducing another Skyrim actor.

### Input and camera

SkyCraft already owns gameplay input routing while preserving Skyrim menu ownership, and already drives Skyrim's camera from an external simulation.

The intended authority states are:

```text
Skyrim/menu authority
        |
        v
Minecraft authority <---- toggle ----> Skate authority
```

Skyrim menus always preempt external gameplay input.

## Prototype architecture

Use a separate Windows process for the first version:

```text
SkyrimSE.exe
  SkyCraft.dll
    Havok collision
    input routing
    player puppet
    camera driver
    Skate bridge
         |
         | Local\\SkyCraftSkate_v1
         v
SkyrimSkateHost.exe
    Skate Session
    collision world
    rail detection
    controller input
    pose/camera output
```

A separate process keeps a Rust/Skate crash from directly taking Skyrim down and matches SkyCraft's existing cross-process design philosophy.

## Coordinate system

SkyCraft protocol space is Minecraft-style:

- X horizontal,
- Y up,
- Z horizontal,
- 1 block = 70 Skyrim units.

For the Skate bridge, start with:

- 1 SkyCraft block = 1 metre.

Keep this conversion in one module so it can be tuned after comparing push speed, wheel size, ollie height and Skyrim stairs.

## Collision strategy

The SKSE side should publish exact collision regions to both:

- Minecraft's existing collision path,
- the Skate bridge.

The Skate host keeps a bounded working collision world around the skater and rebuilds/recenters off-thread. World/cell transitions bump an epoch and invalidate old geometry.

Later, SkyCraft-placed Minecraft block collision can be merged with Skyrim triangles before rail detection.

## Safe authority handoff

When Skate activates:

1. freeze/suspend Minecraft locomotion,
2. seed the Skate session from current position/yaw,
3. let the Skate host become authoritative,
4. puppet Skyrim to returned root position,
5. use Skate camera output.

When Skate deactivates:

1. suspend Skate input,
2. preserve the final valid Skate position,
3. teleport/resync Minecraft to that position,
4. resume Minecraft input,
5. release held Skate inputs.

If the Skate host heartbeat dies or sends non-finite transforms, immediately fall back to Minecraft/Skyrim authority without moving the player back to a stale position.
