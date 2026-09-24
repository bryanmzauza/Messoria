//! The sky: a dome shaded from the horizon to the zenith, the sun or the
//! moon, and low-poly clouds drifting over the valley.
//!
//! The dome and the discs follow the camera, so they always look infinitely
//! far away. Fog does not touch them; it fades the land into the horizon's
//! color instead, which is the color at the dome's rim.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    asset::RenderAssetUsages,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::{
    camera::{CameraPlacement, WorldCamera},
    environment::Sky,
    ui::visible_if,
};

/// Radius of the dome, within the camera's far plane and beyond everything
/// else.
const DOME_RADIUS: f32 = 600.0;
const DOME_SEGMENTS: u16 = 32;
const DOME_RINGS: u16 = 16;
/// How far below the horizon the dome reaches, in radians, so no gap shows
/// at the edge of the land.
const DOME_BELOW_HORIZON: f32 = 0.35;
/// Elevation, as a sine, at which the sky has fully turned to the zenith's
/// color.
const ZENITH_FROM: f32 = 0.7;

const DISC_DISTANCE: f32 = 550.0;
const SUN_RADIUS: f32 = 24.0;
const MOON_RADIUS: f32 = 16.0;
const SUN_COLOR: Color = Color::srgb(1.0, 0.94, 0.72);
const MOON_COLOR: Color = Color::srgb(0.88, 0.9, 1.0);

const CLOUDS: usize = 18;
/// Half the side of the square clouds drift across, centered on the valley.
const CLOUD_FIELD: f32 = 220.0;
const CLOUD_HEIGHT: (f32, f32) = (75.0, 100.0);
/// Meters per second the wind carries clouds.
const WIND: Vec2 = Vec2::new(1.6, 0.5);
const CLOUD_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const RAIN_CLOUD_COLOR: Color = Color::srgb(0.62, 0.64, 0.68);
/// How far from the horizon's color toward white clouds are in full
/// daylight.
const CLOUD_BRIGHTNESS: f32 = 0.7;

pub(crate) struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_dome, spawn_discs, spawn_clouds))
            .add_systems(
                PostUpdate,
                (shade_dome, place_sky, drift_clouds)
                    .after(CameraPlacement)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// The dome, with how far toward the zenith's color each vertex is.
#[derive(Component)]
struct Dome {
    mesh: Handle<Mesh>,
    blends: Vec<f32>,
}

#[derive(Component)]
struct Disc {
    sun: bool,
}

#[derive(Component)]
struct Cloud;

#[derive(Resource)]
struct CloudLook(Handle<StandardMaterial>);

/// Something the sky draws that neither fog nor shadows should touch.
fn sky_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    }
}

