# 0012 — Rigged characters, a village from data, icons rendered by the client

**Status:** accepted

## Context

Players saw each other as capsules, the village was two stalls on a paved
square, crops were colored shapes, and items were a swatch of color and a
name. The slice has to look like a game before a playtest: characters that
move and work, a village with buildings and shopkeepers, and items that are
recognized at a glance.

Characters are simulated on the server and predicted or interpolated on
clients (ADR 0001, ADR 0002); animation is purely a matter of looks and must
not add to what is replicated every tick. Item icons have to match the
models used everywhere else, including models that later packs add.

## Decision

- Characters, crops at every stage, stalls and shopkeepers, and item models
  come from CC0 packs (ADR 0008). Which model plays which part is data:
  `characters.ron` names the models, their scale, the node that holds items
  and how it grips them, and the animation each model plays for each thing
  a character does; items, crops and shops name their models.
- Clients animate characters from what they already receive: velocity picks
  idle, walking, running, jumping or falling, and happenings, which now name
  the player who caused them, are acted out. Nothing about animation is
  replicated. The one addition is `Holding`, the item a character holds,
  which the server derives from the held slot sent with each input.
- Every player's model is picked from their player id, so everyone sees the
  same character for the same player without choosing or sending it.
- The village is laid out in `village.ron`: where each shop's stall stands
  and which buildings stand around the square, as structures (ADR 0011).
  Village buildings are rebuilt from the data at every start, so the world
  file leaves them out and changing the layout needs no save migration.
- Item icons are rendered by each client at startup from the item models,
  by one camera per item into an image, on a render layer used by nothing
  else. Seeds show their bag with what they grow into.
- The first-person view draws the held item with a second camera on its own
  layer, over the world's picture.

## Consequences

- New characters, animations, crops and items need only models and data;
  icons follow the models without drawn art to keep in sync.
- Animation cannot disagree between peers in any way that matters, since it
  never affects the simulation; remote characters may start an action a
  little late, with the happening.
- Icons cost a few seconds of rendering at startup, while shaders compile;
  until then a slot shows only its count. Their lighting is their own, not
  the valley's time of day.
- Moving a village building is a data change that takes effect at the next
  start, for every world.
