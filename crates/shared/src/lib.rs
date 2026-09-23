//! Code that must behave identically on the server and on clients.
//!
//! Anything here runs on both sides of the connection, so it has to be
//! deterministic and must not touch rendering, input devices or files. This is
//! also the only crate that configures lightyear; other crates use the types it
//! re-exports through its modules.

pub mod content;
pub mod energy;
pub mod fields;
pub mod movement;
pub mod network;
pub mod protocol;
pub mod shops;
pub mod terrain;
pub mod tick;
pub mod tools;
pub mod village;

use bevy::{prelude::*, state::app::StatesPlugin};
use lightyear::prelude::{
    client::ClientPlugins, input::native::InputMarker, server::ServerPlugins, *,
};
use messoria_content::Catalog;

use crate::{
    network::NetworkRole,
    protocol::{PlayerId, PlayerInput},
};

/// Networking and simulation common to every app that takes part in a game.
///
/// Add exactly one per app, before `ServerPlugin` or `ClientPlugin`.
pub struct SharedPlugin {
    pub role: NetworkRole,
    /// Loaded with [`content::load_content`] before the app is built, so
    /// that invalid content stops the program before anything starts.
    pub content: Catalog,
}

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        // lightyear drives connection state through Bevy states, which headless
        // apps built on `MinimalPlugins` do not provide.
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }

        let tick_duration = tick::tick_duration();
        match self.role {
            NetworkRole::Server => app.add_plugins(ServerPlugins { tick_duration }),
            // Prediction only records history and rolls back to the server's
            // state when this resource is present. Without it, a remote
            // client's character would drift from the server's for good as
            // soon as one input went unapplied.
            NetworkRole::Client => app
                .add_plugins(ClientPlugins { tick_duration })
                .insert_resource(PredictionManager::default()),
            NetworkRole::Host => app.add_plugins((
                ServerPlugins { tick_duration },
                ClientPlugins { tick_duration },
            )),
        };

        // lightyear requires the protocol to be registered after its plugin groups.
        app.insert_resource(content::Content(self.content.clone()));
        app.add_plugins((
            protocol::ProtocolPlugin,
            terrain::TerrainPlugin,
            movement::MovementPlugin,
        ))
        .add_observer(read_input_for_controlled_player);
    }
}

/// Marks the player this app controls as the one local input is written to.
///
/// Replication does not guarantee which of the two components arrives first,
/// so this reacts to either and requires both.
fn read_input_for_controlled_player(
    trigger: On<Add, (Controlled, PlayerId)>,
    players: Query<
        (),
        (
            With<Controlled>,
            With<PlayerId>,
            Without<InputMarker<PlayerInput>>,
        ),
    >,
    mut commands: Commands,
) {
    if players.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<PlayerInput>::default());
    }
}
