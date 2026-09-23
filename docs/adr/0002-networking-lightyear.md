# 0002 — lightyear for networking

**Status:** accepted

## Context

The server is authoritative ([ADR 0001](0001-server-authoritative-single-codebase.md))
and movement must feel immediate, so the client needs prediction,
reconciliation and interpolation of remote entities. The two maintained Bevy
networking libraries that support Bevy 0.19 are lightyear and bevy_replicon.

- **bevy_replicon** offers a small, well-designed replication core. Prediction
  and interpolation are left to the application or to third-party crates.
- **lightyear** ships replication, tick synchronization, input buffering,
  prediction with rollback, interpolation and several transports (UDP with
  netcode, WebTransport, Steam) as one integrated stack.

## Decision

Use lightyear. Its prediction and interpolation cover the needs of the first
networked milestone without custom infrastructure.

Protocol registration (replicated components, messages, channels, inputs)
lives in one module of `messoria-shared`. Other crates depend on game types,
not on lightyear types, wherever practical.

## Consequences

- Less networking code to write and maintain in the early milestones.
- lightyear's API changes between releases; confining it to the protocol module
  and a few systems limits the cost of each upgrade.
- lightyear upgrades are tied to Bevy upgrades; the two are bumped together.
- If lightyear stops fitting, the confinement keeps a migration to another
  library tractable.
