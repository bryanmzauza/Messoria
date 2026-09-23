//! Going to sleep and getting up.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_shared::protocol::{ActionChannel, Asleep, PlayerInput, SleepRequest};

const SLEEP_KEY: KeyCode = KeyCode::KeyZ;
/// How the sleep key is named on screen.
pub(crate) const SLEEP_KEY_NAME: &str = "Z";

pub(crate) struct SleepPlugin;

impl Plugin for SleepPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle_sleep);
    }
}

/// Asks the server to put the character to sleep or wake it. The server
/// decides, and says so when it is too early to sleep.
fn toggle_sleep(
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<Has<Asleep>, With<InputMarker<PlayerInput>>>,
    mut sender: Query<&mut MessageSender<SleepRequest>, With<Client>>,
) {
    if !keys.just_pressed(SLEEP_KEY) {
        return;
    }
    let (Ok(asleep), Ok(mut sender)) = (player.single(), sender.single_mut()) else {
        return;
    };
    sender.send::<ActionChannel>(if asleep {
        SleepRequest::Wake
    } else {
        SleepRequest::Sleep
    });
}
