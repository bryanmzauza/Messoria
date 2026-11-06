# 0017 — A village to walk into

**Status:** accepted. Builds on [ADR 0011](0011-structures-and-homes.md),
whose structures the village's buildings are, and
[ADR 0012](0012-characters-village-and-icons.md), which laid out the first
square.

## Context

The village was a square with two stalls, two closed shops and a pair of
cottages, fourteen meters across. The MVP asks for a village worth walking
through: streets, houses for the villagers who arrive next, a tavern, a
tailor, a barber and a blacksmith, with interiors for the shops and the
tavern. Characters are predicted on every client with movement that knows
only terrain and upright obstacles ([ADR 0009](0009-action-feedback-and-collision.md)),
so interiors have to be made of the same walls and floors as the cabin.

## Decision

- The village grows to a disc of 44 meters of level ground, paved with a
  square in the middle and streets across it: one toward each point of the
  compass, where the roads out of the village begin, and two lanes across.
  The paving is part of the valley's shape (`messoria_worldgen::paved`), so
  the terrain carries it on every peer. Where players arrive is now the
  village's south street.
- The grocer and the carpenter keep their stalls on the square: an open-air
  market, in front of their shops. The tavern, the tailor, the barber and
  the blacksmith are buildings at the four corners of the square, walked
  into through their doors: each is one structure whose walls are solids in
  pieces around the door, whose furniture is solids too, and whose floor is
  the paved and leveled village ground under a floor drawn over it. Their
  counters are their shops' stalls (`Shopfront`), standing inside, so
  trading inside works exactly as at a stall on the square, with the keeper
  behind the counter and wares on it. Lamps hang inside, lit at night like
  the lanterns.
- Villagers' houses line the two lanes, with garden beds beside them, and a
  well stands on the square. Houses are closed: nobody goes in.
- The tavern sells bread and stew, food worth more than raw produce, and
  buys some of what the farms grow; the blacksmith sells tools and buys
  stone; the carpenter keeps to wood and chests. The tailor and the barber
  open their doors now and get their trade with looks of one's own.
- Seen from outside the character, the camera stays this side of any wall
  or ground between it and the character's head, so rooms are seen from
  within, and the character is hidden when the camera is pushed up against
  it. Ground cover never grows on a building's floor.
- The village is laid out in `village.ron` as before; the buildings and
  their interiors are block models built like everything else
  ([ADR 0014](0014-block-models-painted-for-the-game.md)).

## Consequences

- Every door that opens leads into a furnished room, and a shop's hours and
  trade are the same whether its counter is a stall or stands indoors.
- Bots and newcomers leave the village by its streets; the loadtest's
  farming bots follow fixed waypoints through the square.
- Houses have no interiors, so villagers go home at night by going in at
  the door and out of sight, until houses are furnished after the MVP.
- The village is a fixed layout: a larger server would want a larger
  village, which means more models and more content, not more code.
