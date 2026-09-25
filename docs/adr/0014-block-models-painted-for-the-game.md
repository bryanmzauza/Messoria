# 0014 — Block models painted for the game, foliage tinted and swayed by name

**Status:** accepted. Supersedes where models come from in
[ADR 0008](0008-scenery-and-art.md) and [ADR 0012](0012-characters-village-and-icons.md).

## Context

The characters and the ground are pixel art painted for Messoria (ADR 0012,
ADR 0013), while trees, rocks, houses, crops and items still came from CC0
packs: smooth low-poly shapes in flat colors, recolored through the palette.
Side by side they read as two games. Each pack also has its own scale, so
data files carried factors from 1.4 to 5 to bring models to size, and houses
were assembled from wall and roof tiles meant for another game's grid.

The look we want is the one the characters set: chunky shapes built from
boxes, with textures of square texels that match the ground's size, and
foliage that is alive, turning with the seasons and moving in the wind.

## Decision

- Every model is built for the game from boxes, with a few flat quads for
  grass and triangles for gable ends, and painted in pixel art: tops lit,
  sides shaded downward, edges darker, tones picked in steps. The world is
  painted at 32 texels to the meter, like the ground; small things (crops,
  items) at 64. Each model is one glTF binary with its texture atlas
  embedded and sampled nearest, so the engine loads it like any other
  model and it can be opened in common editors.
- Models are made in meters, at life size, their front toward -z and their
  base at the origin. Data files place them at a scale of about 1: whole
  buildings instead of tiles, stalls, wares and held items without factors.
  Held items are only shrunk when longer than the hand allows; plants in
  fields are drawn a little larger than life, so rows read from afar.
- Materials keep their own textures. The palette, by material name, tints
  the texture of foliage, which is painted in grays, and its seasons change
  the tint: green in spring, orange and gold in autumn, frosted white in
  winter, with evergreens staying dark.
- The palette also names the materials that sway, and how far their tops
  move. Those are drawn with a material extending the standard one, whose
  vertex shaders (for the main pass and for the depth, shadow and motion
  passes) push each vertex along the wind by its height over the base of its
  plant. Ground cover merged into one mesh keeps each vertex's height over
  its own plant in a second texture coordinate.
- Grass and other flat plants are lit as if facing up from both sides, like
  the ground they grow from, so a tuft never turns a dark side to the sun.
- No model from an outside pack is left; the sound packs stay credited.

## Consequences

- The valley, the village, fields, tools and characters share one texel size
  and one way of shading, in every season.
- Adding a model means building it in the same style; a model from elsewhere
  would stand out, and would need its material names and scale to follow
  these rules.
- Each model brings its own texture, so props of different models no longer
  share materials; props of the same model still do, and ground cover is
  still merged per material within a chunk.
- Swaying costs a little vertex work in every pass for foliage only, and
  parts of one plant sway together only when their materials share a
  strength.
- Textures without mipmaps shimmer a little at a distance; temporal
  anti-aliasing smooths most of it.
