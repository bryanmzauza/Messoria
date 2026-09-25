# 0008 — Scenery from the seed, art from CC0 packs through one palette

**Status:** accepted. Where models come from and how the palette colors
them are superseded by [ADR 0014](0014-block-models-painted-for-the-game.md).

## Context

An empty valley does not feel like a place worth farming in. It needs trees,
rocks, bushes and plants underfoot, and they must look like one world even
though they come from art packs made by other people. Later milestones will
let players chop trees and break rocks, so those have to be authoritative and
saveable, while grass underfoot is pure decoration that could number in the
tens of thousands. Seasons should change the look of all of it.

## Decision

- Models come from CC0 packs, starting with Kenney's Nature Kit, kept in
  `assets/models/<pack>/` with the pack's license and credited in
  `assets/CREDITS.md`.
- Every model is drawn in the colors of `palette.ron`, by material name: a
  mesh whose material has a name in the palette is drawn with one material
  shared by everything of that name. The terrain's ground is colored through
  the same palette. Seasons override colors, so one set of models covers the
  whole year: leaves turn in autumn and snow lies on grass in winter.
- Props (trees, bushes, rocks, logs) are defined in `scenery.ron` and
  scattered by the server from the world's seed: a grid per kind, a chance per
  cell raised in groves and lowered in clearings, rules for the ground and
  its slope, and room between props. The village, the arrival clearing and
  fields stay free. Props are replicated entities, and the ground under them
  cannot be dug, raised or tilled.
- The same seed scatters the same props, so saves do not store them. Props
  players remove will be saved as removals when gathering arrives.
- Ground cover (grass tufts, flowers, mushrooms) is grown by each client for
  the chunks near its camera, from the chunk's position, only on flat grass
  with nothing on it. A chunk's plants are merged into one mesh per palette
  material.

## Consequences

- Art from different packs, and the terrain, change together with the
  season, and retuning the look of the game means editing one data file.
- Adding a kind of prop or cover is a data change; the scatter rules and the
  palette apply to it without code.
- Changing the scatter rules or the scenery file moves the props of existing
  worlds. That is acceptable while props are only scenery; once players can
  remove them, the saved removals must stay meaningful, which ties the
  scenery to a save version.
- Cover costs no bandwidth and disappears by itself where the ground is dug,
  tilled or paved, but it only exists near the camera.
