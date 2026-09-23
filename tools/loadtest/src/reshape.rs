//! Bot behavior: every so often, dig or raise the ground just ahead.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{ItemKind, Tool};
use messoria_inventory::HOTBAR_SLOTS;
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{ActionChannel, Belongings, Heading, ItemAction, PlayerInput, Position, UseItem},
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
    content: Res<Content>,
    mut state: ResMut<Reshape>,
    player: Query<(&Position, &Heading, &Belongings), With<InputMarker<PlayerInput>>>,
    mut sender: Query<&mut MessageSender<UseItem>, With<Client>>,
) {
    if !state.timer.tick(time.delta()).just_finished() {
        return;
    }
    let (Ok((feet, heading, belongings)), Ok(mut sender)) = (player.single(), sender.single_mut())
    else {
        return;
    };
    let is_shovel = |slot: usize| {
        belongings.0.slot(slot).is_some_and(|stack| {
            matches!(content.item(stack.item).kind, ItemKind::Tool(Tool::Shovel))
        })
    };
    let Some(slot) = (0..HOTBAR_SLOTS).find(|&slot| is_shovel(slot)) else {
        return;
    };
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let direction = (Quat::from_rotation_y(heading.0) * AIM).normalize();
    let Some(hit) = terrain.raycast(eyes, direction, shovel::REACH) else {
        return;
    };
    let action = if state.rng.random_bool(0.5) {
        ItemAction::Primary
    } else {
        ItemAction::Secondary
    };
    sender.send::<ActionChannel>(UseItem {
        slot: u8::try_from(slot).expect("hotbar slots fit in u8"),
        action,
        target: Some(hit.point),
    });
}
