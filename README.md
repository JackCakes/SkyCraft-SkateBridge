# SkyCraft Skate Bridge

Prototype and research repository for adding a toggleable Skate 3-style skating mode to Skyrim/SkyCraft.

The design is based on two working precedents:

- **SkyCraft** already exports Skyrim's Havok collision and lets an external simulation own the Skyrim player's movement/camera.
- **2010 Rust Rewrite Mashup** already runs reconstructed Skate physics on arbitrary MW2 and Minecraft collision, including automatic grind-rail discovery.

## Goal

Press a configurable toggle while playing Skyrim with SkyCraft and hand local-player locomotion to a Skate runtime. The skater should ride directly on Skyrim collision, use Skate-style pushing/carving/Flickit/tricks/manuals/grinds/bails, and cleanly hand control back to SkyCraft.

This repository contains **no retail Skate 3 data**. The user supplies an owned Xbox 360 dump locally. Generated assets stay ignored by git.

## Current status

Research is complete enough to begin the bridge prototype. Work is split into:

1. host-game collision -> Skate collision,
2. automatic rail detection,
3. live shared-memory transport,
4. movement-authority handoff,
5. pose/camera return,
6. presentation and packaging.

See `docs/` for the implementation research and plan.

## Upstreams

- SkyCraft: https://github.com/chasmlol/SkyCraft
- 2010 Rust Rewrite Mashup: https://github.com/chasmlol/2010-rust-rewrite-mashup
- Skate 3 Rust Engine: https://github.com/SK8-ENGINE/skate-3-rust-engine
