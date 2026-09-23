//! Bot behavior: every so often, dig or raise the ground just ahead.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_shared::{
    movement::EYE_HEIGHT,
    protocol::{ActionChannel, Heading, PlayerInput, Position, ShovelAction, ShovelRequest},
    shovel,
    terrain::Terrain,
};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

/// Time between two edits by the same bot.
const INTERVAL: Duration = Duration::from_secs(2);
/// Direction bots aim in, relative to where they face: ahead and down.
const AIM: Vec3 = Vec3::new(0.0, -1.0, -1.2);

pub(crate) struct ReshapePlugin {
    pub seed: u64,
}

impl Plugin for ReshapePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Reshape {
            rng: SmallRng::seed_from_u64(self.seed),
            timer: Timer::new(INTERVAL, TimerMode::Repeating),
        })
        .add_systems(Update, reshape);
    }
}

#[derive(Resource)]
struct Reshape {
    rng: SmallRng,
    timer: Timer,
}

fn reshape(
    time: Res<Time>,
    terrain: Res<Terrain>,
    mut state: ResMut<Reshape>,
    player: Query<(&Position, &Heading), With<InputMarker<PlayerInput>>>,
    mut sender: Query<&mut MessageSender<ShovelRequest>, With<Client>>,
) {
    if !state.timer.tick(time.delta()).just_finished() {
        return;
    }
    let (Ok((feet, heading)), Ok(mut sender)) = (player.single(), sender.single_mut()) else {
        return;
    };
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let direction = (Quat::from_rotation_y(heading.0) * AIM).normalize();
    let Some(hit) = terrain.raycast(eyes, direction, shovel::REACH) else {
        return;
    };
    let action = if state.rng.random_bool(0.5) {
        ShovelAction::Dig
    } else {
        ShovelAction::Raise
    };
    sender.send::<ActionChannel>(ShovelRequest {
        target: hit.point,
        action,
    });
}
