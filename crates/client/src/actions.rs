//! Using the held item: the left mouse button for its main use, the right
//! button for its second one.
//!
//! A shovel aims at the terrain, shows where it will act and repeats while
//! the button is held; food is eaten once per click. The server carries out
//! every use, so the world changes when its update arrives.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{ItemKind, Tool};
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{ActionChannel, Belongings, ItemAction, PlayerInput, Position, UseItem},
    shovel,
    terrain::Terrain,
};
use messoria_voxel::RayHit;

use crate::{
    camera::{LookSystems, View},
    inventory::HeldSlot,
};

const AIM_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.7);
/// Lifts the aim marker off the surface so it is not hidden inside it.
const AIM_LIFT: f32 = 0.03;
const BUTTONS: [(MouseButton, ItemAction); 2] = [
    (MouseButton::Left, ItemAction::Primary),
    (MouseButton::Right, ItemAction::Secondary),
];

pub(crate) struct ActionsPlugin;

impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>().add_systems(
            Update,
            (
                aim,
                // Before the cursor is captured, so the capturing click is not a use.
                use_held_item.before(LookSystems),
                draw_aim,
            )
                .chain(),
        );
    }
}

/// Where a shovel would act, if the player holds one and aims at terrain in
/// reach.
#[derive(Resource, Default)]
struct Aim(Option<RayHit>);

/// What the local player holds, if anything.
fn held_item<'a>(
    content: &'a Content,
    held: &HeldSlot,
    belongings: &Belongings,
) -> Option<&'a ItemKind> {
    let stack = belongings.0.slot(held.0)?;
    Some(&content.item(stack.item).kind)
}

fn aim(
    view: Res<View>,
    content: Res<Content>,
    held: Res<HeldSlot>,
    terrain: Res<Terrain>,
    camera: Single<&Transform, With<Camera3d>>,
    player: Query<(&Position, &Belongings), With<InputMarker<PlayerInput>>>,
    mut aim: ResMut<Aim>,
) {
    aim.0 = None;
    let Ok((feet, belongings)) = player.single() else {
        return;
    };
    let holding_shovel = matches!(
        held_item(&content, &held, belongings),
        Some(ItemKind::Tool(Tool::Shovel))
    );
    if !view.captured || !holding_shovel {
        return;
    }
    // Aim along the view, which in third person starts behind the character,
    // but measure reach from the character's eyes as the server does.
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let max_distance = camera.translation.distance(eyes) + shovel::REACH;
    aim.0 = terrain
        .raycast(camera.translation, *camera.forward(), max_distance)
        .filter(|hit| shovel::in_reach(eyes, hit.point));
}

fn use_held_item(
    mouse: Res<ButtonInput<MouseButton>>,
    view: Res<View>,
    content: Res<Content>,
    held: Res<HeldSlot>,
    aim: Res<Aim>,
    time: Res<Time>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut pressed: Local<Option<ItemAction>>,
    mut ready_at: Local<Duration>,
    mut sender: Query<&mut MessageSender<UseItem>, With<Client>>,
) {
    for (button, action) in BUTTONS {
        if mouse.just_pressed(button) {
            // A click that captures the cursor is not a use.
            *pressed = view.captured.then_some(action);
        }
        if mouse.just_released(button) && *pressed == Some(action) {
            *pressed = None;
        }
    }

    let (Some(action), Ok(belongings), Ok(mut sender)) =
        (*pressed, player.single(), sender.single_mut())
    else {
        return;
    };
    let (target, repeats) = match held_item(&content, &held, belongings) {
        Some(ItemKind::Tool(Tool::Shovel)) => match aim.0 {
            Some(hit) => (Some(hit.point), true),
            None => return,
        },
        Some(ItemKind::Food { .. }) if action == ItemAction::Primary => (None, false),
        _ => return,
    };
    if time.elapsed() < *ready_at {
        return;
    }

    sender.send::<ActionChannel>(UseItem {
        slot: u8::try_from(held.0).expect("hotbar slots fit in u8"),
        action,
        target,
    });
    *ready_at = time.elapsed() + shovel::COOLDOWN;
    if !repeats {
        *pressed = None;
    }
}

fn draw_aim(aim: Res<Aim>, mut gizmos: Gizmos) {
    if let Some(hit) = aim.0 {
        let facing = Quat::from_rotation_arc(Vec3::Z, hit.normal);
        gizmos.circle(
            Isometry3d::new(hit.point + hit.normal * AIM_LIFT, facing),
            shovel::BRUSH_RADIUS,
            AIM_COLOR,
        );
    }
}
