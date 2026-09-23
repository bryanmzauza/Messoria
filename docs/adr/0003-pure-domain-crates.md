# 0003 — Domain logic in crates without Bevy

**Status:** accepted

## Context

The rules that define Messoria (terrain meshing, crop growth, spoilage, prices
and saturation, inventory) are intricate and must behave identically across
releases. Testing them through a running Bevy `App` is slow and couples the
rules to the engine version.

## Decision

Domain logic lives in dedicated crates (`messoria-voxel`, `messoria-content`,
`messoria-economy`, and others as needed) that do not depend on Bevy. They use
`glam` at the version Bevy re-exports, so math types cross the boundary without
conversion. The server and client crates wrap these crates in thin Bevy systems.

## Consequences

- Rules are unit-tested and benchmarked with plain `cargo test` and `cargo bench`.
- Domain crates compile quickly and are unaffected by most Bevy upgrades.
- Adapter code is needed between ECS components and domain types.
- The `glam` version must be kept in step with Bevy's.
