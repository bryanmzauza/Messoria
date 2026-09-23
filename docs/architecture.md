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
  shared/            messoria-shared            Networking setup, protocol, deterministic simulation
  server/            messoria-server            Authoritative game logic
  client/            messoria-client            Rendering, input, camera, UI, audio
bins/
  game/              messoria                   Game client executable
  dedicated-server/  messoria-dedicated-server  Headless server executable
tools/
  loadtest/          messoria-loadtest          Headless bot players
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

## Dependency rules

```
bins, tools ──► client ──┐
           └──► server ──┴──► shared ──► domain crates (voxel, content, economy)
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

## Simulation timing

Gameplay runs in `FixedUpdate` at `messoria_shared::tick::TICK_RATE_HZ`. Rendering
and input run in `Update` at the display rate. Client-side prediction replays
the same fixed-step systems the server runs, which is why those systems live in
`shared`. Characters simulated locally are blended between ticks for display;
remote characters are interpolated between server snapshots.

## Content

Game content (items, crops, shops, prices) lives in data files under
`assets/data/`, never in code. Content is validated when it is loaded, so a
malformed file fails at startup rather than during play.
