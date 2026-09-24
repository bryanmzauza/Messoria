# 0011 — Structures players build, and every player's home

**Status:** accepted

## Context

Players need a place of their own: a house with a bed to sleep in and
chests to keep what they gather. The valley has no plots yet, and deciding
their number, size and sharing is still open (see the design document), so
players have to be able to choose where they live without that decision.

A house is something to walk into. Characters are predicted on the client
with the same movement code the server runs (ADR 0001), which only knows
terrain and upright obstacles, so floors and walls must fit that model.
Later milestones will add workbenches, sprinklers and village buildings,
which are placed and kept the same way.

## Decision

- Structures are content, in `structures.ron`: the ground they keep, the
  models they are drawn with, the boxes nobody walks through, lamps, the
  structures built along with them and what they are for (a bed, storage).
  Each is laid out in its own frame, front toward -Z, and built turned by a
  quarter-turn facing, its front toward whoever builds it.
- A structure is built by using its item on the ground, aiming at where its
  front will be, so the builder always stands outside it. The server checks
  the ground: outside the village, not too uneven, and clear of props,
  fields, other structures and people, around it too where the ground eases
  back. Small structures may stand on the floor of one that levelled its
  ground, such as a chest in a cabin.
- A cabin levels the terrain under it to one of the shovel's levels. The
  levelled terrain is its floor, so characters walk in on the ground as
  anywhere else and the floor's models are only drawn over it. Walls and
  furniture are boxes among the obstacles, built from the replicated
  `Structure` on every peer.
- One structure is every player's home. Players without a home, and
  without the item to build one, are given it when they arrive; a player
  owns one home. Sleeping happens only in a bed, and whoever collapses at
  02:00 wakes up beside the bed of their home.
- Chests replicate what they keep (`Stored`), and anyone standing at one
  can move stacks in and out. Clients name the chest by its position, as
  they name a shop by its id, rather than send entity ids.
- The world file keeps every structure, its owner and what it keeps
  (format version 3); the levelled terrain is kept with the edited chunks.

## Consequences

- Where players settle stays open until plots are designed, and the rules
  that keep structures apart will carry over to plots.
- Floors are terrain, so a structure cannot have a floor above the ground,
  such as a second storey or a floor on stilts, without teaching movement
  about floors.
- Obstacles now come in two shapes, and new kinds of structures need only
  data: models, boxes and a purpose.
- A structure cannot be taken down yet; nothing needs that before building
  with materials arrives.
