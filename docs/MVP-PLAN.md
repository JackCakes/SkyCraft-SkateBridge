# MVP implementation plan

## Phase 0 — pin/build

- Pin exact SkyCraft and Skate runtime revisions.
- Reproduce builds.
- Keep all retail-generated data outside git.

## Phase 1 — offline Skyrim collision proof

- Add a developer dump of exact SkyCraft triangles around the player.
- Feed the dump into a standalone Skate collision test.
- Validate Whiterun terrain/buildings, slopes and stairs.

Acceptance: a Skate session can roll on recorded Skyrim collision without Skyrim running.

## Phase 2 — live shared-memory bridge

Implement `Local\\SkyCraftSkate_v1`.

Skyrim -> host:

- heartbeat,
- world/cell + epoch,
- current start pose,
- viewport/aspect,
- exact collision chunks,
- input state.

Host -> Skyrim:

- heartbeat/ready/error,
- root pose,
- velocity/state flags,
- camera.

Acceptance: collision streams live with no player takeover.

## Phase 3 — movement authority

- Add configurable toggle.
- Suspend Minecraft movement while Skate owns movement.
- Puppet Skyrim from Skate pose.
- Preserve final position when handing control back.

Acceptance: repeatedly toggle Minecraft <-> Skate without snapping or save damage.

## Phase 4 — grinds

- Port automatic rail detector.
- Tune in Whiterun/Riften/Dwemer geometry.
- Add debug visualization.

Acceptance: naturally useful edges grind without hand-authored markers.

## Phase 5 — presentation

- Board rendering.
- Skater body / rig retarget.
- Bail presentation.
- Skate camera.
- Optional trick/state HUD.

## Phase 6 — SkyCraft expansion

- Merge Minecraft placed-block collision into Skate world.
- Skate player-built ramps/ledges in Skyrim.
- Add multiplayer-aware state only after local authority is stable.
