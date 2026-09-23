//! Code that must behave identically on the server and on clients.
//!
//! Anything here runs on both sides of the connection, so it has to be
//! deterministic and must not touch rendering, input or I/O.

pub mod tick;

use bevy::prelude::*;

/// Configuration common to every app that simulates the game world.
///
/// Both `ServerPlugin` and `ClientPlugin` add this plugin when it is not
/// already present, so a process hosting both in the same `App` only
/// registers it once.
pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(tick::TICK_RATE_HZ));
    }
}
