//! The authoritative game server.
//!
//! [`ServerPlugin`] holds game logic only. It never adds runtime plugins such
//! as a window, a schedule runner or logging, so the same plugin runs in the
//! dedicated server binary and inside the game client when a player hosts
//! their own world. It expects `SharedPlugin` with the `Server` or `Host` role.

mod connections;
mod day_cycle;
mod inventory;
mod players;
mod terrain;

use std::net::SocketAddr;

use bevy::prelude::*;
use messoria_calendar::{SleepRule, WorldTime};

pub struct ServerPlugin {
    /// Address the server listens on for clients.
    pub bind_addr: SocketAddr,
    /// How many players must sleep to end the day.
    pub sleep_rule: SleepRule,
    /// When the world's clock starts.
    pub start_time: WorldTime,
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            connections::ConnectionsPlugin {
                bind_addr: self.bind_addr,
            },
            players::PlayersPlugin,
            inventory::InventoryPlugin,
            terrain::TerrainPlugin,
            day_cycle::DayCyclePlugin {
                sleep_rule: self.sleep_rule,
                start_time: self.start_time,
            },
        ));
    }
}
