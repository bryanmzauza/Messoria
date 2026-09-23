//! The game client: everything the player sees, hears and touches.
//!
//! [`ClientPlugin`] assumes the host app already provides Bevy's default
//! plugins and `SharedPlugin` with the `Client` or `Host` role. Game rules
//! never live here; the client only presents state that the server owns and
//! predicts the local player's movement through shared code.

mod avatars;
mod camera;
mod connection;
mod environment;
mod hud;
mod input;
mod shovel;
mod terrain;

use bevy::prelude::*;

pub use crate::connection::Session;

pub struct ClientPlugin {
    pub session: Session,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            connection::ConnectionPlugin {
                session: self.session.clone(),
            },
            environment::EnvironmentPlugin,
            terrain::TerrainPlugin,
            avatars::AvatarPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            shovel::ShovelPlugin,
            hud::HudPlugin,
        ));
    }
}
