# 0022 — Music and ambience

**Status:** accepted.

## Context

The valley was silent but for footsteps and the sounds of work. The MVP
wants music that follows the seasons and an ambience that follows the
day, the weather and the place: wind, birds, crickets, rain, the river
near its banks, and all of it muffled indoors. Music and sound from
others would have to be found, checked and credited one file at a time,
and little of what is free fits a cozy medieval valley.

## Decision

- The music and the ambient loops are made for Messoria, by scripts that
  synthesize them: four pieces, one a season, in keys and tempos chosen
  for each (spring bright in D, summer warm in G, autumn wistful in A
  dorian, winter sparse and quiet in E minor), of plucked strings, a soft
  flute, a pad and bells, with the reverb's tail folded into the start so
  each loops without a seam; and loops of wind, birds, crickets, rain and
  the river, each in a plain and a muffled recording. They are Ogg
  Vorbis, a few megabytes in all, and fall under the repository's
  license like the models and the skins.
- The client's `music` plays the piece of the season drawn, looping, and
  crossfades to the next as the season turns; the title screen plays
  spring. `ambience` keeps every layer looping at a volume it eases
  toward what the moment calls for: the hour and the season for birds
  and crickets, the weather for rain and the wind, the distance to the
  river for the river, and whether the camera stands inside a building
  with a door, or the player's own cabin, for the muffled recordings.
- Music has a volume of its own among the options, within the main
  volume, since players keep their own music on as often as not.

## Consequences

- Nothing in the audio needs a license check or a credit beyond the
  packs already listed; the scripts can make more pieces in the same
  voice when the valley grows.
- The ambience plays ten loops at once, most of them silent; decoding
  them costs little next to the picture, and switching by volume keeps
  every change smooth.
- Muffling is a second recording rather than a filter, since the audio
  engine offers none; a new layer means two files.
