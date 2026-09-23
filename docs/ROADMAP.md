# Roadmap

The road to the MVP described in the [game design document](design/game-design.md#10-mvp-scope-vertical-slice).
Each milestone ends in a playable or testable state and has an explicit exit
criterion. Milestones are sequential; later ones may be reordered as the design
evolves.

Open design questions are listed under the milestone that first needs an answer.

## M0 — Foundation

Goal: a workspace that builds, lints and tests cleanly on every platform we ship to.

- [x] Cargo workspace with shared lints, profiles and pinned toolchain
- [x] `messoria-shared`, `messoria-server` and `messoria-client` crates
- [x] Dedicated server binary running the fixed-tick simulation headless
- [x] Game binary opening a window with a lit scene
- [x] CI: formatting, Clippy, tests on Linux and Windows, dependency policy
- [x] Design document, architecture overview and decision records

**Exit criterion:** `cargo lint` and `cargo test --workspace` pass; both binaries run.

## M1 — Networked skeleton

Goal: several players share one world over the network and see each other move.

- [ ] Protocol module in `messoria-shared` (components, inputs, channels) on lightyear
- [ ] Dedicated server accepting clients by IP and port
- [ ] Player spawn and despawn on connect and disconnect
- [ ] Shared movement system with client-side prediction and reconciliation
- [ ] Interpolation of remote players
- [ ] First- and third-person camera, toggled with a key
- [ ] Hosted mode: the server running inside the game client
- [ ] Headless bot client that connects and walks at random (seed of the load test)

**Exit criterion:** four clients (two real, two bots) move smoothly on a
dedicated server with 150 ms of simulated latency.

## M2 — Voxel terrain

Goal: a smooth, deformable farm that every player sees identically.

- [ ] `messoria-voxel`: chunk storage (32³), materials, coordinate types, edits
- [ ] Surface Nets meshing with material blending, covered by tests and benchmarks
- [ ] Server-side terrain generation for the fixed farm area
- [ ] Chunk streaming by player proximity; voxel deltas after the initial snapshot
- [ ] Character collision against terrain on server and client alike
- [ ] Shovel: dig and raise terrain; highlight of the targeted voxel

**Exit criterion:** two players reshape the same hill and see identical results;
remeshing a chunk stays under one frame budget.

## M3 — Time and seasons

Goal: the world keeps a calendar and the day has a rhythm.

- [ ] World clock: 20-minute days, 365-day year, four seasons
- [ ] Day/night lighting driven by the clock
- [ ] Sleep rules per mode: solo, hosted (everyone asleep), dedicated (configurable percentage)
- [ ] Passing out at 2 AM with a penalty
- [ ] Energy: spent by actions, restored by sleep

**Exit criterion:** a day advances correctly under each sleep rule, verified by tests.

## M4 — Content and inventory

Goal: items exist, can be carried and go bad.

- [ ] `messoria-content`: item, crop and shop definitions in RON, validated on load
- [ ] Inventory with hotbar and backpack, server-authoritative
- [ ] Shelf life for perishables; spoiled items turn into compost
- [ ] Tools as inventory items bound to actions

**Exit criterion:** an invalid content file fails at startup with a precise error;
inventory rules are covered by unit tests.

## M5 — Farming

Goal: the core farming loop works end to end.

- [ ] Hoe tills terrain surface into farmland
- [ ] Five crops defined in data
- [ ] Watering can and rain
- [ ] Batched growth resolved at day rollover
- [ ] Harvest with quality tiers; crops die out of season

**Exit criterion:** a crop planted on day 1 is harvested on the day its data file
predicts, in a simulated run and in a live session.

## M6 — Village shop and economy

Goal: produce turns into money, and money reacts to supply.

- [ ] `messoria-economy`: seasonal base price, saturation and daily recovery, daily purchase limits
- [ ] Individual wallets and transfers between players
- [ ] One village shop with opening hours: buys crops, sells seeds
- [ ] Protected village area

**Open:** is the daily limit per player or shared across the shop?

**Exit criterion:** price curves under simulated load match the design in tests.

## M7 — Persistence

Goal: nothing is lost when the server restarts.

- [ ] Versioned save format: terrain deltas, entities, economy, one file per player
- [ ] Periodic autosave and save on shutdown
- [ ] Load path with migration hooks for future format versions

**Exit criterion:** stop and restart the server mid-day and resume without any difference.

## M8 — Open to friends

Goal: inviting a friend takes under a minute and needs no router setup.

- [ ] Rendezvous service issuing short access codes
- [ ] UDP hole punching between host and guest
- [ ] Relay fallback through the rendezvous host
- [ ] "Open to friends" flow in the client

**Exit criterion:** two machines behind separate home routers connect by code.

## M9 — Scale

Goal: a dedicated server holds 50 players comfortably.

- [ ] `tools/loadtest`: scripted bots exercising movement, farming and trading
- [ ] Interest management for entities and chunks
- [ ] Server tick profiling and budgets
- [ ] Low-frequency simulation for areas far from players

**Exit criterion:** 50 bots for 30 minutes with the tick budget met at the 99th percentile.

## M10 — MVP playtest

Goal: the slice feels like a game.

- [ ] Art pass with CC0 packs; crop stage meshes generated in code
- [ ] Seasonal music and ambient audio
- [ ] HUD: hotbar, clock, wallet, energy
- [ ] Playtest: a group plays a full in-game week

**Open:** fantasy sub-theme; farm plot count, size and sharing.

**Exit criterion:** the MVP success criteria in the design document are met.

## After the MVP

Loans, combat and mines, procedural forest, villager schedules and friendship,
crafting and buildings, fishing, animals, festivals.
