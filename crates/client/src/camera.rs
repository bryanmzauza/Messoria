//! The player's view: mouse look, and framing of the local character from
//! its eyes, from behind or from the front.
//!
//! Clicking the window captures the cursor for mouse look; Escape releases it.
//! F5 cycles through the perspectives. Seen from outside, the camera stays
//! this side of walls and ground between it and the character, so a room
//! is seen from within. The camera is also where the player hears from.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{
    audio::SpatialListener,
    input::mouse::AccumulatedMouseMotion,
    pbr::DistanceFog,
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use lightyear::prelude::input::native::InputMarker;
use messoria_shared::{
    movement::EYE_HEIGHT, obstacles::Obstacles, protocol::PlayerInput, terrain::Terrain,
};

use crate::{avatars::AvatarSystems, panels::OpenPanel, settings::Preferences};

/// Distance at which the land is all but swallowed by haze, in meters: far
/// past the full terrain, into the coarse land drawn out to the horizon.
const FOG_VISIBILITY: f32 = 1400.0;
/// Farthest the world camera sees: across the valley, to the dome of the
/// sky around it.
const FAR_PLANE: f32 = 3000.0;
/// Radians of rotation per pixel of mouse movement, at a mouse sensitivity
/// of one.
const RADIANS_PER_PIXEL: f32 = 0.0025;
/// Distance between the listener's ears, in meters.
const EAR_GAP: f32 = 0.3;
/// Keeps the view from flipping over when looking straight up or down.
const MAX_PITCH: f32 = FRAC_PI_2 - 0.01;
/// Distance from the character's head to the camera in third person.
const THIRD_PERSON_DISTANCE: f32 = 4.5;
/// How far the camera stays off a wall or the ground it would otherwise
/// go through, and how near the head it may come.
const WALL_CLEARANCE: f32 = 0.3;
const NEAREST: f32 = 0.6;
/// Nearer than this, the character would fill the view, so it is hidden as
/// in first person.
const TOO_NEAR_TO_SHOW: f32 = 1.6;
const PERSPECTIVE_TOGGLE: KeyCode = KeyCode::F5;

pub(crate) struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<View>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (capture_cursor, look, toggle_perspective)
                    .chain()
                    .in_set(LookSystems),
            )
            .add_systems(
                PostUpdate,
                (follow_player, show_own_avatar)
                    .chain()
                    .in_set(CameraPlacement)
                    .after(AvatarSystems)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Updates the view from the mouse and keyboard.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LookSystems;

/// Places the camera for the frame; whatever follows the camera runs after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CameraPlacement;

/// The camera the world is seen through, among others that draw icons or
/// what the player holds.
#[derive(Component)]
pub(crate) struct WorldCamera;

/// Where the player is looking, and from where.
#[derive(Resource, Debug, Default)]
pub(crate) struct View {
    /// Rotation around the vertical axis, in radians. Zero looks toward -Z.
    pub yaw: f32,
    /// Rotation above (positive) or below the horizon, in radians.
    pub pitch: f32,
    pub perspective: Perspective,
    /// Whether the cursor is captured, meaning mouse and keyboard drive the character.
    pub captured: bool,
    /// How far behind or ahead of the head the camera is, in third person,
    /// once walls and ground have pushed it in.
    pub distance: f32,
}

/// Where the view is from: the character's eyes, or behind or ahead of the
/// character, looking at it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Perspective {
    #[default]
    FirstPerson,
    Behind,
    Front,
}

impl Perspective {
    /// The next one, as the key cycles through them.
    fn next(self) -> Self {
        match self {
            Self::FirstPerson => Self::Behind,
            Self::Behind => Self::Front,
            Self::Front => Self::FirstPerson,
        }
    }
}

