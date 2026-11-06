# 0019 — Talking, gifts and friendship

**Status:** accepted. Builds on
[ADR 0018](0018-villagers-with-days-of-their-own.md).

## Context

Villagers walk the village and keep its shops, but there is nothing to do
with them. The design wants getting to know them to be part of the game:
talking, gifts, and a friendship per player that grows, warms what they say
and brings small things back. Two players befriending the same villager
must each have a friendship of their own, kept with their character.

## Decision

- What villagers say is content: each has lines in `villagers.ron`, some
  for any moment and some only in a season, in rain or dry weather, at a
  time of day, or to players with enough hearts of friendship. When a
  player talks to a villager, the most particular lines that fit the
  moment are picked among, spread over the days and the villagers, so
  everyone hears something fitting and a close friend hears what others do
  not. Each villager also has lines for a gift they love, like (or shrug
  at) or dislike, and lists of what those are.
- Friendship is counted in points, a hundred to a heart, up to ten hearts.
  The first talk each day earns a few; a gift earns more or less by how
  the villager takes it, one gift a day. Reaching new hearts brings a
  present from the villager (seeds, food, materials, as the content says),
  a friend's price at the counter of the shop they keep, and warmer lines.
- Talking and giving are messages to the server, which answers with what
  the villager said, the points, the reaction and any present, and tells
  each client its friendships whenever they change. The villager turns to
  whoever talked to them for a while. Friendships live on the character
  (`Bonds`) and in the player's file, which is at format 2; older files
  read as having none.
- The client opens a window with the villager's words, the hearts, a
  button to give what the player holds and goodbye; `V` lists every
  villager with their hearts, what they keep, where they are and whether
  they were talked to today. A shopkeeper behind a counter is talked to
  from the shop window, since the counter is what the crosshair meets.

## Consequences

- A villager's whole character, from schedule to what they say and want,
  is in one data file; adding a villager adds no code.
- Friendship has no decay and no events yet: heart events and recipes come
  after the MVP, as the roadmap says.
- Lines are chosen on the server, so everyone hears the same line of the
  same villager on the same day; two players side by side get the same
  words, which suits a village where news travels.
