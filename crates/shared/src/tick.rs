//! The fixed simulation rate.
//!
//! Gameplay systems run in `FixedUpdate` at this rate on the server and on
//! clients alike, which is what makes client-side prediction line up with the
//! authoritative simulation.

use std::time::Duration;

/// Simulation ticks per second.
pub const TICK_RATE_HZ: u32 = 30;

/// Wall-clock length of a single simulation tick.
pub fn tick_duration() -> Duration {
    Duration::from_secs(1) / TICK_RATE_HZ
}

/// Whole ticks in `duration`. Game logic that runs on a schedule counts ticks
/// rather than summing tick durations, whose rounding would drift.
///
/// # Panics
///
/// If `duration` spans more than `u32::MAX` ticks, which is over four years.
pub fn ticks_in(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis() * u128::from(TICK_RATE_HZ) / 1000)
        .expect("durations used by game logic span few ticks")
}
