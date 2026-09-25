//! Bot behavior: travel the valley, running toward a far destination and
//! picking another on arrival, or when stuck, so bots end up spread over
//! the whole world and keep the server loading terrain as they go.

use bevy::prelude::*;
use lightyear::prelude::{
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
};
use messoria_shared::protocol::{PlayerInput, Position};
use messoria_worldgen::HALF_WIDTH;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

/// Share of the world's width destinations are picked within, clear of the
/// mountains at its edge.
const REACH: f32 = 0.75;
/// How close to its destination a bot must come to pick another.
const ARRIVED: f32 = 8.0;
/// Ticks over which a bot must make headway, and how much, before it counts
/// as stuck.
const PATIENCE_TICKS: u32 = 90;
const HEADWAY: f32 = 2.0;

pub(crate) struct RoamPlugin {
    pub seed: u64,
}

impl Plugin for RoamPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Roam {
            rng: SmallRng::seed_from_u64(self.seed),
            destination: None,
            checkpoint: None,
            ticks: 0,
        })
        .add_systems(FixedPreUpdate, roam.in_set(InputSystems::WriteClientInputs));
    }
}

#[derive(Resource)]
struct Roam {
    rng: SmallRng,
    destination: Option<Vec2>,
    /// Where the bot was when it last checked its headway.
    checkpoint: Option<Vec2>,
    ticks: u32,
}

impl Roam {
    fn pick(&mut self) -> Vec2 {
        let reach = HALF_WIDTH * REACH;
        let destination = Vec2::new(
            self.rng.random_range(-reach..reach),
            self.rng.random_range(-reach..reach),
        );
        self.destination = Some(destination);
        destination
    }
}

fn roam(
    mut state: ResMut<Roam>,
    mut player: Query<(&Position, &mut ActionState<PlayerInput>), With<InputMarker<PlayerInput>>>,
) {
    let Ok((feet, mut action)) = player.single_mut() else {
        return;
    };
    let here = feet.0.xz();
    let mut destination = match state.destination {
        Some(destination) if destination.distance(here) > ARRIVED => destination,
        _ => state.pick(),
    };

    state.ticks += 1;
    let mut jump = false;
    if state.ticks >= PATIENCE_TICKS {
        state.ticks = 0;
        if state
            .checkpoint
            .is_some_and(|checkpoint| checkpoint.distance(here) < HEADWAY)
        {
            destination = state.pick();
            jump = true;
        }
        state.checkpoint = Some(here);
    }

    // Moving forward heads along -z turned by the yaw.
    let toward = destination - here;
    action.0 = PlayerInput {
        movement: Vec2::Y,
        yaw: (-toward.x).atan2(-toward.y),
        jump,
        sprint: true,
        held: 0,
    };
}
