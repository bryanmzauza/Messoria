# 0016 — Farm lots: land of one's own along the roads

**Status:** accepted. Settles where players settle, left open in
[ADR 0011](0011-structures-and-homes.md).

## Context

With individual money and many players, each needs land that only they
work: the design document asks for fenced lots of 64 meters, the first
free and the rest bought, shared by invitation, with gathering allowed
everywhere and nothing else worked outside them. Until now anybody could
dig, till and build anywhere outside the village, and a cabin went wherever
its deed was used.

The lots have to look the same on every peer, since fences are walked into
and predicted movement must collide with them as the server does, and they
have to be found again after a restart. The valley grows from the seed on
both sides ([ADR 0015](0015-a-larger-valley-grown-on-both-sides.md)), so
anything laid out with it costs nothing to replicate.

## Decision

- The lots are part of the valley's layout, the same in every world: three
  to a side along each of the four roads out of the village, 64 meters
  square, a lane apart, each with a four-meter gate in the fence on its road
  side. The roads run straight past them and wander only beyond; the ring
  road and the river keep clear of them. The scenery keeps off the fences,
  out of the gateways and out of the middle of each lot, where a cabin has
  room.
- Who owns a lot is a replicated `LotClaim`: the owner's key and the keys of
  the players they invited. Claims are made at the village notice board,
  as trades are made at a stall: the first is free, every other costs the
  village's price. Owners invite players who are in the world and take
  invitations back by key, so a guest need not be there to lose it. The
  world file keeps every claim.
- Ground is worked only on a lot by its owner or their guests: digging,
  raising, tilling, planting, watering, harvesting, building and opening
  chests. Outside the lots, and on lots of others, players walk, gather
  and trade. A home is built only on a lot of one's own; other structures
  on any lot one may work. The ground along a fence is kept, like the
  ground under a tree.
- Fences are obstacles grown with the columns they stand on, on every peer,
  and drawn by clients for the columns they draw in full, section by section
  along the ground, with a gateway at each gate; nothing of them crosses the
  network.
- Players are named on screen from their keys until they choose names.

## Consequences

- A newcomer's first stop is the notice board; the deed they arrive with
  waits until they have a lot. The loadtest's farming bots walk there,
  claim a lot and walk to it by the road.
- The valley changed shape around the village (straight roads, the ring
  road farther out, the river farther east or west), so worlds saved before
  cannot be resumed, and the world file is at format 5.
- Twenty-four lots serve the players the MVP is for; a larger server needs
  more rings of lots, which the layout can grow without changing anything
  else.
- The village's square is the one road out for bots and players alike;
  streets come with the larger village.
