//! The player's own arm in first person, at the bottom right of the view,
//! holding what the player holds.
//!
//! The arm is the player's figure's right arm, dressed the same, and holds
//! items the way the figure does. It is drawn by a camera of its own over
//! the world, so it never sinks into walls or the ground ahead. It sways as
//! the character walks and swings each time the player uses what it holds.

use std::f32::consts::PI;

use bevy::{
    camera::{Hdr, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
};
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
    figures::{Figure, FigureMeshes, Part, in_hand},
    inventory::HeldSlot,
};

/// The render layer the arm is drawn on.
const ARM_LAYER: usize = 2;
/// Where the shoulder rests in the view, and how the arm is turned from
/// hanging down: raised forward, and in toward the middle of the view. The
/// arm is drawn smaller than the figure's, as it is seen from so close.
const REST: Vec3 = Vec3::new(0.24, -0.22, -0.3);
const REST_TURN: Vec3 = Vec3::new(1.35, 0.35, 0.1);
const ARM_SCALE: f32 = 0.55;
/// How long a swing lasts, in seconds, and how far it goes, in radians.
const SWING_TIME: f32 = 0.28;
const SWING_ANGLE: f32 = 0.9;
/// How far and how fast the arm sways while walking.
const SWAY: Vec2 = Vec2::new(0.012, 0.018);
const SWAY_PACE: f32 = 1.6;
/// Brightness of the light on the arm, by night and by day.
const NIGHT_LIGHT: f32 = 250.0;
const DAY_LIGHT: f32 = 2_600.0;

pub(crate) struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_arm_camera).add_systems(
            PostUpdate,
            (follow_world_camera, dress_arm, hold_in_view, move_arm)
                .chain()
                .after(CameraPlacement),
        );
    }
}

#[derive(Component)]
struct ArmCamera;

#[derive(Component)]
struct ArmLight;

/// How much the elbow bends at rest, in radians, and how far it bends
/// further through a swing.
const ELBOW: f32 = 0.55;
const ELBOW_SWING: f32 = 0.5;

/// The arm in view: its forearm once it is dressed, what it holds and how
/// it moves.
#[derive(Component)]
struct ArmInView {
    forearm: Option<Entity>,
    item: Option<ItemId>,
    model: Option<Entity>,
    /// Seconds left in the current swing.
    swing: f32,
    /// How far along its sway the arm is.
    sway: f32,
}

fn spawn_arm_camera(mut commands: Commands) {
    let layer = RenderLayers::layer(ARM_LAYER);
    let camera = commands
        .spawn((
            Name::new("Arm camera"),
            ArmCamera,
            Camera3d::default(),
            Camera {
                // Over the world camera, keeping its picture.
                order: 1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            // Drawn into the world camera's finished picture, so it must
            // match its format and not tonemap that picture a second time.
            Hdr,
            Msaa::Off,
            Tonemapping::None,
            layer.clone(),
        ))
        .id();
    commands.spawn((
        Name::new("Arm light"),
        ArmLight,
        DirectionalLight::default(),
        Transform::default().looking_to(Vec3::new(-0.3, -1.0, -0.5), Vec3::Y),
        layer.clone(),
        ChildOf(camera),
    ));
    commands.spawn((
        Name::new("Arm in view"),
        ArmInView {
            forearm: None,
            item: None,
            model: None,
            swing: 0.0,
            sway: 0.0,
        },
        Transform::from_translation(REST).with_scale(Vec3::splat(ARM_SCALE)),
        Visibility::Hidden,
        DrawnOnLayer(ARM_LAYER),
        ChildOf(camera),
    ));
}

/// The arm camera sees from where the world camera does, as wide.
fn follow_world_camera(
    sky: Res<Sky>,
    world: Single<(&Transform, &Projection), (With<WorldCamera>, Without<ArmCamera>)>,
    arm: Single<(&mut Transform, &mut Projection), With<ArmCamera>>,
    mut light: Single<&mut DirectionalLight, With<ArmLight>>,
) {
    let (world_transform, world_projection) = world.into_inner();
    let (mut transform, mut projection) = arm.into_inner();
    *transform = *world_transform;
    if let (Projection::Perspective(world), Projection::Perspective(arm)) =
        (world_projection, &mut *projection)
        && (arm.fov - world.fov).abs() > f32::EPSILON
    {
        arm.fov = world.fov;
    }
    light.illuminance = NIGHT_LIGHT + (DAY_LIGHT - NIGHT_LIGHT) * sky.daylight;
}

/// Gives the arm in view the player's own sleeve and hand, once the
/// player's figure is dressed.
fn dress_arm(
    content: Res<Content>,
    meshes: Res<FigureMeshes>,
    player: Query<&Figure, With<InputMarker<PlayerInput>>>,
    arm: Single<(Entity, &mut ArmInView)>,
    mut commands: Commands,
) {
    let (entity, mut arm) = arm.into_inner();
    let Ok(figure) = player.single() else {
        return;
    };
    if arm.forearm.is_some() {
        return;
    }
    for mesh in meshes.of(Part::RightArm) {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(figure.material.clone()),
            ChildOf(entity),
        ));
    }
    let characters = content.characters();
    let elbow = Vec3::from(characters.limbs.right_forearm.joint) * characters.pixel;
    let forearm = commands
        .spawn((
            Transform::from_translation(elbow),
            Visibility::default(),
            ChildOf(entity),
        ))
        .id();
    for mesh in meshes.of(Part::RightForearm) {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(figure.material.clone()),
            ChildOf(forearm),
        ));
    }
    arm.forearm = Some(forearm);
}

