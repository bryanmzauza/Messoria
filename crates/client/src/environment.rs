//! Sky, sunlight and atmosphere.
//!
//! Values are static for now; the day/night cycle will drive them from the
//! world clock.

use bevy::{light::CascadeShadowConfigBuilder, prelude::*};
use messoria_shared::movement::GROUND_HEIGHT;

/// Horizon color, shared by the clear color and the distance fog so distant
/// geometry fades into the sky instead of into a band of a different hue.
pub(crate) const SKY_COLOR: Color = Color::srgb(0.62, 0.76, 0.88);

const SUN_COLOR: Color = Color::srgb(1.0, 0.95, 0.85);
const SUN_ILLUMINANCE_LUX: f32 = 12_000.0;

/// Stand-in for the voxel terrain, which does not exist yet.
const GROUND_SIZE: f32 = 1024.0;
const GROUND_COLOR: Color = Color::srgb(0.36, 0.52, 0.28);

/// Stones scattered over the flat stand-in ground so that motion reads clearly.
const STONE_COUNT: u16 = 120;
const STONE_SPACING: f32 = 6.0;
const STONE_COLOR: Color = Color::srgb(0.55, 0.53, 0.5);

pub(crate) struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY_COLOR))
            .insert_resource(GlobalAmbientLight {
                color: SKY_COLOR,
                brightness: 400.0,
                ..default()
            })
            .add_systems(Startup, (spawn_sun, spawn_ground, spawn_stones));
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
        Transform::from_xyz(0.0, GROUND_HEIGHT, 0.0),
    ));
}

/// Lays stones out on a golden-angle spiral: evenly spread, with no visible
/// rows, and the same on every run.
fn spawn_stones(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let mesh = meshes.add(Cuboid::new(0.6, 0.4, 0.5));
    let material = materials.add(StandardMaterial {
        base_color: STONE_COLOR,
        perceptual_roughness: 1.0,
        ..default()
    });

    for i in 1..=STONE_COUNT {
        let i = f32::from(i);
        let radius = i.sqrt() * STONE_SPACING;
        let angle = i * golden_angle;
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(
                radius * angle.cos(),
                GROUND_HEIGHT + 0.2,
                radius * angle.sin(),
            )
            .with_rotation(Quat::from_rotation_y(angle)),
        ));
    }
}
