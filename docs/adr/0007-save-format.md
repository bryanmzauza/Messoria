# 0007 — Worlds saved as versioned files, referring to content by name

**Status:** accepted

## Context

A server must be able to stop and start again without losing anything: the
terrain players reshaped, their fields and crops, the market, the time of
day, and every player's character, whether they are online or not. Content
changes between versions of the game, and so will the save format. Players
must be recognized when they come back, before accounts exist.

## Decision

- A world is a folder: `world.ron` (seed, clock, market, fields),
  `terrain.bin` (the chunks that differ from the generated valley) and one
  `players/<key>.ron` per player. `messoria-save` reads and writes them and
  has no engine dependency.
- Generation is deterministic, so only changed chunks are saved and are laid
  over a freshly generated valley on load.
- Saves refer to items, crops and shops by their text ids. Whatever the
  content no longer defines is left out on load and reported, instead of
  failing or being misread.
- Every file starts with its format version. Readers match on it; an older
  version is read with its own types and upgraded to the current one, and a
  newer one is refused.
- Files are written under a temporary name and renamed over the old ones.
- The executable loads the save before building the app, as it does content.
  A save that cannot be read stops the program with the file and the
  problem, rather than starting a new world over it.
- The server saves a new world at once, then every five minutes, at each
  dawn and when it shuts down. Players who leave are kept in memory and
  written with the next save.
- Players are recognized by the id their client uses to connect, which the
  game keeps in a local profile and generates once. The player hosting a
  world is always `host`.

## Consequences

- A save survives changes to the content, and old saves stay readable as the
  format evolves.
- Changes to world generation would move the unedited terrain of existing
  worlds; such a change needs its own save version.
- Nothing proves a player owns the id they connect with. Issuing
  connections through the rendezvous service (M8) will.
- Saving runs on the main thread. It takes milliseconds for a valley with a
  few dozen players; larger worlds may need to write in the background.
