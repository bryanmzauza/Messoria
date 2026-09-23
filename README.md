# Messoria

A cooperative 3D farming game set in a fantasy village. It pairs the daily rhythm
of a farming sim with smooth, shapeable voxel terrain, and it is built for
playing with friends, from a small group joining by access code to communities
on dedicated servers.

Written in Rust with [Bevy](https://bevyengine.org).

## Status

Early development. The foundation is in place; networking is next. See the
[roadmap](docs/ROADMAP.md) for what is done and what comes next.

## Building

Requirements:

- Rust, installed through [rustup](https://rustup.rs). The toolchain version is
  pinned in `rust-toolchain.toml` and installed automatically.
- Linux only: `libudev-dev`, `libwayland-dev` and `libxkbcommon-dev` (or your
  distribution's equivalents).

```sh
cargo client    # run the game
cargo server    # run a headless dedicated server
cargo lint      # Clippy across the workspace, warnings as errors
cargo test --workspace
```

For faster incremental builds while iterating on the client, enable Bevy's
dynamic linking:

```sh
cargo run -p messoria-game --features dev
```

## Repository layout

```
crates/   libraries: shared protocol and simulation, server, client
bins/     executables: game client and dedicated server
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
