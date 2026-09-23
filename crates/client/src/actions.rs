//! Using the held item and harvesting.
//!
//! The left mouse button uses the held item and the right button uses it the
//! other way, where it has one. Items that act on the world aim at the
//! terrain under the crosshair: the shovel shows the ground it will move, and
//! farming items the field square they work. Tools repeat while the button is
//! held; seeds, fertilizer and food act once per click. `E` harvests the ripe
//! crop under the crosshair. The server carries out every use, so the world
//! changes when its update arrives.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{ItemKind, Tool};
use messoria_shared::{
    content::Content,
    fields::{tile_at, tile_center},
    movement::EYE_HEIGHT,
    protocol::{
        ActionChannel, Belongings, HarvestRequest, ItemAction, PlayerInput, Position, UseItem,
    },
    terrain::Terrain,
    tools,
};
use messoria_voxel::RayHit;

use crate::{
    camera::{LookSystems, View},
    inventory::HeldSlot,
};

const HARVEST_KEY: KeyCode = KeyCode::KeyE;
const AIM_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.7);
/// Lifts aim markers off the surface so they are not hidden inside it.
const AIM_LIFT: f32 = 0.05;
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
                (use_held_item, harvest).before(LookSystems),
                draw_aim,
            )
                .chain(),
        );
    }
}

/// The terrain under the crosshair, if it is within the character's reach.
#[derive(Resource, Default)]
struct Aim(Option<RayHit>);

/// How the held item acts on the world, which decides how aiming looks and
/// whether holding the button repeats the use.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Handling {
    /// Digs or raises ground around the aimed point, repeatedly.
    Shovel,
    /// Works the aimed field square, repeatedly.
    FieldTool,
    /// Goes into the aimed field square, once per click.
    FieldSupply,
    /// Used on oneself, once per click.
    Consumable,
}

fn handling(kind: &ItemKind) -> Option<Handling> {
    match kind {
        ItemKind::Tool(Tool::Shovel) => Some(Handling::Shovel),
        ItemKind::Tool(Tool::Hoe | Tool::WateringCan) => Some(Handling::FieldTool),
        ItemKind::Seed | ItemKind::Fertilizer => Some(Handling::FieldSupply),
        ItemKind::Food { .. } => Some(Handling::Consumable),
        ItemKind::Terrain { .. } | ItemKind::Goods => None,
    }
}

fn held_handling(content: &Content, held: &HeldSlot, belongings: &Belongings) -> Option<Handling> {
    let stack = belongings.0.slot(held.0)?;
    handling(&content.item(stack.item).kind)
}

fn aim(
    view: Res<View>,
    terrain: Res<Terrain>,
    camera: Single<&Transform, With<Camera3d>>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
    mut aim: ResMut<Aim>,
) {
    aim.0 = None;
    let Ok(feet) = player.single() else {
        return;
    };
    if !view.captured {
        return;
    }
    // Aim along the view, which in third person starts behind the character,
    // but measure reach from the character's eyes as the server does.
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let max_distance = camera.translation.distance(eyes) + tools::REACH;
    aim.0 = terrain
        .raycast(camera.translation, *camera.forward(), max_distance)
        .filter(|hit| tools::in_reach(eyes, hit.point));
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
    let Some(handling) = held_handling(&content, &held, belongings) else {
        return;
    };
    let target = match handling {
        Handling::Consumable if action == ItemAction::Primary => None,
        Handling::Consumable => return,
        Handling::Shovel | Handling::FieldTool | Handling::FieldSupply => match aim.0 {
            Some(hit) => Some(hit.point),
            None => return,
        },
    };
    if time.elapsed() < *ready_at {
        return;
    }

    sender.send::<ActionChannel>(UseItem {
        slot: u8::try_from(held.0).expect("hotbar slots fit in u8"),
        action,
        target,
    });
    *ready_at = time.elapsed() + tools::USE_INTERVAL;
    if matches!(handling, Handling::FieldSupply | Handling::Consumable) {
        *pressed = None;
    }
}

fn harvest(
    keys: Res<ButtonInput<KeyCode>>,
    view: Res<View>,
    aim: Res<Aim>,
    mut sender: Query<&mut MessageSender<HarvestRequest>, With<Client>>,
) {
    if !view.captured || !keys.just_pressed(HARVEST_KEY) {
        return;
    }
    if let (Some(hit), Ok(mut sender)) = (aim.0, sender.single_mut()) {
        sender.send::<ActionChannel>(HarvestRequest { target: hit.point });
    }
}

fn draw_aim(
    aim: Res<Aim>,
    content: Res<Content>,
    held: Res<HeldSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut gizmos: Gizmos,
) {
    let (Some(hit), Ok(belongings)) = (aim.0, player.single()) else {
        return;
    };
    // Gizmo shapes are drawn facing +Z; this lays them on the ground.
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    match held_handling(&content, &held, belongings) {
        Some(Handling::Shovel) => {
            gizmos.circle(
                Isometry3d::new(hit.point + Vec3::Y * AIM_LIFT, flat),
                tools::BRUSH_RADIUS,
                AIM_COLOR,
            );
        }
        Some(Handling::FieldTool | Handling::FieldSupply) => {
            let center = tile_center(tile_at(hit.point));
            let square =
                Isometry3d::new(Vec3::new(center.x, hit.point.y + AIM_LIFT, center.y), flat);
            gizmos.rect(square, Vec2::ONE, AIM_COLOR);
        }
        Some(Handling::Consumable) | None => {}
    }
}
