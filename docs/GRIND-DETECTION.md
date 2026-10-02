# Automatic grind detection

## Why this matters

Skyrim was never authored with Skate grind splines, but the MW2/Minecraft mashup already proves that useful grind rails can be inferred from arbitrary collision geometry.

## Algorithm to port

For each triangle:

1. compute its normal,
2. if the face is sufficiently upward/walkable, examine its three edges,
3. determine the outside direction from edge direction and face normal,
4. sample along the edge,
5. probe just past the edge:
   - continued ground -> reject,
   - wall immediately outside/above -> reject,
   - open drop -> candidate lip,
6. merge collinear touching/overlapping lip segments,
7. chain runs through gentle corners,
8. discard tiny or overly steep rails.

The probing approach is better than requiring two triangles to share byte-identical edge vertices, which is important for Skyrim collision seams.

## Skyrim-specific filters

The first implementation should support filters for:

- actors/character controllers: always ignore,
- foliage/tree collision: ignore by default,
- terrain micro-edges: optionally suppress,
- architecture/statics/furniture: allow,
- moving doors: include only when current collision is stable,
- SkyCraft Minecraft blocks: allow later as a second source.

## Initial metre-scale tunables

Treat these only as starting ranges:

- walkable normal threshold: 0.60-0.70,
- outward probe: 0.04-0.08 m,
- required drop: 0.08-0.15 m,
- wall probe height: 0.08-0.15 m,
- minimum rail length: 0.5-0.8 m,
- early maximum rail slope: ~35 degrees,
- chain turn: ~30-40 degrees.

## Debug visualization

Development mode should be able to show:

- source collision triangles,
- candidate edges,
- accepted lips,
- merged rails,
- current contacted rail.

Tuning this without visualization would be unnecessarily slow.
