//! The player's view: mouse look, and framing of the local character from
//! its eyes, from behind or from the front.
//!
//! Clicking the window captures the cursor for mouse look; Escape releases it.
//! F5 cycles through the perspectives. The camera is also where the player hears from.

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
use messoria_shared::{movement::EYE_HEIGHT, protocol::PlayerInput};

use crate::{avatars::AvatarSystems, panels::OpenPanel, settings::Preferences};

/// Distance at which terrain is fully swallowed by fog, in meters.
const FOG_VISIBILITY: f32 = 180.0;
/// Radians of rotation per pixel of mouse movement, at a mouse sensitivity
/// of one.
const RADIANS_PER_PIXEL: f32 = 0.0025;
/// Distance between the listener's ears, in meters.
const EAR_GAP: f32 = 0.3;
/// Keeps the view from flipping over when looking straight up or down.
const MAX_PITCH: f32 = FRAC_PI_2 - 0.01;
/// Distance from the character's head to the camera in third person.
const THIRD_PERSON_DISTANCE: f32 = 4.5;
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
    view: Res<View>,
    player: Query<&Transform, (With<InputMarker<PlayerInput>>, Without<WorldCamera>)>,
    mut camera: Single<&mut Transform, With<WorldCamera>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let eye = player.translation + Vec3::Y * EYE_HEIGHT;
    let rotation = view.rotation();
    let (facing, place) = match view.perspective {
        Perspective::FirstPerson => (rotation, eye),
        Perspective::Behind => (rotation, eye + rotation * Vec3::Z * THIRD_PERSON_DISTANCE),
        Perspective::Front => {
            // Turned around, ahead of the character and looking back at it;
            // the mouse still turns the character.
            let facing = Quat::from_euler(EulerRot::YXZ, view.yaw + PI, -view.pitch, 0.0);
            (facing, eye + facing * Vec3::Z * THIRD_PERSON_DISTANCE)
        }
    };
    camera.rotation = facing;
    camera.translation = place;
}

/// Hides the local character's body in first person, where it would block the view.
fn show_own_avatar(
    view: Res<View>,
    mut player: Query<&mut Visibility, With<InputMarker<PlayerInput>>>,
) {
    if let Ok(mut visibility) = player.single_mut() {
        visibility.set_if_neq(match view.perspective {
            Perspective::FirstPerson => Visibility::Hidden,
            Perspective::Behind | Perspective::Front => Visibility::Inherited,
        });
    }
}
