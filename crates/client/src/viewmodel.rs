//! What the player holds, seen in first person at the bottom of the view.
//!
//! The held item is drawn by a camera of its own over the world, so it never
//! sinks into walls or the ground ahead. It sways as the character walks and
//! swings down each time the player uses it.

use std::f32::consts::PI;

use bevy::{camera::visibility::RenderLayers, prelude::*};
use lightyear::prelude::input::native::InputMarker;
use messoria_content::ItemId;
use messoria_shared::{
    content::Content,
    protocol::{Belongings, PlayerInput, Velocity},
};

use crate::{
    actions::Used,
    art::{DrawnOnLayer, Models},
    camera::{CameraPlacement, Perspective, View, WorldCamera},
    environment::Sky,
    inventory::HeldSlot,
};

/// The render layer held items are drawn on.
const HELD_LAYER: usize = 2;
/// Where the held item rests in the view, turned and at what size.
const REST: Vec3 = Vec3::new(0.36, -0.44, -0.62);
const REST_TURN: Vec3 = Vec3::new(-0.7, 2.7, 0.4);
const HELD_SCALE: f32 = 1.1;
/// How long a swing lasts, in seconds, and how far down it goes, in radians.
const SWING_TIME: f32 = 0.28;
const SWING_ANGLE: f32 = 1.1;
/// How far and how fast the item sways while walking.
const SWAY: Vec2 = Vec2::new(0.012, 0.018);
const SWAY_PACE: f32 = 1.6;
/// Brightness of the light on the held item, by night and by day.
const NIGHT_LIGHT: f32 = 600.0;
const DAY_LIGHT: f32 = 7_000.0;

pub(crate) struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hand_camera).add_systems(
            PostUpdate,
            (follow_world_camera, show_held, move_held)
                .chain()
                .after(CameraPlacement),
        );
    }
}

#[derive(Component)]
struct HandCamera;

#[derive(Component)]
struct HandLight;

/// The held item in view, and the item it shows.
#[derive(Component)]
struct HeldInView {
    item: Option<ItemId>,
    model: Option<Entity>,
    /// Seconds left in the current swing.
    swing: f32,
    /// How far along its sway the item is.
    sway: f32,
}

fn spawn_hand_camera(mut commands: Commands) {
    let layer = RenderLayers::layer(HELD_LAYER);
    let camera = commands
        .spawn((
            Name::new("Hand camera"),
            HandCamera,
            Camera3d::default(),
            Camera {
                // Over the world camera, keeping its picture.
                order: 1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            layer.clone(),
        ))
        .id();
    commands.spawn((
        Name::new("Hand light"),
        HandLight,
        DirectionalLight::default(),
        Transform::default().looking_to(Vec3::new(-0.3, -1.0, -0.5), Vec3::Y),
        layer.clone(),
        ChildOf(camera),
    ));
    commands.spawn((
        Name::new("Held in view"),
        HeldInView {
            item: None,
            model: None,
            swing: 0.0,
            sway: 0.0,
        },
        Transform::from_translation(REST),
        Visibility::Hidden,
        DrawnOnLayer(HELD_LAYER),
        ChildOf(camera),
    ));
}

/// The hand camera sees from where the world camera does, as wide.
fn follow_world_camera(
    sky: Res<Sky>,
    world: Single<(&Transform, &Projection), (With<WorldCamera>, Without<HandCamera>)>,
    hand: Single<(&mut Transform, &mut Projection), With<HandCamera>>,
    mut light: Single<&mut DirectionalLight, With<HandLight>>,
) {
    let (world_transform, world_projection) = world.into_inner();
    let (mut transform, mut projection) = hand.into_inner();
    *transform = *world_transform;
    if let (Projection::Perspective(world), Projection::Perspective(hand)) =
        (world_projection, &mut *projection)
        && (hand.fov - world.fov).abs() > f32::EPSILON
    {
        hand.fov = world.fov;
    }
    light.illuminance = NIGHT_LIGHT + (DAY_LIGHT - NIGHT_LIGHT) * sky.daylight;
}

/// Shows the held item's model in first person, and nothing otherwise.
fn show_held(
    content: Res<Content>,
    models: Res<Models>,
    view: Res<View>,
    held: Res<HeldSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    shown: Single<(Entity, &mut HeldInView, &mut Visibility)>,
    mut commands: Commands,
) {
    let (entity, mut in_view, mut visibility) = shown.into_inner();
    let item = player
        .single()
        .ok()
        .and_then(|belongings| belongings.0.slot(held.0))
        .map(|stack| stack.item);
    visibility.set_if_neq(
        if view.perspective == Perspective::FirstPerson && item.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    );
    if in_view.item == item {
        return;
    }
    if let Some(old) = in_view.model.take() {
        commands.entity(old).despawn();
    }
    in_view.item = item;
    let scene = item
        .and_then(|item| content.item(item).model.as_deref())
        .and_then(|model| models.scene(model));
    in_view.model = scene.map(|scene| {
        commands
            .spawn((
                scene,
                Transform::from_scale(Vec3::splat(HELD_SCALE)),
                ChildOf(entity),
            ))
            .id()
    });
}

/// Sways the item with the character's steps and swings it on each use.
fn move_held(
    time: Res<Time>,
    mut used: MessageReader<Used>,
    player: Query<&Velocity, With<InputMarker<PlayerInput>>>,
    held: Single<(&mut HeldInView, &mut Transform)>,
) {
    let (mut in_view, mut transform) = held.into_inner();
    let dt = time.delta_secs();
    if used.read().count() > 0 {
        in_view.swing = SWING_TIME;
    }
    in_view.swing = (in_view.swing - dt).max(0.0);
    let speed = player
        .single()
        .map_or(0.0, |velocity| velocity.0.xz().length());
    in_view.sway += speed * SWAY_PACE * dt;

    // Down and back up over the swing, fastest at its start.
    let swing = (in_view.swing / SWING_TIME * PI).sin() * SWING_ANGLE;
    let moving = (speed / 4.0).min(1.0);
    let sway = Vec3::new(
        in_view.sway.sin() * SWAY.x,
        (in_view.sway * 2.0).sin().abs() * SWAY.y,
        0.0,
    ) * moving;
    transform.translation = REST + sway;
    transform.rotation =
        Quat::from_euler(EulerRot::XYZ, REST_TURN.x - swing, REST_TURN.y, REST_TURN.z);
}
