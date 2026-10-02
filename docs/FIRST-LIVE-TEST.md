# First live integration test

This test requires **no Skate 3 retail data** and deliberately does not take movement authority.

## What it proves

- the optional SKSE bridge loads without breaking normal SkyCraft,
- `SkyrimSkateHost.exe` can open `Local\\SkyCraftSkate_v1`,
- Skyrim world/player state reaches the host,
- exact Havok collision regions stream to the host,
- the host retains the live collision world,
- automatic grind detection finds candidate rails from Skyrim geometry.

## Expected behavior

Normal SkyCraft remains in control. You should still move as the Minecraft player exactly as before.

Pressing **F6** only changes the prototype's requested-mode flag. It must **not** move, freeze, teleport, or replace the player yet.

The host console should eventually print messages like:

```text
SkyrimSkateHost: connected
SkyrimSkateHost: collision clear epoch=...
SkyrimSkateHost: rail scan tris=... edges=... lips=... runs=... rails=...
SkyrimSkateHost: world=0x... epoch=... mode=0 regions=... triangles=... rails=...
```

After F6, `mode=1` should appear. Pressing F6 again should return to `mode=0`.

The host also writes the same telemetry to **`SkyrimSkateHost.log` beside `SkyrimSkateHost.exe`**. The file is replaced each time the host starts, so after a test you can upload that one file for analysis.

## Safe MO2 layout

Install the prototype DLL as a **separate mod below the normal SkyCraft mod**:

```text
SkyCraft                         [enabled]
SkyCraft Skate Bridge Test      [enabled, lower priority]
```

The test mod should contain only:

```text
SKSE/
  Plugins/
    SkyCraft.dll
```

That lets MO2 override only the DLL. To revert, uncheck `SkyCraft Skate Bridge Test`; the original SkyCraft mod remains untouched.

## Test locations

Start with an ordinary exterior after the world has settled, for example Whiterun/Riverwood. Walk through:

- flat ground,
- stairs,
- stone walls/ledges,
- bridges,
- slopes.

For the first pass, the important result is that triangle/rail counts are nonzero and update without crashes.

## Failure rule

If Skyrim crashes, normal SkyCraft movement stops working, or the host reports malformed/non-finite collision, stop the test and save both:

- `Documents/My Games/Skyrim Special Edition/SKSE/SkyCraft.log`
- the host console text.

Do not overwrite the original SkyCraft installation while testing.
