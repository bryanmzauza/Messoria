//! Turns keyboard state into the `PlayerInput` sent to the server each tick.

use bevy::prelude::*;
use lightyear::prelude::{
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
};
use messoria_shared::protocol::PlayerInput;

use crate::camera::View;

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            write_input.in_set(InputSystems::WriteClientInputs),
        );
    }
}

/// Samples input once per simulation tick. The value is written every tick,
/// including "nothing pressed", so the server can tell an idle player apart
/// from missing input.
fn write_input(
    keys: Res<ButtonInput<KeyCode>>,
    view: Res<View>,
    mut player: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    let Ok(mut action) = player.single_mut() else {
        return;
    };

    action.0 = if view.captured {
        let axis = |positive: KeyCode, negative: KeyCode| {
            f32::from(u8::from(keys.pressed(positive)))
                - f32::from(u8::from(keys.pressed(negative)))
        };
        PlayerInput {
            movement: Vec2::new(
                axis(KeyCode::KeyD, KeyCode::KeyA),
                axis(KeyCode::KeyW, KeyCode::KeyS),
            ),
            yaw: view.yaw,
            jump: keys.pressed(KeyCode::Space),
            sprint: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        }
    } else {
        PlayerInput {
            yaw: view.yaw,
            ..default()
        }
    };
}
