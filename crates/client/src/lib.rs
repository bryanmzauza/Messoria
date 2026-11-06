//! The game client: everything the player sees, hears and touches.
//!
//! [`ClientPlugin`] assumes the host app already provides Bevy's default
//! plugins and `SharedPlugin` with the `Client` or `Host` role. Game rules
//! never live here; the client only presents state that the server owns and
//! predicts the local player's movement through shared code.

mod actions;
mod art;
mod avatars;
mod barber;
mod board;
mod camera;
mod clock;
mod connection;
mod cover;
mod creator;
mod environment;
mod feedback;
mod fences;
mod fields;
mod figures;
mod furniture;
mod ground;
mod horizon;
mod hud;
mod icons;
mod input;
mod inventory;
mod menu;
mod mirror;
mod noise;
mod panels;
mod particles;
mod picture;
mod scenery;
mod settings;
mod shops;
mod skins;
mod sky;
mod sounds;
mod structures;
mod talk;
mod target;
mod terrain;
mod ui;
mod viewmodel;
mod wallet;
mod wardrobe;
mod water;
mod weather;
mod wind;

use std::path::PathBuf;

use bevy::prelude::*;
use messoria_save::Profile;

pub use crate::connection::Session;

pub struct ClientPlugin {
    pub session: Session,
    /// Where the player's settings are kept.
    pub settings_file: PathBuf,
    /// Who the player is, and where that is kept, if anywhere.
    pub profile: Profile,
    pub profile_file: Option<PathBuf>,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        // The world and the player in it.
        app.add_plugins((
            connection::ConnectionPlugin {
                session: self.session.clone(),
            },
            clock::ClockPlugin,
            environment::EnvironmentPlugin,
            picture::PicturePlugin,
            ground::GroundPlugin,
            terrain::TerrainPlugin,
            horizon::HorizonPlugin,
            water::WaterPlugin,
            fields::FieldsPlugin,
            weather::WeatherPlugin,
            avatars::AvatarPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            actions::ActionsPlugin,
            furniture::FurniturePlugin,
        ))
        // How it looks and sounds.
        .add_plugins((
            art::ArtPlugin,
            wind::WindPlugin,
            scenery::SceneryPlugin,
            fences::FencesPlugin,
            structures::StructuresPlugin,
            cover::CoverPlugin,
            sky::SkyPlugin,
            feedback::FeedbackPlugin,
            sounds::SoundsPlugin,
            particles::ParticlesPlugin,
            figures::FiguresPlugin,
            skins::SkinsPlugin,
            icons::IconsPlugin,
            mirror::MirrorPlugin,
            viewmodel::ViewmodelPlugin,
        ))
        // What is on screen over it.
        .add_plugins((
            ui::UiPlugin,
            hud::HudPlugin,
            target::TargetPlugin,
            panels::PanelsPlugin,
            inventory::InventoryPlugin,
            shops::ShopsPlugin,
            board::BoardPlugin,
            talk::TalkPlugin,
            wardrobe::WardrobePlugin,
            barber::BarberPlugin,
            creator::CreatorPlugin {
                profile: self.profile.clone(),
                file: self.profile_file.clone(),
            },
            wallet::WalletPlugin,
            settings::SettingsPlugin {
                file: self.settings_file.clone(),
            },
            menu::MenuPlugin,
        ));
    }
}
