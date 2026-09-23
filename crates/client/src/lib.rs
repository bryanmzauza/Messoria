//! The game client: everything the player sees, hears and touches.
//!
//! [`ClientPlugin`] assumes the host app already provides Bevy's default
//! plugins and `SharedPlugin` with the `Client` or `Host` role. Game rules
//! never live here; the client only presents state that the server owns and
//! predicts the local player's movement through shared code.

mod actions;
mod art;
mod avatars;
mod camera;
mod clock;
mod connection;
mod cover;
mod environment;
mod fields;
mod hud;
mod input;
mod inventory;
mod noise;
mod panels;
mod scenery;
mod shops;
mod sky;
mod sleep;
mod terrain;
mod ui;
mod wallet;
mod weather;

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
            clock::ClockPlugin,
            environment::EnvironmentPlugin,
            terrain::TerrainPlugin,
            fields::FieldsPlugin,
            weather::WeatherPlugin,
            avatars::AvatarPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            inventory::InventoryPlugin,
            actions::ActionsPlugin,
            sleep::SleepPlugin,
            hud::HudPlugin,
        ))
        .add_plugins((
            art::ArtPlugin,
            scenery::SceneryPlugin,
            cover::CoverPlugin,
            sky::SkyPlugin,
            ui::UiPlugin,
            panels::PanelsPlugin,
            shops::ShopsPlugin,
            wallet::WalletPlugin,
        ));
    }
}
