# Architecture

## Overview

Messoria runs a single authoritative server simulation. Clients send inputs and
render the state the server replicates back to them. The same server code runs
in two places:

- **Dedicated server:** a headless process for community hosting.
- **Hosted world:** inside the game client, when a player opens their world to friends.

See [ADR 0001](adr/0001-server-authoritative-single-codebase.md).

## Workspace

```
crates/
  shared/            messoria-shared            Protocol and deterministic systems both sides run
  server/            messoria-server            Authoritative game logic
  client/            messoria-client            Rendering, input, camera, UI, audio
bins/
  game/              messoria                   Game client executable
  dedicated-server/  messoria-dedicated-server  Headless server executable
docs/
  design/            Game design document
  adr/               Architecture decision records
```

Crates planned by the [roadmap](ROADMAP.md) are added when their milestone
starts, never ahead of time:

| Crate | Milestone | Responsibility |
|---|---|---|
| `messoria-voxel` | M2 | Chunk storage, voxel edits, Surface Nets meshing |
| `messoria-content` | M4 | Data definitions loaded from RON, with validation |
| `messoria-economy` | M6 | Prices, saturation, purchase limits, wallets |
| `rendezvous` (binary) | M8 | Access codes, hole punching, relay |
| `loadtest` (tool) | M9 | Bot clients for load testing |

## Dependency rules

```
bins ──► client ──┐
    └──► server ──┴──► shared ──► domain crates (voxel, content, economy)
```

- **Domain crates do not depend on Bevy.** They hold pure data structures and
  rules that are unit-tested and benchmarked in isolation. See
  [ADR 0003](adr/0003-pure-domain-crates.md).
- **`shared` is the only crate that knows the wire protocol.** Networking
  library types stay behind it, see [ADR 0002](adr/0002-networking-lightyear.md).
- **`client` and `server` never depend on each other.** A hosted world composes
  both plugins in one `App`; neither assumes the other is present.
- **Binaries only compose plugins.** They choose runtime plugins (window,
  schedule runner, logging) and add `ClientPlugin` or `ServerPlugin`, with no
  game logic of their own.

## Plugins

Every game system is a Bevy plugin owned by the crate responsible for it.
Library plugins never add runtime plugins such as `DefaultPlugins`,
`MinimalPlugins` or `LogPlugin`, so they stay composable in tests and in the
hosted mode.

`SharedPlugin` configures what both sides must agree on, starting with the
fixed simulation rate. `ServerPlugin` and `ClientPlugin` each add it only when
it is not already present.

## Simulation timing

Gameplay runs in `FixedUpdate` at `messoria_shared::tick::TICK_RATE_HZ`. Rendering
and input run in `Update` at the display rate. Client-side prediction replays
the same fixed-step systems the server runs, which is why those systems live in
`shared`.

## Content

Game content (items, crops, shops, prices) lives in data files under
`assets/data/`, never in code. Content is validated when it is loaded, so a
malformed file fails at startup rather than during play.
