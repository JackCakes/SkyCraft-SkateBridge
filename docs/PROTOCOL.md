# Live bridge protocol v1

Mapping: `Local\\SkyCraftSkate_v1`

The prototype keeps Skate transport separate from SkyCraft's existing Minecraft mapping so early experiments cannot destabilize the working Minecraft protocol.

## Fixed sections

- `0x0000`: 64-byte header
- `0x0100`: Skyrim -> host `SkyState` seqlock
- `0x0200`: host -> Skyrim `SkateState` seqlock
- `0x1000`: 16 MiB Skyrim -> host collision byte ring

Coordinates are **SkyCraft MC-space**:

- X horizontal
- Y up
- Z horizontal
- one block = 70 Skyrim units

The Skate host is responsible for the single MC-space -> Skate-space basis/scale conversion.

## Collision ring

The ring intentionally mirrors SkyCraft's existing collision-ring style:

- monotonic u64 head and tail,
- 8-byte-aligned messages,
- pad message on wrap,
- region payload containing exact triangles.

Message types:

- `COL_CLEAR`: collision epoch changed; discard installed world collision
- `COL_REGION_TRIS`: replace one collision region with exact triangles

`CollisionTri` is 40 bytes: 9 floats plus the original SkyCraft collision flags. Keeping the flags lets later code filter stair helpers/diggable/material types without another Havok walk.

## Current prototype behavior

The first host does **not** run Skate physics yet. It validates:

- mapping ABI,
- heartbeats,
- seqlocks,
- mode request,
- collision-region transport,
- finite triangle data.

It echoes the Skyrim start position into `SkateState`. This is intentional: authority handoff should not be connected until the transport can be observed safely.
