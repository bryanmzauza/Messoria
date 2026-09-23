//! The authoritative game server.
//!
//! [`ServerPlugin`] holds game logic only. It never adds runtime plugins such
//! as a window, a schedule runner or logging, so the same plugin runs in the
//! dedicated server binary and inside the game client when a player hosts
//! their own world. It expects `SharedPlugin` with the `Server` or `Host` role.

mod connections;
mod players;

use std::net::SocketAddr;

use bevy::prelude::*;

pub struct ServerPlugin {
    /// Address the server listens on for clients.
    pub bind_addr: SocketAddr,
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            connections::ConnectionsPlugin {
                bind_addr: self.bind_addr,
            },
            players::PlayersPlugin,
        ));
    }
}
