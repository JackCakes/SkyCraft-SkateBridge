# Synthetic authority harness (no retail data)

This is the next development layer after the first live collision test.

It is **not** intended to approximate Skate 3. Its only purpose is to prove that SkyCraft can safely hand the Skyrim player to a second external movement authority and take them back without snapping, corrupting saves, or breaking Minecraft.

## Gating

The harness must remain disabled by default and require an explicit developer switch. Normal F6 transport testing remains non-moving until that switch is enabled.

## What the synthetic controller will do

While active it will use the live collision cache to provide a deliberately simple diagnostic pose:

- keep the player on/near the current ground,
- accept basic forward/turn input,
- publish a finite root transform,
- publish a simple chase camera,
- stop immediately if collision, heartbeat, or state becomes invalid.

It will not implement ollies, tricks, grinds, carving, manuals, bails, or Skate physics.

## Handoff invariant

Minecraft -> synthetic:

1. record the current externally driven feet position,
2. initialize the diagnostic controller at exactly that pose,
3. freeze Minecraft locomotion/input ownership,
4. wait for one valid synthetic output frame,
5. then move Skyrim from that output.

Synthetic -> Minecraft:

1. stop accepting synthetic input,
2. keep the last finite synthetic feet position,
3. request a Minecraft teleport/resync to that position,
4. wait for Minecraft's teleport acknowledgement,
5. restore normal SkyCraft ownership.

Menus, scripted Skyrim takeovers, death, loading screens, host failure, and non-finite output always cancel synthetic authority.

## Why do this before retail Skate data?

The real Skate session should be plugged into an already-proven authority boundary. Then a problem in real skating can be debugged as a Skate/collision/input issue rather than being mixed with an untested Skyrim control handoff.
