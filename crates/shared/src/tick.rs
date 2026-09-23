//! The fixed simulation rate.
//!
//! Gameplay systems run in `FixedUpdate` at this rate on the server and on
//! clients alike, which is what makes client-side prediction line up with the
//! authoritative simulation.

use std::time::Duration;

/// Simulation ticks per second.
pub const TICK_RATE_HZ: f64 = 30.0;

/// Wall-clock length of a single simulation tick.
pub fn tick_duration() -> Duration {
    Duration::from_secs_f64(1.0 / TICK_RATE_HZ)
}
