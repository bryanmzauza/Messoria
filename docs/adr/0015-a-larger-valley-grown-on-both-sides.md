# 0015 — A larger valley, grown from the seed on both sides

**Status:** accepted. Supersedes how the valley is generated in
[ADR 0004](0004-terrain-representation.md), and how scenery is scattered and
replicated in [ADR 0008](0008-scenery-and-art.md) and
[ADR 0010](0010-gathered-scenery.md).

## Context

The valley was 256 meters across, generated whole when the server started,
with its scenery (about a thousand props) replicated to every client as
entities. The farms planned next need land of their own for every player,
around a village, with woods and hills between: a world of about two
kilometers, sixty-four times the area.

At that size neither choice holds up. The whole terrain, generated at once,
would take hundreds of megabytes on the server and seconds to build.
Scattered props would number in the tens of thousands, and replicating each
as an entity costs the server time every tick for every client, whether or
not anything changed. Everything else the server replicates (fields,
buildings, characters) would reach every player wherever they are. The
land past the few hundred meters a client holds would simply not be drawn.

## Decision

- The valley's shape is a pure function of the world's seed, in a domain
  crate of its own (`messoria-worldgen`): broad rolling land, gentle across
  the farmland around the village and rising into hills and ridges past it;
  mountains closing the world at its edge; dirt roads graded into the ground,
  from the village to each point of the compass and in a ring around it; a
  river winding across the world, whose water only runs downhill, cutting
  gorges and crossing roads as fords. The village is leveled over the roads
  that leave it.
- The server generates terrain a column of chunks at a time, around each
  player, several columns at once on every core, and unloads columns nobody
  is near. Edited chunks are kept aside while their columns are unloaded and
  take the place of the generated ones when they load again. Clients are
  streamed whole columns, nearest first.
- Scenery is grown on each side from the seed, per column: a prop's place,
  kind and look come from its kind's grid, and where two would crowd each
  other the one first in a fixed order grows, deciding only from candidates
  within reach, so a column's props are the same whichever columns are
  worked out and in whatever order. No prop crosses the network. What
  players gathered is the only scenery state: the server keeps all of it and
  saves it, and sends each client what was gathered on every column it
  receives, then every change. Scenery grows thinner across the farmland
  than in the wilds.
- The world's seed is the first thing a client receives. With it, the
  client draws the land past its terrain coarsely out to the horizon, grows
  the tall props beyond its terrain for a few hundred meters, and draws the
  river's water, all without being sent any of it.
- Everything else that stands in the world is replicated through rooms: a
  room per column, entities in the room of the column they stand in (players
  moving from room to room), and each client in the rooms around its
  character, as far as it holds terrain. The clock and the market reach
  everyone.
- Worlds saved before the valley grew cannot be resumed: their terrain,
  scenery, fields and buildings belong to a valley the seed no longer grows.
  Loading one stops with a message saying so.

## Consequences

- The server only holds, generates and simulates what is near players, and
  a client only receives what it can see. Gathering a tree costs one small
  message to the clients near it.
- Server and clients must grow the same valley: any change to generation or
  scattering changes every world, and has to go with a new save version.
- A client far away does not see other players, their fields or their
  buildings; things that need everyone, such as a list of players to give
  money to, only show those nearby.
- Props far from a client are drawn as they grew, even if someone gathered
  them, until the client comes near enough to be told.
- The farmland near the village is gently rolling and open, ready to be cut
  into farm lots.
