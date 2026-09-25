# 0010 — Gathered scenery is saved by seed cell, over the generated valley

**Status:** accepted. Props are no longer replicated entities; what was
gathered is sent with the terrain, see
[ADR 0015](0015-a-larger-valley-grown-on-both-sides.md).

## Context

Players now fell trees, break rocks and pick berries. What they gathered
must survive a restart: a cleared rock stays cleared, a stump stays a stump
until the tree grows back. ADR 0008 chose not to save scenery at all, since
the seed grows the same props every time, so a save only needs to say which
of those props were gathered.

That needs a stable name for each prop, and the scattering to really be the
same every time. It was not quite: props were scattered after the saved
terrain and fields were loaded, so digging, raising or tilling could change
which props the seed grew on the next start.

## Decision

- The seed scatters props over the valley as it was generated, before the
  changes players made to the terrain are laid over it, and never looks at
  fields. The terrain's startup is split in two steps (`WorldBuilding`) with
  the scattering between them. Fields cannot be made on a prop's ground, so
  nothing overlaps.
- A kind of prop grows at most one prop per cell of its scattering grid, so
  the kind's text id and the cell name a prop. The world file lists the
  props gathered, by that name, with the day they were gathered (world
  format version 2; version 1 files read as nothing gathered).
- Gathering is described in `scenery.ron`, per kind: the tool (or by hand),
  the strikes it takes, what it yields, what remains (nothing, itself
  without its fruit, or another model such as a stump) and after how many
  days it grows back. Only what leaves something standing may grow back, so
  a prop never reappears on ground that was freed.
- A gathered prop keeps its entity and gets a replicated `Gathered`
  component. What leaves nothing stops being an obstacle and frees its
  ground, which can then be dug and tilled.

## Consequences

- Saves stay small: one line per gathered prop, however large the valley.
- Adding a kind of prop, or changing a kind's scattering, changes which
  props the seed grows, as it already did; gathered props of kinds that no
  longer exist are forgotten and reported.
- Scattering on the generated valley costs nothing more, since the valley
  is generated anyway before the saved chunks replace parts of it.
