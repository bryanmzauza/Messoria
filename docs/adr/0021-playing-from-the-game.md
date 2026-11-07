# 0021 — Playing from the game

**Status:** accepted.

## Context

Until now a world was chosen on the command line: the game started
straight into `saves/local`, hosted it only when told to, and joined a
server by address. For anyone who downloads a release and double-clicks
the game, that is no way in. The MVP wants a title screen that lists and
starts worlds, hosts one for friends, joins by address and remembers
where, a game menu that opens and closes the world and says who is in it,
a clear way back when a connection fails or a server turns the player
away, and a card of controls.

## Decision

- The title screen is a small app of its own, in the same executable: run
  with no world or address, the game shows it, over a painting of the
  valley made with the game itself. Starting or joining a world runs the
  executable again with the arguments for it and ends the title instance;
  the game comes back the same way when the player leaves, with a word on
  why if it was sent away. The server and the client were built to start
  with their world and are not taken apart and rebuilt at runtime; a
  second process costs a moment of black window at each change, and
  nothing else.
- A world started from the title screen listens on the default port from
  the start, closed to guests unless hosted on purpose, so its host can
  open it from the game menu without starting over; opening and closing
  are a message from the host's own client, and closing sends every guest
  away. A world started from a terminal without `--host` listens on
  loopback only, as before.
- Every client says hello with the protocol's version once connected. The
  server only gives a character to a client it admits: the right version,
  the world open to guests (the host's own client always is) and room
  left. Anyone else is told the reason and leaves on its own. The
  transport's own protocol id no longer changes with the game, since a
  transport that drops packets cannot say why; the version is ours to
  check, and to explain.
- The clock entity carries whether the world is open, on which port, and
  the names of everyone in it, for every client's menu.
- Worlds are folders under the saves folder, listed with their clock and
  when they were last played; a new one takes a name and a seed, if
  wanted. The settings remember the last world and the addresses joined
  lately. A world is deleted from the title after a second thought.
- The card of controls opens on a player's first arrival in a world, once
  they have made their look, and on F1 after that. Players' names float
  over their heads and name them wherever players are listed, from the
  name they introduced themselves with.

## Consequences

- One executable, three ways to run it: the title, a hosted world, a
  joined one. The dedicated server stays its own executable.
- Rejections are honest but not enforced at the transport: a client that
  ignores being sent away stays connected without a character, seeing no
  more than the world's few global things. That is good enough for a game
  among friends.
- The title screen needs no content loaded, so it opens at once.
