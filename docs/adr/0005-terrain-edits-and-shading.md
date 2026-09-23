# 0005 — Terrain edits move ground between levels; terrain is flat-shaded

**Status:** accepted. Supersedes the edit shape and shading in
[ADR 0004](0004-terrain-representation.md).

## Context

The first shovel subtracted a sphere from the distance field and added one to
raise. On a one-meter grid, a few digs made deep pits with steep, faceted
walls. The smooth normals and blended vertex colors made those pits look dark
and smeared, and repeated digs stacked into craters. A farm needs digging to
level ground, cut terraces and shape ponds, not to carve holes.

## Decision

- An edit moves the ground up or down within a disc instead of carving a
  shape. Each column of samples under the disc shifts vertically, so the
  ground keeps its shape and only its height changes. The ground under a brush
  is treated as a heightfield; only the topmost surface near the target moves.
- Edits bring ground to levels shared by the whole world, at every multiple of
  the brush's step (half a meter for the shovel). The middle of the disc moves
  to the next level below or above the target, and the movement fades to
  nothing at the rim. Ground never moves past that level.
- Ground that moves most of the way takes on the edit's material: lowering
  exposes what lies under grass, and raising builds with soil.
- Terrain is rendered flat-shaded. Each triangle is lit as one facet and
  colored by one material, with a small brightness variation keyed to its
  position. Faces too steep to hold grass show the soil beneath.
- Surface meshes carry no normals. Shading is the renderer's choice.

## Consequences

- Digs made at the same level, by anyone, join into flat ground. Digging along
  a slope cuts terraces, and digging where a floor ends extends it.
- A single use moves ground by at most one step, so holes deepen predictably
  and never collapse into craters.
- Terrain edits cannot make overhangs or tunnels. Mines will need their own
  representation or brush when they arrive.
- The 1 m grid shows as clean facets instead of as smeared lighting, which is
  the intended look.
- A field is lost only when the samples at its tile's corners move. Movement
  further out only tilts the ground near its edges, which the depth of the soil
  drawn for it hides.
