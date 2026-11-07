# 0018 — Villagers with days of their own

**Status:** accepted. The shopkeepers of
[ADR 0012](0012-characters-village-and-icons.md) become villagers.

## Context

The village has streets, shops and houses but nobody living in it: the
shopkeepers were figures each client stood behind the stalls, with no life
beyond greeting a trade. The MVP asks for eight to ten named villagers with
schedules by hour, weekday, season and weather, seen by everyone walking
the village, keeping shop, sitting, tending gardens and going home at
night, who will soon be talked to and given gifts.

Whatever villagers do must look the same to every player, so it has to
happen on the server, and it must cost the server little when nobody is
there to see it.

## Decision

- Villagers are content: `villagers.ron` names each one, dresses them from
  the same skins as players, gives them a house (a labeled building in
  `village.ron`), the shop they keep, and a day: slots from one time to
  another, each limited if wanted to weekdays, seasons and rain or dry
  weather, saying where they are (`At` a spot), that they stroll between
  spots, or that they are at home. The first slot that fits the moment
  holds; otherwise they are at home.
- Spots are content too: labeled places in `village.ron` with a facing and
  a way of holding oneself (standing, sitting, tending the ground), and the
  spots to pass on the way in, such as a shop's door. The paths villagers
  walk are a small graph of points along the streets and lanes, around the
  fountain and up to the shops' doorsteps. A route is the way out of the
  spot one stands at, the shortest way along the paths, and the way in to
  the next.
- The server moves villagers as it does nothing else: straight toward
  each waypoint at a walking pace, pushed out of obstacles, on the
  valley's ground, giving up on a waypoint it cannot reach. Each villager
  is a replicated character (position, heading, velocity) with what it is
  doing (`Doing`), interpolated on clients and placed in the rooms of the
  columns it walks through like a player. Clients dress villagers from the
  content and animate them as they animate players, seated or bending to
  the ground as the server says, out of sight when indoors.
- While no player is within a few hundred meters of the village, nobody
  sees villagers walk: they are moved straight to where their schedule
  has them, once every couple of seconds, so a far-off server pays almost
  nothing for them.
- Houses have no interiors, so going home means standing at the door and
  being out of sight until the next slot.

## Consequences

- A villager's whole life is in two data files; adding one means a look, a
  house and a day. Shops no longer carry a look of their own.
- Villagers walk through players and players through villagers: making
  villagers obstacles would make predicted movement disagree with the
  server's by the lag of interpolation.
- Schedules are checked only for their form; two villagers sent to one
  bench at one time would stand in each other.
- Following a villager through a day is a test, so a change to the
  village's layout that cuts a path shows up before anyone plays.
