# Messoria

A cooperative 3D farming game set in a fantasy village. It pairs the daily rhythm
of a farming sim with smooth, shapeable voxel terrain, and it is built for
playing with friends, from a small group joining by access code to communities
on dedicated servers.

Written in Rust with [Bevy](https://bevyengine.org).

## Status

Early development. Players share a wooded farm valley over the network,
hosted from the game or on a dedicated server, reshape its smooth voxel
terrain with a shovel, farm crops through the seasons, sell their harvests at
the village grocer, whose prices respond to what everyone sells, and live
through days and nights that end when they go to sleep, under a sky that
turns with the hours and a valley that turns with the seasons. They gather
wood and stone, build cabins, and see each other as blocky, hand-painted
characters, each dressed their own way, with their tools in hand, around a
village square with shops and shopkeepers.
The valley is lit and its ground painted to match. Worlds are saved and
resume where they stopped. Art of our own for the whole valley is next, then
music and a playtest. See the [roadmap](docs/ROADMAP.md) for what is done and what comes next.

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
cargo client -- --world saves/farm           # play the world saved in another folder
cargo client -- --connect 192.168.0.10       # join a hosted world or a dedicated server
cargo server -- --port 5717                  # dedicated server
cargo server -- --sleep-percent 50           # share of players that must sleep to end the day
cargo server -- --world saves/community      # folder the world is saved in
cargo server -- --start-time 21:00           # start a new world's clock at a given time
cargo server -- --minute-length 20           # make days pass quickly, for testing
cargo bots -- --server 127.0.0.1 --bots 4    # add simulated players
cargo bots -- --bots 2 --farm                # bots that farm and log their harvests
```

Worlds are saved in `saves/`: the game's own world in `saves/local`, a
dedicated server's in `saves/world`. They are saved every few minutes, at
dawn and on exit, and resume where they stopped. The game keeps who you are in
`saves/profile.ron`, so servers recognize you when you come back, and your
options in `saves/settings.ron`.

The host must allow the port through their firewall, and players outside the
local network need it forwarded on the router until access codes arrive (see
the roadmap). `--simulate-latency <ms>` on `--connect` and on the bots delays
packets from the server, for testing under latency.

Controls: click the window to capture the mouse, `W` `A` `S` `D` to move,
`Shift` to sprint, `Space` to jump, `1` to `0` or the mouse wheel to pick the
held item, left and right mouse buttons to use it, `F` to harvest a ripe crop,
pick berries, open a chest, go to bed (from 18:00) or get up, or at a stall
open its shop, `E` to open the backpack (click a stack to pick it up and a
slot to put it down, and give money to other players beside it), `F5` to cycle
between first person, third person from behind and from the front, `Esc` for
the game menu (options, and saving and quitting). What the crosshair is on is
named under it, and the game says why when an action does nothing.

Every newcomer is given a cabin deed: used on open ground, aimed where the
door should be, it builds their cabin, with a bed and a chest inside. The
carpenter sells more chests. Players sleep in beds, and whoever is still
up at 02:00 collapses and wakes up at home.
The shovel digs and raises ground; the hoe tills a field, seeds are planted in
it and the watering can waters it; food is eaten. The axe fells trees for
wood, leaving stumps that grow back in a week, and the pickaxe breaks rocks
for stone. The village lies straight ahead of where players arrive: at its
stalls on the square, the grocer buys crops and sells seeds from 09:00 to
17:00, and the carpenter buys wood and stone and sells tools from 08:00 to
18:00, both closed on Sundays.

Game content lives in `assets/data/`: items in
[`items.ron`](assets/data/items.ron), crops in
[`crops.ron`](assets/data/crops.ron), shops and market prices in
[`shops.ron`](assets/data/shops.ron), trees, rocks, ground cover and what
gathering them gives in [`scenery.ron`](assets/data/scenery.ron), what
players build in [`structures.ron`](assets/data/structures.ron), how the
village is laid out in [`village.ron`](assets/data/village.ron), how
characters are built and dressed in
[`characters.ron`](assets/data/characters.ron), from the skins in
[`assets/skins/`](assets/skins),
and the colors of everything, season by season, in
[`palette.ron`](assets/data/palette.ron). The game refuses to
start, naming the problem, if a data file is invalid. Models come from CC0 art
and sound packs, credited in [`assets/CREDITS.md`](assets/CREDITS.md).

## Repository layout

```
crates/   libraries: domain rules (calendar, content, economy, farming, inventory, save, voxel),
          shared networking and simulation, server, client
bins/     executables: game client and dedicated server
tools/    development tools: load-testing bots
assets/   game content, models and sounds
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
