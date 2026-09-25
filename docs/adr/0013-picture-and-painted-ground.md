# 0013 — A finished picture, and ground painted in pixel art

**Status:** accepted. Supersedes the shading in
[ADR 0005](0005-terrain-edits-and-shading.md).

## Context

The valley was drawn the way the engine draws by default: colors clipped at
white, no ambient occlusion, hard shadows, and a terrain of flat facets in one
color per material. Characters now wear pixel-art skins (ADR 0012), and next
to them flat-colored ground reads as unfinished. Flat shading was chosen to
hide the smeared lighting of smooth normals on a one-meter grid; painted
ground carries its own detail and no longer needs it.

## Decision

- The world camera renders in high dynamic range and finishes the picture:
  tonemapping that keeps the colors the art is painted in, bloom, screen-space
  ambient occlusion, temporal anti-aliasing (instead of multisampling) with
  temporally filtered soft shadows, and a color grade and exposure that follow
  the time of day and the weather. A haze around the camera lets the low sun
  cast beams at dawn and dusk.
- A graphics quality setting (low, medium, high) decides how much of that
  finish is drawn: ambient occlusion from medium up, sunbeams at high, and
  larger shadow maps at each step. It is kept with the player's settings
  (format version 2).
- Terrain is shaded smoothly. The mesher gives each vertex a normal from the
  gradient of the distance field, which chunks sharing a border agree on.
- Ground is painted with pixel-art textures in world space, 32 texels to the
  meter like the characters, by a shader on top of the standard material.
  Each ground material has four variants, picked per meter and flipped, laid
  from above on flat ground and from the side on slopes. Where materials meet
  the border is decided texel by texel, with noise, so it is drawn in pixels.
  Stone lies as paving on flat ground and stands as rock on steep faces;
  grass too steep to hold shows the soil beneath.
- Textures are painted in spring's colors. The palette's ground colors tint
  them in other seasons, as they differ from spring's, and snow covers the
  grass in winter. Changing season updates the material, not the meshes.

## Consequences

- The ground looks like part of the same world as the characters, and the
  same texel size keeps them consistent up close.
- Temporal anti-aliasing needs motion vectors: everything drawn in the world
  must write them, and effects without them may ghost.
- The first-person arm is drawn after the world's picture is finished, so its
  light is set to match the finished image rather than the world's.
- The finish costs frame time; weaker machines lower the graphics quality.
- Pits and cliffs are lit smoothly now; their texture, not facets, shows
  their shape.
