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
  economy/           messoria-economy           Wallets, market prices, daily sales limits, trades
  farming/           messoria-farming           Crop growth, withering, harvests and their quality
  inventory/         messoria-inventory         Slots, stacks, quality and spoilage
  save/              messoria-save              Versioned save files for worlds and players
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
| `rendezvous` (binary) | M8 | Access codes, hole punching, relay |

## Dependency rules

```
bins, tools ──► client ──┐
           └──► server ──┴──► shared ──► domain crates (calendar, content, economy, farming, inventory, save, voxel)
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
- A remote client inserts lightyear's `PredictionManager`. Without it,
  lightyear neither records prediction history nor rolls back to the server's
  state, and a predicted character drifts from the server's for good once one
  input goes unapplied.
- lightyear drops messages left unread at the end of a frame, so systems that
  receive messages run every frame (`PreUpdate`), never in `FixedUpdate`.

## Terrain

See [ADR 0004](adr/0004-terrain-representation.md) and
[ADR 0005](adr/0005-terrain-edits-and-shading.md).

- The server generates the farm valley at startup and owns the authoritative
  `Terrain` resource. A client's `Terrain` holds only the chunks streamed to it.
  A hosted world shares one `Terrain` between its server and client.
- Chunks within a client's view radius are streamed nearest first, a few per
  tick; chunks beyond a wider radius are unloaded.
- Clients dig or raise by using the shovel (`UseItem`). The server checks
  reach, rate, the target and nearby players, applies the brush and forwards
  the changed voxels to every client holding the chunk.
- A brush moves the ground within a disc to the next level, a multiple of its
  step, below or above the target. The client draws terrain flat-shaded, one
  material per facet.
- Any change to `Terrain` emits `ChunkChanged` for each chunk whose mesh
  depends on it; the client remeshes those under a per-frame time budget.

## Days

- The server owns a single clock entity carrying `WorldClock`,
  `CurrentWeather` and `SleepTally`, replicated to every client. It advances one game minute per
  real second, counted in whole ticks so it never drifts.
- A day ends when the server's `SleepRule` is met or at 02:00. Sleepers wake
  rested; anyone still awake at 02:00 passes out and recovers only half their
  energy. The rule is `Everyone` in a hosted world and a configurable share
  on a dedicated server.
- Clients advance their copy of the clock between updates so lighting moves
  smoothly; sky, sun, fog and ambient light blend between keyframes.

## Persistence

See [ADR 0007](adr/0007-save-format.md).

- A world is a save folder read by `messoria-save`. Executables open it
  before building the app (`WorldSetup::open`): a world saved there resumes,
  otherwise a new one starts with a random seed. A save that cannot be read
  stops the program.
- `ServerPlugin` receives a `WorldSetup`. Each part of the server sets up its
  share of the world at startup from the `Beginning` resource: the clock and
  weather seed, the terrain laid over the generated valley, the market, the
  fields and the players who have been in the world.
- The server saves a new world at once, then every five minutes, at each
  dawn and when the app exits. Players who leave are kept by key and written
  with the next save; returning players find their character as they left
  it, caught up with any days that passed.
- Clients identify themselves with the id in their local profile
  (`saves/profile.ron`); the host of a world is `host`.

## Simulation timing

Gameplay runs in `FixedUpdate` at `messoria_shared::tick::TICK_RATE_HZ`. Rendering
and input run in `Update` at the display rate. Client-side prediction replays
the same fixed-step systems the server runs, which is why those systems live in
`shared`. Characters simulated locally are blended between ticks for display;
remote characters are interpolated between server snapshots.

## Content

Game content (items, crops, shops and the market's tuning, scenery and the
palette) lives in RON files under `assets/data/`, never in code. Models live
under `assets/models/`, and the catalog checks that every model it refers to
is there.

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
  of the world as a Bevy message (`ShovelUse` for the terrain, `FieldWork` for
  fields); eating is handled by the inventory itself. Every use counts against
  one per-player rate limit.
- At dawn every inventory spoils what has expired.

## Fields

- A field is a replicated entity per tilled square of the one-meter grid, not
  a terrain change: `Field` holds its tile and ground height, with `Watered`,
  `Fertilized` and `Crop` added as it is tended. The server indexes fields by
  tile.
- Crops grow once per night, in one batch at dawn, from whether their field
  was watered during the day. The weather for the new day is chosen first; rain
  waters every field.
- Reshaping the ground under a field destroys it, which terrain edits report
  as `GroundReshaped`.

## Scenery and art

See [ADR 0008](adr/0008-scenery-and-art.md).

- The server scatters props from the world's seed after startup and
  replicates each as a `Prop` entity (kind, model, position, turn, scale). Its
  `Scenery` resource keeps their footprints, where the shovel and the hoe
  cannot work.
- Clients draw props from their glTF models, and grow ground cover for the
  chunks near the camera, merged into one mesh per palette material.
- Every mesh whose material name is in the palette is drawn with a shared
  material for that name. The client follows the world's season (`DrawnSeason`)
  and recolors those materials, and remeshes the terrain, when it turns.
- `environment` blends the sky's colors and the light through the day into
  the `Sky` resource, and `sky` draws the dome, the sun or moon and the
  clouds from it. Fog fades the land into the horizon's color.

## Feedback and feel

See [ADR 0009](adr/0009-action-feedback-and-collision.md).

- Server systems write `Tell` where they refuse an action for a reason the
  player can act on, and `Show` where one goes through; the `feedback`
  module sends them as `Notice` (to that player) and `Happening` (to
  everyone) on the `FeedbackChannel`.
- The client shows notices above the hotbar, and plays a spatial sound and
  throws particles where each happening happened. Footsteps sound by the
  ground under each character.
- `messoria_shared::obstacles` builds upright cylinders from replicated props
  and stalls on every peer; movement pushes bodies out of them, so predicted
  and authoritative movement collide alike. Sprinting is part of
  `PlayerInput`.
- The client names what the crosshair is on from the terrain it aims at and
  a ray cast against the same obstacles.
- The game menu, its options and the backpack window are client panels
  (`OpenPanel`), one at a time. Options are kept in `saves/settings.ron`.

## Village and economy

See [ADR 0006](adr/0006-shared-market-priced-on-both-sides.md).

- The village is a fixed area of the valley (`messoria_shared::village`),
  generated level and paved. Its ground cannot be dug, raised or tilled.
- Each shop's stall is a replicated `Shopfront` entity placed by the server's
  village layout. A client trades by sending `Trade` while its character
  stands at the stall; the server checks the stall, the shop's hours and the
  trade rules.
- Characters carry `Money` and `SoldToday` (their sales against the shops'
  daily limits). One `MarketState` entity holds the server-wide saturation.
  At dawn the market recovers and everyone's daily sales start over.
- Trades run on copies of a character's state, which replace the originals
  only if the trade happens. Clients price their shop window with the same
  functions on copies of their own state.
- `GiveMoney` moves money to another player, all of it or none.
