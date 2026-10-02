# Third-party source boundary

Research snapshot: 2026-10-02.

## MW2/IW4L mashup

The architectural reference is:

- `chasmlol/2010-rust-rewrite-mashup`
- snapshot `f608f85e407ff1b7689d54a9aafdd16e95711ac4`

That repository has an Apache-2.0 root `LICENSE`. Its `NOTICE` explicitly
describes `skate/` as an in-tree project of the mashup author and states that
no Skate game code or data is included.

If source is copied from that snapshot into this project, preserve the Apache
license requirements and applicable NOTICE attribution, and mark modified files.

## Current SK8-ENGINE main

The current `SK8-ENGINE/skate-3-rust-engine` repository was reviewed as a
technical reference. At this snapshot a root `LICENSE` file was not found
through the GitHub contents API. Do not vendor/copy source from current main
until its redistribution terms are explicit.

This restriction does not prevent reading it as a technical reference or using
the separately licensed mashup snapshot above.

## Game data

Never commit or redistribute Skate 3 retail assets, `default.xex`, extracted
`data/`, converted skater assets, animation banks, state graphs, physics
settings, or other retail-derived output. Those remain local user-provided data.
