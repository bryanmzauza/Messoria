//! Bot behavior: walk in a random direction for a while, stop, pick another.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lightyear::prelude::{
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
};
use messoria_shared::protocol::PlayerInput;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

/// Range of ticks a bot keeps doing the same thing.
const LEG_TICKS: std::ops::Range<u32> = 30..150;
const IDLE_CHANCE: f64 = 0.2;
const JUMP_CHANCE: f64 = 0.02;

pub(crate) struct WanderPlugin {
    pub seed: u64,
}

impl Plugin for WanderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Wander {
            rng: SmallRng::seed_from_u64(self.seed),
            current: PlayerInput::default(),
            ticks_left: 0,
        })
        .add_systems(
            FixedPreUpdate,
            wander.in_set(InputSystems::WriteClientInputs),
        );
    }
}

#[derive(Resource)]
struct Wander {
    rng: SmallRng,
    current: PlayerInput,
    ticks_left: u32,
}

fn wander(
    mut state: ResMut<Wander>,
    mut player: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    let Ok(mut action) = player.single_mut() else {
        return;
    };
    let state = &mut *state;

    if state.ticks_left == 0 {
        state.ticks_left = state.rng.random_range(LEG_TICKS);
        state.current = PlayerInput {
            movement: if state.rng.random_bool(IDLE_CHANCE) {
                Vec2::ZERO
            } else {
                Vec2::Y
            },
            yaw: state.rng.random_range(0.0..TAU),
            jump: false,
            sprint: false,
            held: 0,
        };
    }
    state.ticks_left -= 1;

    action.0 = PlayerInput {
        jump: state.rng.random_bool(JUMP_CHANCE),
        ..state.current
    };
}
