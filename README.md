# Messoria

A cooperative 3D farming game set in a fantasy village. It pairs the daily rhythm
of a farming sim with smooth, shapeable voxel terrain, and it is built for
playing with friends, from a small group joining by access code to communities
on dedicated servers.

Written in Rust with [Bevy](https://bevyengine.org).

## Status

Early development. Players share a farm valley over the network, hosted from
the game or on a dedicated server, reshape its smooth voxel terrain with a
shovel, farm crops through the seasons, sell their harvests at the village
grocer, whose prices respond to what everyone sells, and live through days and
nights that end when they go to sleep. Saving the world is next. See the
[roadmap](docs/ROADMAP.md) for what is done and what comes next.

## Building

Requirements:

- Rust, installed through [rustup](https://rustup.rs). The toolchain version is
  pinned in `rust-toolchain.toml` and installed automatically.
- Linux only: `libudev-dev`, `libwayland-dev` and `libxkbcommon-dev` (or your
  distribution's equivalents).

```sh
cargo client    # run the game in a private local world
cargo server    # run a headless dedicated server
cargo bots      # connect a bot player to a local server
cargo lint      # Clippy across the workspace, warnings as errors
cargo test --workspace
```

For faster incremental builds while iterating on the client, enable Bevy's
dynamic linking:

```sh
cargo run -p messoria-game --features dev
```

## Playing together

Arguments go after `--`:

```sh
cargo client -- --host                       # open your world to others (UDP port 5717)
cargo client -- --connect 192.168.0.10       # join a hosted world or a dedicated server
cargo server -- --port 5717                  # dedicated server
cargo server -- --sleep-percent 50           # share of players that must sleep to end the day
cargo server -- --start-time 21:00           # start the world's clock at a given time
cargo server -- --minute-length 20           # make days pass quickly, for testing
cargo bots -- --server 127.0.0.1 --bots 4    # add simulated players
cargo bots -- --bots 2 --farm                # bots that farm and log their harvests
```

The host must allow the port through their firewall, and players outside the
local network need it forwarded on the router until access codes arrive (see
the roadmap). `--simulate-latency <ms>` on `--connect` and on the bots delays
packets from the server, for testing under latency.

Controls: click the window to capture the mouse, `W` `A` `S` `D` to move,
`Space` to jump, `1` to `0` or the mouse wheel to pick the held item, left and
right mouse buttons to use it, `E` to harvest a ripe crop or, at a stall, to
open its shop, `Tab` to open the backpack (click two slots to move items, and
give money to other players beside it), `Z` to sleep or get up (from 18:00),
`F5` to switch between first and third person, `Esc` to release the mouse.
The shovel digs and raises ground; the hoe tills a field, seeds are planted in
it and the watering can waters it; food is eaten. The village lies straight
ahead of where players arrive; its grocer opens from 09:00 to 17:00, except on
Sundays.

Game content lives in `assets/data/`: items in
[`items.ron`](assets/data/items.ron), crops in
[`crops.ron`](assets/data/crops.ron), and shops and market prices in
[`shops.ron`](assets/data/shops.ron). The game refuses to start, naming the
problem, if a data file is invalid.

## Repository layout

```
crates/   libraries: domain rules (calendar, content, economy, farming, inventory, voxel), shared
          networking and simulation, server, client
bins/     executables: game client and dedicated server
tools/    development tools: load-testing bots
assets/   game content and, later, art and audio
docs/     design document, architecture notes, decision records
```

The [architecture overview](docs/architecture.md) explains how the crates fit
together.

## Documentation

- [Game design document](docs/design/game-design.md)
- [Roadmap](docs/ROADMAP.md)
- [Architecture](docs/architecture.md)
- [Decision records](docs/adr/)

## License

Copyright (c) 2026 Bryan Munaretto Zauza. All rights reserved. The source is
published for reference only; see [LICENSE](LICENSE).
