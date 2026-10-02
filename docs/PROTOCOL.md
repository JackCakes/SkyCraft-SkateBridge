# Live bridge protocol v2

Mapping: `Local\\SkyCraftSkate_v1`

The prototype keeps Skate transport separate from SkyCraft's existing Minecraft mapping so early experiments cannot destabilize the working Minecraft protocol.

## Fixed sections

- `0x0000`: 64-byte header
- `0x0100`: Skyrim -> host `SkyState` seqlock
- `0x0200`: host -> Skyrim `SkateState` seqlock
- `0x0300`: Skyrim -> host `InputState` seqlock
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

## Input state

`InputState` is deliberately XInput-shaped because the reconstructed Skate host consumes the same raw layout:

- 16-bit button mask,
- two 8-bit analog triggers,
- left stick X/Y as signed 16-bit axes,
- right stick X/Y as signed 16-bit axes,
- monotonically increasing packet number,
- frame delta used by the diagnostic harness.

The current Skyrim side supplies a keyboard fallback: WASD -> left stick, arrows -> right stick, Q/E -> triggers, and Space/Ctrl/Shift/R -> A/B/X/Y. A real controller can later populate the same ABI without changing the Skate-facing transport.

## Current prototype behavior

Retail Skate physics is still intentionally absent. The host now validates the complete authority boundary with a **bounded synthetic controller**:

- F6 requests external authority,
- live input crosses the shared-memory ABI,
- the host publishes a finite root pose and diagnostic chase camera,
- Skyrim accepts movement only within a strict safety radius,
- menus/loading/death/world changes/host failure preempt authority,
- the final finite position is handed back through SkyCraft's normal Minecraft teleport handshake.

The synthetic controller is diagnostic plumbing only. It does not implement or approximate Skate tricks, grinds, ollies, carving, manuals, or bails.
