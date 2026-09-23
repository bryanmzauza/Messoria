//! Sky, sunlight and atmosphere.
//!
//! Values are static for now; the day/night cycle will drive them from the
//! world clock.

use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

/// Horizon color, shared by the clear color and the distance fog so distant
/// geometry fades into the sky instead of into a band of a different hue.
pub(crate) const SKY_COLOR: Color = Color::srgb(0.62, 0.76, 0.88);

const SUN_COLOR: Color = Color::srgb(1.0, 0.95, 0.85);
const SUN_ILLUMINANCE_LUX: f32 = 12_000.0;

/// Stand-in for the voxel terrain, which does not exist yet.
const GROUND_SIZE: f32 = 1024.0;
const GROUND_COLOR: Color = Color::srgb(0.36, 0.52, 0.28);

pub(crate) struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY_COLOR))
            .insert_resource(GlobalAmbientLight {
                color: SKY_COLOR,
                brightness: 400.0,
                ..default()
            })
            .add_systems(Startup, (spawn_sun, spawn_ground));
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            color: SUN_COLOR,
            illuminance: SUN_ILLUMINANCE_LUX,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            maximum_distance: 120.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(40.0, 80.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: GROUND_COLOR,
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
}
