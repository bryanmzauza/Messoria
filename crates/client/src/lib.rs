//! The game client: everything the player sees, hears and touches.
//!
//! [`ClientPlugin`] assumes the host app already provides Bevy's default
//! plugins (window, renderer, input). Game rules never live here; the client
//! only presents state that the server owns.

mod camera;
mod environment;

use bevy::prelude::*;
use messoria_shared::SharedPlugin;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<SharedPlugin>() {
            app.add_plugins(SharedPlugin);
        }

        app.add_plugins((environment::EnvironmentPlugin, camera::CameraPlugin));
    }
}
