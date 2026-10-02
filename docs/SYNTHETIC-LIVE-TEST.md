# Synthetic authority live test

This is the second live integration test. It still requires **no Skate 3 retail data**.

## Purpose

Prove the complete movement/input/camera authority boundary before real Skate physics is attached.

## Expected behavior

1. Start the updated `SkyrimSkateHost.exe`.
2. Launch Skyrim through the experimental SkyCraft Skate Bridge MO2 override.
3. Enter ordinary SkyCraft/Minecraft-character mode in a safe exterior.
4. Press **F6** once.
5. The host should report `synthetic authority acquired`.
6. While F6 mode is active:
   - **WASD** is the diagnostic left stick and moves the synthetic controller,
   - **arrow keys** are the diagnostic right stick and orbit the chase camera,
   - **Q/E** are the two analog triggers,
   - **Space / Ctrl / Shift / R** map to Xbox A / B / X / Y.
7. Movement is intentionally capped to **1.25 blocks from the activation point**. This is a safety harness, not Skate physics.
8. Press **F6** again. Minecraft should resume from the host's final position instead of snapping back.

The host log should show input telemetry such as `input_packet`, `buttons`, `left`, `right`, and `triggers`.

## Safety expectations

Authority must cancel rather than fight Skyrim when any of these happen:

- host disconnect/error,
- non-finite or implausibly distant host pose,
- Skyrim menu/loading screen,
- player death,
- scripted/furniture/mount takeover,
- world or collision epoch change.

After a safety preemption the host will not immediately reacquire. F6 must fully return to Minecraft mode before another external-authority attempt.

## What this does not test

This controller does **not** implement Skate 3 physics, tricks, ollies, grinds, manuals, bails, animations, or the skater rig. It exists only to prove the transport and authority seam that the real Skate `Session` will later occupy.
