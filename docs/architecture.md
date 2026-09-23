# Architecture

## Overview

Messoria runs a single authoritative server simulation. Clients send inputs and
render the state the server replicates back to them. The same server code runs
in two places:

- **Dedicated server:** a headless process for community hosting.
- **Hosted world:** inside the game client. Solo play is a hosted world that
  only listens on loopback; opening it to friends means listening on a public
  port instead.

See [ADR 0001](adr/0001-server-authoritative-single-codebase.md).

## Workspace

```
crates/
  calendar/          messoria-calendar          Game time, dates, seasons, sleep rules
  content/           messoria-content           Content catalog loaded from data files, with validation
  inventory/         messoria-inventory         Slots, stacks and spoilage
  voxel/             messoria-voxel             Terrain storage, edits, queries, Surface Nets meshing
  shared/            messoria-shared            Networking setup, protocol, deterministic simulation
  server/            messoria-server            Authoritative game logic
  client/            messoria-client            Rendering, input, camera, UI, audio
bins/
  game/              messoria                   Game client executable
  dedicated-server/  messoria-dedicated-server  Headless server executable
tools/
  loadtest/          messoria-loadtest          Headless bot players
assets/
  data/              Game content in RON
docs/
  design/            Game design document
  adr/               Architecture decision records
```

Crates planned by the [roadmap](ROADMAP.md) are added when their milestone
starts, never ahead of time:

| Crate | Milestone | Responsibility |
|---|---|---|
| `messoria-economy` | M6 | Prices, saturation, purchase limits, wallets |
| `rendezvous` (binary) | M8 | Access codes, hole punching, relay |

## Dependency rules

```
bins, tools ──► client ──┐
           └──► server ──┴──► shared ──► domain crates (calendar, content, inventory, voxel, economy)
```

- **Domain crates do not depend on Bevy.** They hold pure data structures and
  rules that are unit-tested and benchmarked in isolation. See
  [ADR 0003](adr/0003-pure-domain-crates.md).
- **`shared` owns the network configuration.** It adds lightyear's plugin
  groups, registers the protocol and defines transports and authentication.
  Client and server use lightyear's replication types, never its setup. See
  [ADR 0002](adr/0002-networking-lightyear.md).
- **`client` and `server` never depend on each other.** A hosted world composes
  both plugins in one `App`; neither assumes the other is present.
- **Executables only compose plugins.** They parse arguments, choose runtime
  plugins (window, schedule runner, logging) and add `SharedPlugin` with a
  role, followed by `ServerPlugin`, `ClientPlugin` or both.

## Plugins

Every game system is a Bevy plugin owned by the crate responsible for it.
Library plugins never add runtime plugins such as `DefaultPlugins`,
`MinimalPlugins` or `LogPlugin`, so they stay composable in tests and in the
hosted mode.

`SharedPlugin { role }` must be added exactly once and before the others:
lightyear requires its client and server plugin groups to exist before the
protocol is registered, and the role (`Server`, `Client` or `Host`) decides
which groups are added.

## Networking

- Transport is UDP with netcode.io authentication, configured in
  `messoria_shared::network`. Clients currently sign their own connect tokens;
  the rendezvous service takes over token issuing in M8.
- The server spawns a character when a connection is confirmed and replicates
  it to everyone. Its owner predicts it; everyone else interpolates it.
  `ControlledBy` ties the character's lifetime to the connection.
- Clients send one `PlayerInput` per tick. The server treats input as
  untrusted and sanitizes it in the shared movement code.
- lightyear drops messages left unread at the end of a frame, so systems that
  receive messages run every frame (`PreUpdate`), never in `FixedUpdate`.

## Terrain

See [ADR 0004](adr/0004-terrain-representation.md).

- The server generates the farm valley at startup and owns the authoritative
  `Terrain` resource. A client's `Terrain` holds only the chunks streamed to it.
  A hosted world shares one `Terrain` between its server and client.
- Chunks within a client's view radius are streamed nearest first, a few per
  tick; chunks beyond a wider radius are unloaded.
- Clients ask to dig or raise with a `ShovelRequest`. The server checks reach,
  rate, the target and nearby players, applies the brush and forwards the
  changed voxels to every client holding the chunk.
- Any change to `Terrain` emits `ChunkChanged` for each chunk whose mesh
  depends on it; the client remeshes those under a per-frame time budget.

## Days

- The server owns a single clock entity carrying `WorldClock` and
  `SleepTally`, replicated to every client. It advances one game minute per
  real second, counted in whole ticks so it never drifts.
- A day ends when the server's `SleepRule` is met or at 02:00. Sleepers wake
  rested; anyone still awake at 02:00 passes out and recovers only half their
  energy. The rule is `Everyone` in a hosted world and a configurable share
  on a dedicated server.
- Clients advance their copy of the clock between updates so lighting moves
  smoothly; sky, sun, fog and ambient light blend between keyframes.

## Simulation timing

Gameplay runs in `FixedUpdate` at `messoria_shared::tick::TICK_RATE_HZ`. Rendering
and input run in `Update` at the display rate. Client-side prediction replays
the same fixed-step systems the server runs, which is why those systems live in
`shared`. Characters simulated locally are blended between ticks for display;
remote characters are interpolated between server snapshots.

## Content

Game content (items now; crops, shops and prices as they arrive) lives in RON
files under `assets/data/`, never in code.

- Executables load the catalog before building the app. Any problem, from a
  syntax error to a reference to an item that does not exist, stops the
  program with the file, the position where there is one, and the reason.
- Assets are found the way Bevy finds them: `BEVY_ASSET_ROOT`, which
  `.cargo/config.toml` points at the workspace root for development, or the
  executable's directory in a shipped build.
- Items are identified on the wire by their position in the catalog, so a
  client and a server must load the same content. Data files and saves refer
  to items by their text id.

## Items

- Each character's inventory (`Belongings`) lives on the server and is
  replicated. Clients ask to `MoveItem` between slots and to `UseItem` from a
  hotbar slot; the item in the slot decides what the use does.
- Tool uses that act on the world are handed to the system that owns that part
  of the world as a Bevy message (`ShovelUse` for the terrain); eating is
  handled by the inventory itself.
- At dawn every inventory spoils what has expired.
