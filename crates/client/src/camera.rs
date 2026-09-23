//! The player's view: mouse look, and first- or third-person framing of the
//! local character.
//!
//! Clicking the window captures the cursor for mouse look; Escape releases it.
//! F5 switches perspective.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    pbr::DistanceFog,
    prelude::*,
    transform::TransformSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use lightyear::prelude::input::native::InputMarker;
use messoria_shared::protocol::PlayerInput;

use crate::{
    avatars::{AvatarSystems, EYE_HEIGHT},
    environment::SKY_COLOR,
};

/// Distance at which terrain is fully swallowed by fog, in meters.
const FOG_VISIBILITY: f32 = 180.0;
/// Radians of rotation per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.0025;
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
            .add_systems(Update, (capture_cursor, look, toggle_perspective).chain())
            .add_systems(
                PostUpdate,
                (follow_player, show_own_avatar)
                    .after(AvatarSystems)
                    .before(TransformSystems::Propagate),
            );
    }
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Perspective {
    #[default]
    FirstPerson,
    ThirdPerson,
}

impl View {
    fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        DistanceFog {
            color: SKY_COLOR,
            falloff: FogFalloff::from_visibility_squared(FOG_VISIBILITY),
            ..default()
        },
        // Overview shown until the local character arrives.
        Transform::from_xyz(-12.0, 9.0, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn capture_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut view: ResMut<View>,
) {
    let (window, mut cursor) = window.into_inner();
    let capture = if view.captured && (!window.focused || keys.just_pressed(KeyCode::Escape)) {
        false
    } else if !view.captured && window.focused && mouse.just_pressed(MouseButton::Left) {
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

fn look(motion: Res<AccumulatedMouseMotion>, mut view: ResMut<View>) {
    if !view.captured {
        return;
    }
    view.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
    view.pitch = (view.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
}

fn toggle_perspective(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<View>) {
    if keys.just_pressed(PERSPECTIVE_TOGGLE) {
        view.perspective = match view.perspective {
            Perspective::FirstPerson => Perspective::ThirdPerson,
            Perspective::ThirdPerson => Perspective::FirstPerson,
        };
    }
}

fn follow_player(
    view: Res<View>,
    player: Query<&Transform, (With<InputMarker<PlayerInput>>, Without<Camera3d>)>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let eye = player.translation + Vec3::Y * EYE_HEIGHT;
    let rotation = view.rotation();
    camera.rotation = rotation;
    camera.translation = match view.perspective {
        Perspective::FirstPerson => eye,
        Perspective::ThirdPerson => eye + rotation * Vec3::Z * THIRD_PERSON_DISTANCE,
    };
}

/// Hides the local character's body in first person, where it would block the view.
fn show_own_avatar(
    view: Res<View>,
    mut player: Query<&mut Visibility, With<InputMarker<PlayerInput>>>,
) {
    if let Ok(mut visibility) = player.single_mut() {
        visibility.set_if_neq(match view.perspective {
            Perspective::FirstPerson => Visibility::Hidden,
            Perspective::ThirdPerson => Visibility::Inherited,
        });
    }
}