/// Shows the arm in first person, holding the held item's model.
fn hold_in_view(
    content: Res<Content>,
    models: Res<Models>,
    view: Res<View>,
    held: Res<HeldSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    arm: Single<(&mut ArmInView, &mut Visibility)>,
    mut commands: Commands,
) {
    let (mut arm, mut visibility) = arm.into_inner();
    visibility.set_if_neq(if view.perspective == Perspective::FirstPerson {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    let item = player
        .single()
        .ok()
        .and_then(|belongings| belongings.0.slot(held.0))
        .map(|stack| stack.item);
    if arm.item == item {
        return;
    }
    if let Some(old) = arm.model.take() {
        commands.entity(old).despawn();
    }
    arm.item = item;
    let Some(hand) = arm.forearm else {
        return;
    };
    arm.model = item.and_then(|item| {
        let scene = models.scene(content.item(item).model.as_deref()?)?;
        Some(
            commands
                .spawn((scene, in_hand(&content, item), ChildOf(hand)))
                .id(),
        )
    });
}

/// Sways the arm with the character's steps and swings it on each use.
fn move_arm(
    time: Res<Time>,
    mut used: MessageReader<Used>,
    player: Query<&Velocity, With<InputMarker<PlayerInput>>>,
    arm: Single<(&mut ArmInView, &mut Transform)>,
    mut forearms: Query<&mut Transform, Without<ArmInView>>,
) {
    let (mut arm, mut transform) = arm.into_inner();
    let dt = time.delta_secs();
    if used.read().count() > 0 {
        arm.swing = SWING_TIME;
    }
    arm.swing = (arm.swing - dt).max(0.0);
    let speed = player
        .single()
        .map_or(0.0, |velocity| velocity.0.xz().length());
    arm.sway += speed * SWAY_PACE * dt;

    // Down and back up over the swing.
    let swing = (arm.swing / SWING_TIME * PI).sin() * SWING_ANGLE;
    let moving = (speed / 4.0).min(1.0);
    let sway = Vec3::new(
        arm.sway.sin() * SWAY.x,
        (arm.sway * 2.0).sin().abs() * SWAY.y,
        0.0,
    ) * moving;
    transform.translation = REST + sway;
    transform.rotation =
        Quat::from_euler(EulerRot::XYZ, REST_TURN.x - swing, REST_TURN.y, REST_TURN.z);
    if let Some(mut forearm) = arm
        .forearm
        .and_then(|forearm| forearms.get_mut(forearm).ok())
    {
        forearm.rotation = Quat::from_rotation_x(ELBOW + ELBOW_SWING * swing);
    }
}
