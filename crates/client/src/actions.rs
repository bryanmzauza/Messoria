//! Using the held item, harvesting and gathering by hand.
//!
//! The left mouse button uses the held item and the right button uses it the
//! other way, where it has one. Items that act on the ground aim at the
//! terrain under the crosshair: the shovel shows the ground it will move, and
//! farming items the field square they work. The axe and the pickaxe aim at
//! the tree or rock under the crosshair. Tools repeat while the button is
//! held; seeds, fertilizer and food act once per click. `F` picks what is
//! under the crosshair, such as berries, or harvests the ripe crop there,
//! unless it opens a shop. The server carries out every use, so the world
//! changes when its update arrives. Aim markers turn red over the village,
//! whose ground cannot be worked.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{ItemKind, Tool};
use messoria_shared::{
    content::Content,
    fields::{tile_at, tile_center},
    movement::EYE_HEIGHT,
    obstacles::Obstacles,
    protocol::{
        ActionChannel, Belongings, Gather, HarvestRequest, ItemAction, PlayerInput, Position,
        UseItem,
    },
    structures,
    terrain::Terrain,
    tools, village,
};
use messoria_voxel::RayHit;

use crate::{
    camera::{LookSystems, View},
    furniture::FurnitureSystems,
    inventory::HeldSlot,
    shops::ShopSystems,
};

/// Harvests, picks by hand, and opens shops at their stalls.
pub(crate) const INTERACT_KEY: KeyCode = KeyCode::KeyF;
/// How the interact key is named on screen.
pub(crate) const INTERACT_KEY_NAME: &str = "F";
const AIM_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.7);
const PROTECTED_AIM_COLOR: Color = Color::srgba(1.0, 0.3, 0.25, 0.8);
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
                aim.in_set(AimSystems),
                // Before the cursor is captured, so the capturing click is not a use.
                (
                    use_held_item,
                    harvest.after(ShopSystems).after(FurnitureSystems),
                )
                    .before(LookSystems),
                draw_aim,
            )
                .chain(),
        );
    }
}

/// What is under the crosshair within the character's reach: the ground, or
/// something standing on it, such as a tree or a stall, whichever is nearer.
#[derive(Resource, Default)]
pub(crate) struct Aim {
    pub ground: Option<RayHit>,
    /// The entity standing there, and the point on it aimed at.
    pub thing: Option<(Entity, Vec3)>,
}

/// Finds what the crosshair is on.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AimSystems;

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
    /// Strikes the aimed tree or rock, repeatedly.
    Gatherer,
    /// Builds a structure on the aimed ground, once per click.
    Builder,
}

fn handling(kind: &ItemKind) -> Option<Handling> {
    match kind {
        ItemKind::Tool(Tool::Shovel) => Some(Handling::Shovel),
        ItemKind::Tool(Tool::Hoe | Tool::WateringCan) => Some(Handling::FieldTool),
        ItemKind::Tool(Tool::Axe | Tool::Pickaxe) => Some(Handling::Gatherer),
        ItemKind::Seed | ItemKind::Fertilizer => Some(Handling::FieldSupply),
        ItemKind::Structure => Some(Handling::Builder),
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
    obstacles: Res<Obstacles>,
    camera: Single<&Transform, With<Camera3d>>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
    mut aim: ResMut<Aim>,
) {
    *aim = Aim::default();
    let Ok(feet) = player.single() else {
        return;
    };
    if !view.captured {
        return;
    }
    // Aim along the view, which in third person starts behind the character,
    // but measure reach from the character's eyes as the server does.
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let (origin, direction) = (camera.translation, *camera.forward());
    let max_distance = origin.distance(eyes) + tools::REACH;
    let ground = terrain
        .raycast(origin, direction, max_distance)
        .filter(|hit| tools::in_reach(eyes, hit.point));
    let thing = obstacles
        .raycast(origin, direction, max_distance)
        .map(|(entity, distance)| (entity, distance, origin + direction * distance))
        .filter(|&(_, _, point)| tools::in_reach(eyes, point));
    match thing {
        Some((entity, distance, point)) if ground.is_none_or(|hit| distance < hit.distance) => {
            aim.thing = Some((entity, point));
        }
        _ => aim.ground = ground,
    }
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
        Handling::Shovel | Handling::FieldTool | Handling::FieldSupply | Handling::Builder => {
            match aim.ground {
                Some(hit) => Some(hit.point),
                None => return,
            }
        }
        Handling::Gatherer => match aim.thing {
            Some((_, point)) => Some(point),
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
    if matches!(
        handling,
        Handling::FieldSupply | Handling::Consumable | Handling::Builder
    ) {
        *pressed = None;
    }
}

/// Picks what stands under the crosshair, or harvests the crop there.
fn harvest(
    keys: Res<ButtonInput<KeyCode>>,
    view: Res<View>,
    aim: Res<Aim>,
    mut harvests: Query<&mut MessageSender<HarvestRequest>, With<Client>>,
    mut gathers: Query<&mut MessageSender<Gather>, With<Client>>,
) {
    if !view.captured || !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    if let (Some((_, point)), Ok(mut sender)) = (aim.thing, gathers.single_mut()) {
        sender.send::<ActionChannel>(Gather { target: point });
    } else if let (Some(hit), Ok(mut sender)) = (aim.ground, harvests.single_mut()) {
        sender.send::<ActionChannel>(HarvestRequest { target: hit.point });
    }
}

fn draw_aim(
    aim: Res<Aim>,
    view: Res<View>,
    content: Res<Content>,
    held: Res<HeldSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut gizmos: Gizmos,
) {
    let (Some(hit), Ok(belongings)) = (aim.ground, player.single()) else {
        return;
    };
    // Gizmo shapes are drawn facing +Z; this lays them on the ground.
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let color = |reach| {
        if village::reaches(hit.point, reach) {
            PROTECTED_AIM_COLOR
        } else {
            AIM_COLOR
        }
    };
    match held_handling(&content, &held, belongings) {
        Some(Handling::Shovel) => {
            gizmos.circle(
                Isometry3d::new(hit.point + Vec3::Y * AIM_LIFT, flat),
                tools::BRUSH_RADIUS,
                color(tools::BRUSH_RADIUS),
            );
        }
        Some(Handling::FieldTool | Handling::FieldSupply) => {
            let center = tile_center(tile_at(hit.point));
            let square =
                Isometry3d::new(Vec3::new(center.x, hit.point.y + AIM_LIFT, center.y), flat);
            gizmos.rect(square, Vec2::ONE, color(0.0));
        }
        Some(Handling::Builder) => {
            let built = belongings
                .0
                .slot(held.0)
                .and_then(|stack| content.structure_built_from(stack.item));
            if let Some(kind) = built {
                let definition = content.structure(kind);
                let plan = structures::planned(kind, definition, hit.point, view.yaw);
                let (width, depth) = definition.size;
                let ground = Isometry3d::new(
                    plan.position.with_y(hit.point.y + AIM_LIFT),
                    Quat::from_rotation_y(plan.facing) * flat,
                );
                let reach = Vec2::new(width, depth).length() / 2.0;
                gizmos.rect(ground, Vec2::new(width, depth), {
                    if village::reaches(plan.position, reach) {
                        PROTECTED_AIM_COLOR
                    } else {
                        AIM_COLOR
                    }
                });
            }
        }
        Some(Handling::Consumable | Handling::Gatherer) | None => {}
    }
}