fn spawn_dome(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let (mesh, blends) = dome_mesh();
    let mesh = meshes.add(mesh);
    commands.spawn((
        Name::new("Sky dome"),
        Dome {
            mesh: mesh.clone(),
            blends,
        },
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(sky_material(Color::WHITE))),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// A dome of rings from a little below the horizon up to the zenith, and how
/// far toward the zenith's color each of its vertices is.
fn dome_mesh() -> (Mesh, Vec<f32>) {
    let mut positions = Vec::new();
    let mut blends = Vec::new();
    for ring in 0..=DOME_RINGS {
        let elevation = -DOME_BELOW_HORIZON
            + (FRAC_PI_2 + DOME_BELOW_HORIZON) * f32::from(ring) / f32::from(DOME_RINGS);
        for segment in 0..=DOME_SEGMENTS {
            let bearing = TAU * f32::from(segment) / f32::from(DOME_SEGMENTS);
            let direction = Vec3::new(
                elevation.cos() * bearing.cos(),
                elevation.sin(),
                elevation.cos() * bearing.sin(),
            );
            positions.push((direction * DOME_RADIUS).to_array());
            let t = (elevation.sin() / ZENITH_FROM).clamp(0.0, 1.0);
            blends.push(t * t * (3.0 - 2.0 * t));
        }
    }
    let row = u32::from(DOME_SEGMENTS) + 1;
    let mut indices = Vec::new();
    for ring in 0..u32::from(DOME_RINGS) {
        for segment in 0..u32::from(DOME_SEGMENTS) {
            let (a, b) = (ring * row + segment, (ring + 1) * row + segment);
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let normals = vec![[0.0, -1.0, 0.0]; positions.len()];
    let colors = vec![[1.0; 4]; positions.len()];
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    (mesh, blends)
}

fn spawn_discs(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (sun, radius, color) in [
        (true, SUN_RADIUS, SUN_COLOR),
        (false, MOON_RADIUS, MOON_COLOR),
    ] {
        commands.spawn((
            Name::new(if sun { "Sun" } else { "Moon" }),
            Disc { sun },
            Mesh3d(meshes.add(Circle::new(radius).mesh().resolution(24))),
            MeshMaterial3d(materials.add(sky_material(color))),
            Transform::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Clouds are clusters of flattened low-poly blobs.
fn spawn_clouds(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let blob = meshes.add(Sphere::new(1.0).mesh().ico(1).expect("a small subdivision"));
    // Unlit, and tinted by the sky instead: lit from below, they would be
    // gray and heavy even at noon.
    let look = materials.add(sky_material(CLOUD_COLOR));
    commands.insert_resource(CloudLook(look.clone()));

    let mut rng = SmallRng::seed_from_u64(0x636c_6f75_6473);
    for index in 0..CLOUDS {
        let position = Vec3::new(
            rng.random_range(-CLOUD_FIELD..CLOUD_FIELD),
            rng.random_range(CLOUD_HEIGHT.0..CLOUD_HEIGHT.1),
            rng.random_range(-CLOUD_FIELD..CLOUD_FIELD),
        );
        let size = rng.random_range(10.0..20.0);
        commands
            .spawn((
                Name::new(format!("Cloud {index}")),
                Cloud,
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(rng.random_range(0.0..TAU))),
                Visibility::default(),
            ))
            .with_children(|cloud| {
                for _ in 0..rng.random_range(4..7) {
                    let offset = Vec3::new(
                        rng.random_range(-1.4..1.4),
                        rng.random_range(-0.1..0.3),
                        rng.random_range(-0.7..0.7),
                    ) * size;
                    let scale = Vec3::new(1.0, 0.45, 0.8) * size * rng.random_range(0.55..0.9);
                    cloud.spawn((
                        Mesh3d(blob.clone()),
                        MeshMaterial3d(look.clone()),
                        Transform::from_translation(offset).with_scale(scale),
                        NotShadowCaster,
                    ));
                }
            });
    }
}

fn shade_dome(
    sky: Res<Sky>,
    dome: Single<&Dome>,
    mut meshes: ResMut<Assets<Mesh>>,
    clouds: Option<Res<CloudLook>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !sky.is_changed() {
        return;
    }
    let (horizon, zenith) = (sky.horizon.to_linear(), sky.zenith.to_linear());
    let colors: Vec<[f32; 4]> = dome
        .blends
        .iter()
        .map(|&blend| horizon.mix(&zenith, blend).to_f32_array())
        .collect();
    if let Some(mut mesh) = meshes.get_mut(&dome.mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
    // White by day and tinted by the horizon at dawn and dusk; by night they
    // sink into the sky's own darkness.
    let white = if sky.overcast {
        RAIN_CLOUD_COLOR
    } else {
        CLOUD_COLOR
    };
    let cloud_color = sky.horizon.mix(&white, CLOUD_BRIGHTNESS * sky.daylight);
    if let Some(mut look) = clouds.and_then(|clouds| materials.get_mut(&clouds.0))
        && look.base_color != cloud_color
    {
        look.base_color = cloud_color;
    }
}

/// Keeps the dome around the camera and the sun or moon in the direction of
/// the light, facing the camera.
fn place_sky(
    sky: Res<Sky>,
    camera: Single<&Transform, (With<WorldCamera>, Without<Dome>, Without<Disc>)>,
    mut dome: Single<&mut Transform, (With<Dome>, Without<Disc>)>,
    mut discs: Query<(&Disc, &mut Transform, &mut Visibility), Without<Dome>>,
) {
    dome.translation = camera.translation;
    for (disc, mut transform, mut visibility) in &mut discs {
        visibility.set_if_neq(visible_if(disc.sun == sky.sun_up && !sky.overcast));
        let position = camera.translation + sky.toward_light * DISC_DISTANCE;
        *transform = Transform::from_translation(position).looking_at(camera.translation, Vec3::Y);
    }
}

fn drift_clouds(time: Res<Time>, mut clouds: Query<&mut Transform, With<Cloud>>) {
    let drift = WIND * time.delta_secs();
    for mut transform in &mut clouds {
        let moved = transform.translation.xz() + drift;
        // Clouds leaving the field come back in on the other side.
        let wrapped =
            (moved + CLOUD_FIELD).rem_euclid(Vec2::splat(2.0 * CLOUD_FIELD)) - CLOUD_FIELD;
        transform.translation.x = wrapped.x;
        transform.translation.z = wrapped.y;
    }
}
