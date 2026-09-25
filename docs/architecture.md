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
  worldgen/          messoria-worldgen          The valley grown from the seed: its shape, river, roads and scenery
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
           └──► server ──┴──► shared ──► domain crates (calendar, content, economy, farming, inventory, save, voxel, worldgen)
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
- Interest management (`messoria_server::interest`): every column of the
  world is a lightyear room. Fields, structures, stalls and characters are
  put in the room of the column they stand in, characters changing rooms as
  they walk, and each client joins the rooms around its character, as far as
  it holds terrain. What is in no room (the clock, the market) reaches
  everyone. See [ADR 0015](adr/0015-a-larger-valley-grown-on-both-sides.md).

## Terrain

See [ADR 0004](adr/0004-terrain-representation.md),
[ADR 0005](adr/0005-terrain-edits-and-shading.md),
[ADR 0013](adr/0013-picture-and-painted-ground.md) and
[ADR 0015](adr/0015-a-larger-valley-grown-on-both-sides.md).

- `messoria-worldgen` grows the valley from the seed: `Landscape` gives the
  ground's height and material anywhere, the river's course and the roads,
  and generates any column of chunks; `props_in_column` grows the scenery of
  a column. The world is 64 by 64 columns of three chunks, two kilometers
  across. `messoria_shared::valley::Valley` holds the `Landscape`: the server
  from the start, a client once `TerrainUpdate::World` tells it the seed.
- The server owns the authoritative `Terrain` resource, holding the columns
  within a few columns of any player: `terrain::loading` generates missing
  ones nearest first, several at a time on every core, and unloads those
  nobody is near. Edited chunks are parked while their columns are unloaded.
  A client's `Terrain` holds only the columns streamed to it; a hosted world
  shares one between its server and client.
- Loading and unloading a column writes `ColumnLoaded` or `ColumnUnloaded`
  (`LoadColumns` systems run before the scenery grows on them).
- Columns within a client's view radius are streamed whole, nearest first, a
  few per tick, with what was gathered on them; columns beyond a wider
  radius are unloaded.
- Clients dig or raise by using the shovel (`UseItem`). The server checks
  reach, rate, the target and nearby players, applies the brush and forwards
  the changed voxels to every client holding the chunk.
- A brush moves the ground within a disc to the next level, a multiple of its
  step, below or above the target.
- The mesher gives each vertex its ground material and a normal from the
  distance field. The client's `ground` material paints the terrain with
  pixel-art textures in world space, picking the material texel by texel,
  and tints them by the season.
- Any change to `Terrain` emits `ChunkChanged` for each chunk whose mesh
  depends on it; the client remeshes those under a per-frame time budget.
- The client draws in full the loaded columns near its camera
  (`FullColumns`), and the rest of the valley coarsely from its shape
  (`horizon`): tiles of the heightfield on a coarse grid, in the same ground,
  leaving out the columns drawn in full with skirts beside them. `water`
  lays the river's water along its whole course.

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
  weather seed, the chunks players edited (parked until their columns
  load), what was gathered, the market, the fields and the players who have
  been in the world. Worlds saved before the valley grew (world format 3 and
  earlier) are refused.
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

Game content (items, crops, shops and the market's tuning, scenery, the
palette, structures, the village's layout and the characters) lives in RON
files under `assets/data/`, never in code. Models live under
`assets/models/`, and the catalog checks that every model it refers to is
there.

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
  fields, `GatherUse` for scenery, `BuildUse` for structures); eating is
  handled by the inventory itself. Every use counts against
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

## The picture

See [ADR 0013](adr/0013-picture-and-painted-ground.md).

- `picture` finishes what the world camera renders: high dynamic range,
  tonemapping, bloom, temporal anti-aliasing, and a color grade, exposure
  and haze that follow the time of day and the weather. The graphics
  quality in the player's settings adds ambient occlusion, sunbeams through
  the haze and sharper shadows.
- `environment` blends the sky's colors and the light through the day into
  the `Sky` resource, and `sky` draws the dome, the glowing sun or moon, the
  stars and the clouds from it. Fog fades the land into the horizon's color.

## Scenery and art

See [ADR 0008](adr/0008-scenery-and-art.md) and
[ADR 0014](adr/0014-block-models-painted-for-the-game.md).

- Server and clients grow the props of every column they load from the seed
  (`messoria_shared::scenery`): the `Scenery` resource keeps each prop, named
  by its `PropKey` (kind and scattering cell), its footprint, where the
  shovel and the hoe cannot work, and what was gathered. It finds the prop a
  player works on. No prop is replicated. The client also grows the tall
  props of the columns past its terrain, for a few hundred meters, and draws
  them standing.
- Models are block models with an embedded pixel-art atlas, made in meters.
  Clients draw props from them, and grow ground cover for the chunks near the
  camera, merged into one mesh per material, each vertex keeping its height
  over its plant.
- `art` dresses every mesh whose material name is in the palette: its
  texture tinted by the palette's color, and, for names the palette sways,
  drawn with `wind`'s material, whose vertex shaders push foliage along the
  wind in every pass. Dressed materials are shared by name and source
  material. The client follows the world's season (`DrawnSeason`) and
  retints them, and remeshes the terrain, when it turns.

## Gathering

See [ADR 0010](adr/0010-gathered-scenery.md).

