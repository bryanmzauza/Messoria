# 0009 — Feedback on actions, and collision without a physics engine

**Status:** accepted

## Context

Until now an action the server refused simply did nothing: a hoe on the
village square, a shovel with no energy left, a trade after closing time. A
new player had no way to tell a refusal from a bug. Actions that went through
were silent too, and only the changed terrain or field showed them, and only
to whoever was looking.

Characters also walked through trees, rocks and stalls. Movement is predicted
on the client with the same code the server runs (ADR 0001), so whatever
stops a character has to be known identically on both sides.

## Decision

- The server tells players about actions with two messages on their own
  channel. A `Notice` goes to the player whose action was refused, naming a
  reason they can act on. A `Happening` (what happened, and where) goes to
  every client, which plays a sound there and throws particles.
- Game systems never touch the network to do this. They write the server's
  own `Tell` and `Show` Bevy messages where they refuse or carry out an
  action, and one module sends them on. Requests no honest client makes
  (targets out of reach, malformed slots) are still refused silently.
- Clients check nothing on the player's behalf that the server would refuse
  for a reason worth telling: the request goes out, and the notice comes back.
- Obstacles are upright cylinders: a trunk, a rock, the posts of a stall's
  counter. Server and clients build the same set from the replicated props
  and stalls, and movement pushes a body out of any cylinder it overlaps,
  undoing the step if the push would put it into the ground.
- Settings (volume, mouse sensitivity, field of view) are the player's, not
  the world's, and live in `saves/settings.ron` next to the profile.

## Consequences

- Every refusal a player can cause has a sentence, which also documents the
  rules in one place (`Notice`'s `Display`).
- Sounds and particles follow from the same messages, so what one player
  does is heard and seen by everyone near it, in every topology.
- Cylinders are cheap to test against and deterministic, and need no physics
  engine or its fixed-point care. They only suit round, upright things;
  buildings with walls will need boxes, added the same way.
- The menu does not pause the world: it is shared, and a host serves the
  other players while in it. Pausing a world with a single player can come
  later without changing this.
