# 0001 — Server-authoritative simulation, one codebase for both hosting modes

**Status:** accepted

## Context

Messoria is played in two ways: a small group of friends joining a world hosted
by one of them, and communities of 50 or more players on a dedicated server.
The economy, crops and terrain are shared state that players have strong
incentives to manipulate, and players compare notes about prices and harvests.
Divergent behavior between the two modes would be both a bug source and a
support burden.

## Decision

All gameplay logic runs on an authoritative server. Clients send inputs and
render replicated state. The server is a set of Bevy plugins with no knowledge
of how it is hosted: the dedicated server binary runs it headless, and the game
client runs it in-process when a player opens their world.

## Consequences

- One implementation of every rule; both modes behave identically by construction.
- Cheating is limited to what the client can influence through inputs.
- The server crate must stay headless-safe: no rendering, windowing or audio
  dependencies.
- Latency is visible to players, so movement needs client-side prediction from
  the first networked milestone.
- A hosted world costs the host some CPU for the server simulation.
