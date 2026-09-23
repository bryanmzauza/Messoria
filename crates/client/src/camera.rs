//! The player's view into the world.
//!
//! A fixed overview camera until players exist; the first- and third-person
//! rig replaces it once movement is networked.

use bevy::{pbr::DistanceFog, prelude::*};

use crate::environment::SKY_COLOR;

/// Distance at which terrain is fully swallowed by fog, in meters.
const FOG_VISIBILITY: f32 = 180.0;

pub(crate) struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
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
        Transform::from_xyz(-12.0, 9.0, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
