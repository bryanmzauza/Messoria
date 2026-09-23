# Messoria

A cooperative 3D farming game set in a fantasy village. It pairs the daily rhythm
of a farming sim with smooth, shapeable voxel terrain, and it is built for
playing with friends, from a small group joining by access code to communities
on dedicated servers.

Written in Rust with [Bevy](https://bevyengine.org).

## Status

Early development. Players can share a world over the network, host it from
the game or run a dedicated server, and walk around together. Terrain is next.
See the [roadmap](docs/ROADMAP.md) for what is done and what comes next.

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
cargo bots -- --server 127.0.0.1 --bots 4    # add simulated players
```

The host must allow the port through their firewall, and players outside the
local network need it forwarded on the router until access codes arrive (see
the roadmap). `--simulate-latency <ms>` on `--connect` and on the bots delays
packets from the server, for testing under latency.

Controls: click the window to capture the mouse, `W` `A` `S` `D` to move,
`Space` to jump, `F5` to switch between first and third person, `Esc` to
release the mouse.

## Repository layout

```
crates/   libraries: shared networking and simulation, server, client
bins/     executables: game client and dedicated server
tools/    development tools: load-testing bots
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