impl View {
    fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Camera"),
        WorldCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: FAR_PLANE,
            ..default()
        }),
        SpatialListener::new(EAR_GAP),
        // The environment tints the fog to match the sky.
        DistanceFog {
            falloff: FogFalloff::from_visibility_squared(FOG_VISIBILITY),
            ..default()
        },
        // Overview shown until the local character arrives.
        Transform::from_xyz(0.0, 30.0, 40.0).looking_at(Vec3::new(0.0, 8.0, 0.0), Vec3::Y),
    ));
}

/// Captures the cursor on a click in the world and releases it on Escape,
/// when the window loses focus or while a window such as the backpack is
/// open.
fn capture_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<OpenPanel>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut view: ResMut<View>,
) {
    let (window, mut cursor) = window.into_inner();
    let release = !window.focused || keys.just_pressed(KeyCode::Escape) || panel.is_open();
    let capture = if view.captured && release {
        false
    } else if !view.captured && !release && mouse.just_pressed(MouseButton::Left) {
        true
    } else {
        return;
    };

    view.captured = capture;
    cursor.visible = !capture;
    cursor.grab_mode = if capture {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    preferences: Res<Preferences>,
    mut view: ResMut<View>,
) {
    if !view.captured {
        return;
    }
    let turn = motion.delta * RADIANS_PER_PIXEL * preferences.0.mouse_sensitivity;
    view.yaw -= turn.x;
    view.pitch = (view.pitch - turn.y).clamp(-MAX_PITCH, MAX_PITCH);
}

fn toggle_perspective(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<View>) {
    if keys.just_pressed(PERSPECTIVE_TOGGLE) {
        view.perspective = view.perspective.next();
    }
}

fn follow_player(
    mut view: ResMut<View>,
    obstacles: Res<Obstacles>,
    terrain: Res<Terrain>,
    player: Query<&Transform, (With<InputMarker<PlayerInput>>, Without<WorldCamera>)>,
    mut camera: Single<&mut Transform, With<WorldCamera>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let eye = player.translation + Vec3::Y * EYE_HEIGHT;
    let rotation = view.rotation();
    let (facing, back) = match view.perspective {
        Perspective::FirstPerson => (rotation, None),
        Perspective::Behind => (rotation, Some(rotation * Vec3::Z)),
        Perspective::Front => {
            // Turned around, ahead of the character and looking back at it;
            // the mouse still turns the character.
            let facing = Quat::from_euler(EulerRot::YXZ, view.yaw + PI, -view.pitch, 0.0);
            (facing, Some(facing * Vec3::Z))
        }
    };
    camera.rotation = facing;
    camera.translation = match back {
        None => {
            view.distance = 0.0;
            eye
        }
        Some(direction) => {
            view.distance = reach(&obstacles, &terrain, eye, direction);
            eye + direction * view.distance
        }
    };
}

/// How far from the eyes along `direction` the camera can be without going
/// through a wall or the ground.
fn reach(obstacles: &Obstacles, terrain: &Terrain, eye: Vec3, direction: Vec3) -> f32 {
    let wall = obstacles
        .raycast(eye, direction, THIRD_PERSON_DISTANCE)
        .map(|(_, distance)| distance);
    let ground = terrain
        .raycast(eye, direction, THIRD_PERSON_DISTANCE)
        .map(|hit| hit.distance);
    let nearest = [wall, ground]
        .into_iter()
        .flatten()
        .fold(f32::INFINITY, f32::min);
    (nearest - WALL_CLEARANCE).clamp(NEAREST, THIRD_PERSON_DISTANCE)
}

/// Hides the local character's body in first person, where it would block
/// the view, and when the camera is pushed up against it.
fn show_own_avatar(
    view: Res<View>,
    mut player: Query<&mut Visibility, With<InputMarker<PlayerInput>>>,
) {
    if let Ok(mut visibility) = player.single_mut() {
        let shown = match view.perspective {
            Perspective::FirstPerson => false,
            Perspective::Behind | Perspective::Front => view.distance >= TOO_NEAR_TO_SHOW,
        };
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
