//! Terrain meshes, rebuilt as chunks load and change.
//!
//! Meshing runs on the main thread under a per-frame time budget, nearest
//! chunks first, so loading a whole area never stalls a frame.
//!
//! Terrain is flat-shaded, low-poly style: every triangle is lit as one facet
//! and colored by one material, with a slight brightness difference from its
//! neighbors. Faces too steep to hold grass show the soil beneath it.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use messoria_calendar::Season;
use messoria_content::Palette;
use messoria_shared::{
    content::Content,
    terrain::{ChunkChanged, Terrain},
};
use messoria_voxel::{ChunkPos, Material, SurfaceMesh, mesh_chunk};

use crate::{
    art::{DrawnSeason, srgb},
    camera::WorldCamera,
    noise::unit_noise,
};

/// Time per frame spent rebuilding chunk meshes.
const MESHING_BUDGET: Duration = Duration::from_millis(4);
/// Largest brightness difference between facets, as a share of their color.
const FACET_VARIATION: f32 = 0.03;
/// Least upward component of a facet's normal for it to show grass.
const GRASS_FLATNESS: f32 = 0.6;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StaleChunks>()
            .init_resource::<ChunkMeshes>()
            .add_systems(Startup, create_terrain_material)
            .add_systems(Update, (collect_stale_chunks, rebuild_meshes).chain());
    }
}

/// Chunks whose meshes no longer match the terrain.
#[derive(Resource, Default)]
struct StaleChunks(HashSet<ChunkPos>);

/// The entity displaying each chunk that has a visible surface.
#[derive(Resource, Default)]
struct ChunkMeshes(HashMap<ChunkPos, Entity>);

/// One material for all terrain; ground types are told apart by vertex color.
#[derive(Resource)]
struct TerrainMaterial(Handle<StandardMaterial>);

fn create_terrain_material(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    commands.insert_resource(TerrainMaterial(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        ..default()
    })));
}

/// Chunks go stale when their voxels change, and all of them when the season
/// turns, since the ground's colors change with it.
fn collect_stale_chunks(
    mut changes: MessageReader<ChunkChanged>,
    season: Res<DrawnSeason>,
    chunk_meshes: Res<ChunkMeshes>,
    mut stale: ResMut<StaleChunks>,
) {
    stale.0.extend(changes.read().map(|changed| changed.0));
    if season.is_changed() {
        stale.0.extend(chunk_meshes.0.keys().copied());
    }
}

fn rebuild_meshes(
    terrain: Res<Terrain>,
    content: Res<Content>,
    season: Res<DrawnSeason>,
    material: Res<TerrainMaterial>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut stale: ResMut<StaleChunks>,
    mut chunk_meshes: ResMut<ChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    if stale.0.is_empty() {
        return;
    }
    let started = Instant::now();
    let colors = GroundColors::new(content.palette(), season.0);
    let viewer = ChunkPos::containing(camera.translation.floor().as_ivec3());
    let mut queue: Vec<ChunkPos> = stale.0.iter().copied().collect();
    queue.sort_unstable_by_key(|chunk| (chunk.0 - viewer.0).length_squared());

    for chunk in queue {
        if started.elapsed() > MESHING_BUDGET {
            break;
        }
        stale.0.remove(&chunk);

        let origin = chunk.origin().as_vec3();
        let surface = mesh_chunk(&terrain, chunk).filter(|surface| !surface.is_empty());
        match (surface, chunk_meshes.0.get(&chunk).copied()) {
            (Some(surface), Some(entity)) => {
                commands.entity(entity).insert(Mesh3d(
                    meshes.add(to_render_mesh(&surface, origin, &colors)),
                ));
            }
            (Some(surface), None) => {
                let entity = commands
                    .spawn((
                        Name::new(format!("Terrain chunk {}", chunk.0)),
                        Mesh3d(meshes.add(to_render_mesh(&surface, origin, &colors))),
                        MeshMaterial3d(material.0.clone()),
                        Transform::from_translation(origin),
                    ))
                    .id();
                chunk_meshes.0.insert(chunk, entity);
            }
            (None, Some(entity)) => {
                commands.entity(entity).despawn();
                chunk_meshes.0.remove(&chunk);
            }
            (None, None) => {}
        }
    }
}

/// A flat-shaded mesh of a chunk's surface, whose origin is at `origin` in
/// the world. Every triangle gets vertices of its own, so that it can carry
/// its own normal and color.
fn to_render_mesh(surface: &SurfaceMesh, origin: Vec3, ground: &GroundColors) -> Mesh {
    let vertex_count = surface.indices.len();
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    for triangle in surface.indices.as_chunks::<3>().0 {
        let corners = triangle.map(|vertex| Vec3::from(surface.positions[vertex as usize]));
        let normal = (corners[1] - corners[0])
            .cross(corners[2] - corners[0])
            .normalize_or(Vec3::Y);
        let material = facet_material(triangle.map(|vertex| surface.materials[vertex as usize]));
        let material = if normal.y < GRASS_FLATNESS {
            material.exposed()
        } else {
            material
        };
        let middle = origin + (corners[0] + corners[1] + corners[2]) / 3.0;
        let color = facet_color(ground.of(material), middle);
        for corner in corners {
            positions.push(corner.to_array());
            normals.push(normal.to_array());
            colors.push(color);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// The material most of a triangle's corners share, or its first corner's
/// if they all differ.
fn facet_material([first, second, third]: [Material; 3]) -> Material {
    if second == third { second } else { first }
}

/// The color of a facet of `ground` color whose middle is at `middle` in the
/// world. The brightness difference depends only on where the facet is, so
/// it stays the same when the chunk is remeshed.
fn facet_color(ground: [f32; 4], middle: Vec3) -> [f32; 4] {
    let cell = (middle * 4.0).round().as_ivec3().to_array();
    #[expect(clippy::cast_sign_loss, reason = "only the bits matter")]
    let key = (cell[0] as u32).wrapping_mul(0x8da6_b343)
        ^ (cell[1] as u32).wrapping_mul(0xd816_3841)
        ^ (cell[2] as u32).wrapping_mul(0xcb1a_b31f);
    let brightness = 1.0 + FACET_VARIATION * (2.0 * unit_noise(key) - 1.0);
    let [red, green, blue, alpha] = ground;
    [
        red * brightness,
        green * brightness,
        blue * brightness,
        alpha,
    ]
}

/// Vertex color of each ground material in one season, in linear space.
struct GroundColors([[f32; 4]; Material::ALL.len()]);

impl GroundColors {
    fn new(palette: &Palette, season: Season) -> Self {
        Self(Material::ALL.map(|material| {
            srgb(palette.ground(material, season))
                .to_linear()
                .to_f32_array()
        }))
    }

    fn of(&self, material: Material) -> [f32; 4] {
        self.0[material as usize]
    }
}
