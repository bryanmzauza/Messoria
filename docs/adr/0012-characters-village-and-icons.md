# 0012 — Figures of boxes posed in code, a village from data, icons rendered by the client

**Status:** accepted

## Context

Players saw each other as capsules, the village was two stalls on a paved
square, crops were colored shapes, and items were a swatch of color and a
name. The slice has to look like a game before a playtest: characters that
move and work, a village with buildings and shopkeepers, and items that are
recognized at a glance.

The look we want for characters is chunky and hand-painted: figures of
boxes with pixel-art skins, big heads, hands and boots, in the spirit of
Hytale. No free pack has villagers in that style, and a small cast of
models from a pack would make every tenth player a twin.

Characters are simulated on the server and predicted or interpolated on
clients (ADR 0001, ADR 0002); animation is purely a matter of looks and must
not add to what is replicated every tick. Item icons have to match the
models used everywhere else, including models that later packs add.

## Decision

- A character is a figure of ten limbs, each a few boxes turning at a
  joint (hips, neck, shoulders, elbows and knees), described in
  `characters.ron`: sizes in figure pixels and where each box's faces are
  painted from in a skin. Its proportions follow a character sheet: a head
  over a third of the height, a torso a fifth of it wide, slim arms and
  long legs. Clients build the meshes
  from that data. The figure has boxes for everything any outfit or
  hairstyle may need (a hood, a pouch, sleeves, boot cuffs, tufts of hair
  standing out of the head's outline, a bun, a beard); each layer paints the ones it uses and leaves the rest clear.
- The front of the head is also painted elsewhere in the skin for each
  expression: blinking, smiling, surprise and effort. A figure's face swaps
  between them by what the character does, with nothing replicated.
- Skins are laid in layers made for the game: a body, an outfit over it and
  hair, painted in grays and tinted with a hair color. Clients lay each
  look into one image when its layers have loaded. Every player is dressed
  from the wardrobe by their player id, so everyone sees the same look for
  the same player without choosing or sending it; shopkeepers' looks are
  data in `shops.ron`.
- Figures are posed in code rather than played from recorded animations:
  limbs swing with the distance walked, knees and elbows bending, figures
  lean into a run, tuck their legs in a jump and crouch on landing, sleepers
  lie in the nearest bed, and happenings,
  which now name the player who caused them, are acted out. Nothing about
  animation is replicated. The one addition is `Holding`, the item a
  character holds, which the server derives from the held slot sent with
  each input. Held models are fitted to the hand by their size.
- Crops at every stage, stalls and item models come from CC0 packs
  (ADR 0008); items, crops and shops name their models in data.
- The village is laid out in `village.ron`: where each shop's stall stands
  and which buildings stand around the square, as structures (ADR 0011).
  Village buildings are rebuilt from the data at every start, so the world
  file leaves them out and changing the layout needs no save migration.
- Item icons are rendered by each client at startup from the item models,
  by one camera per item into an image, on a render layer used by nothing
  else. Seeds show their bag with what they grow into.
- The first-person view draws the player's own arm, holding the item, with
  a second camera on its own layer, over the world's picture.

## Consequences

- A new outfit, hairstyle or skin tone is one painted layer and a line of
  data, and multiplies the looks players can have. New items need only
  models; icons and held items follow them without art to keep in sync.
- Posing in code keeps figures and their motion small and data-free, but
  every new action is written as code, not recorded in a tool.
- Animation cannot disagree between peers in any way that matters, since it
  never affects the simulation; remote characters may start an action a
  little late, with the happening.
- Icons cost a few seconds of rendering at startup, while shaders compile;
  until then a slot shows only its count. Their lighting is their own, not
  the valley's time of day.
- Moving a village building is a data change that takes effect at the next
  start, for every world.
