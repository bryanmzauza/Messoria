use bevy::prelude::*;
use messoria_client::ClientPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Messoria".into(),
                    ..default()
                }),
                ..default()
            }),
            ClientPlugin,
        ))
        .run()
}