- Using the axe or the pickaxe on a prop, or `Gather` (by hand), becomes a
  `GatherUse` for the server's `gathering` module, which counts strikes,
  gives the yield and records in `Scenery` the day the prop was gathered. At
  dawn, kinds that grow back are forgotten as gathered once enough days have
  passed. Each change writes `SceneryChanged`, which moves obstacles, redraws
  the prop and is sent to the clients that have its column.
- What a gathered prop leaves standing decides whether it is still an
  obstacle and keeps its ground; clients draw it as its remains, and draw
  fruit on props that can be picked.
- The world file lists gathered props by kind and scattering cell.

## Building and homes

See [ADR 0011](adr/0011-structures-and-homes.md).

- Using a structure's item becomes a `BuildUse` for the server's `building`
  module, which checks the ground (`Site`), levels it for structures that
  level theirs (`messoria_voxel::Levelling`), and spawns a replicated
  `Structure` for it and for each structure it contains. Chests also get a
  replicated `Stored` inventory. `Built` finds structures and the ground
  they keep, which the shovel and the hoe leave alone.
- `messoria_shared::structures` places a structure's parts, boxes and
  contents in the world the same way on every peer; the client draws its
  parts and lamps, and the obstacles gain its boxes.
- `homes` gives newcomers without a home the item to build one and moves
  whoever collapses at the end of a day beside their bed. The day cycle
  accepts `SleepRequest::Sleep` only near a bed.
- `storage` carries out `MoveStored` between a character and the chest it
  stands at. The client's chest window shows the chest above the backpack
  and hotbar.
- The world file keeps every structure, whose home it is and what it keeps.

## Feedback and feel

See [ADR 0009](adr/0009-action-feedback-and-collision.md).

- Server systems write `Tell` where they refuse an action for a reason the
  player can act on, and `Show` where one goes through; the `feedback`
  module sends them as `Notice` (to that player) and `Happening` (to
  everyone) on the `FeedbackChannel`.
- The client shows notices above the hotbar, and plays a spatial sound and
  throws particles where each happening happened. Footsteps sound by the
  ground under each character.
- `messoria_shared::obstacles` builds upright cylinders from the props grown
  on the loaded columns and from replicated stalls, and boxes from
  structures, on every peer; movement pushes bodies out of them, so predicted
  and authoritative movement collide alike. Each obstacle belongs to a
  `Blocker`: an entity, or a prop by its key. Sprinting is part of
  `PlayerInput`.
- The client names what the crosshair is on from the terrain it aims at and
  a ray cast against the same obstacles.
- The game menu, its options and the backpack window are client panels
  (`OpenPanel`), one at a time. Options are kept in `saves/settings.ron`.

## Characters and icons

See [ADR 0012](adr/0012-characters-village-and-icons.md).

- Every character is a figure of boxes (`figures`), built from the limbs in
  `characters.ron` and painted from a skin. `skins` lays each look's layers
  (a body, an outfit, hair tinted with a hair color) into one image once they
  load, and figures dressed alike share its material. Players are dressed
  from the wardrobe by their id (`Characters::look_for`), so everyone sees
  the same look for the same player; shopkeepers' looks are in `shops.ron`.
  Faces blink, strain at hard work, smile at a trade or a meal, start when
  falling and close in sleep, by swapping the head's front for another
  painted expression.
- `avatars` poses figures every frame rather than playing recorded clips:
  legs and arms swing with the distance walked, knees and elbows bending,
  figures lean into a run, tuck their legs in a jump, crouch on landing,
  and sleepers lie in the nearest bed.
  Happenings name who caused them (`Happening::by`), and that character acts
  them out: chopping, digging, bending down to plant, watering, eating.
- What a character holds is fitted to its hand by the size of its model.
  The local player's hand follows its hotbar at once; everyone else's
  follows the replicated `Holding`, which the server derives from the held
  slot in `PlayerInput`.
- In first person a second camera draws the player's own arm, holding the
  item, over the world on its own render layer, so it never sinks into
  walls; it sways with the steps and swings on each use.
- Shopkeepers stand behind their stalls' counters, with some of their wares
  on them, and greet whoever trades there.
- Item icons are rendered by the client at startup from each item's model
  (a lump of its color if it has none; seeds with what they grow into),
  each by its own camera into an image, far below the valley on a render
  layer nothing else uses. The hotbar, backpack, chest and shop show them.

## Village and economy

See [ADR 0006](adr/0006-shared-market-priced-on-both-sides.md).

- The village is a fixed area of the valley (`messoria_shared::village`),
  generated level and paved. Its ground cannot be dug, raised or tilled.
- `village.ron` places each shop's stall and the village's buildings around
  the square. At startup the server spawns a replicated `Shopfront` for each
  stall and a `Structure` marked `Landmark` for each building; landmarks are
  rebuilt from the data every time, so saves leave them out. A client trades by sending `Trade` while its character
  stands at the stall; the server checks the stall, the shop's hours and the
  trade rules.
- Characters carry `Money` and `SoldToday` (their sales against the shops'
  daily limits). One `MarketState` entity holds the server-wide saturation.
  At dawn the market recovers and everyone's daily sales start over.
- Trades run on copies of a character's state, which replace the originals
  only if the trade happens. Clients price their shop window with the same
  functions on copies of their own state.
- `GiveMoney` moves money to another player, all of it or none. Clients
  only know the players near them, so money is given to someone nearby.
