//! Terrain meshes, rebuilt as chunks load and change.
//!
//! Meshing runs on the main thread under a per-frame time budget, nearest
//! chunks first, so loading a whole area never stalls a frame.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use messoria_shared::terrain::{ChunkChanged, Terrain};
use messoria_voxel::{ChunkPos, Material, SurfaceMesh, mesh_chunk};

/// Time per frame spent rebuilding chunk meshes.
const MESHING_BUDGET: Duration = Duration::from_millis(4);

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

fn collect_stale_chunks(mut changes: MessageReader<ChunkChanged>, mut stale: ResMut<StaleChunks>) {
    stale.0.extend(changes.read().map(|changed| changed.0));
}

fn rebuild_meshes(
    terrain: Res<Terrain>,
    material: Res<TerrainMaterial>,
    camera: Single<&Transform, With<Camera3d>>,
    mut stale: ResMut<StaleChunks>,
    mut chunk_meshes: ResMut<ChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    if stale.0.is_empty() {
        return;
    }
    let started = Instant::now();
    let viewer = ChunkPos::containing(camera.translation.floor().as_ivec3());
    let mut queue: Vec<ChunkPos> = stale.0.iter().copied().collect();
    queue.sort_unstable_by_key(|chunk| (chunk.0 - viewer.0).length_squared());

    for chunk in queue {
        if started.elapsed() > MESHING_BUDGET {
            break;
        }
        stale.0.remove(&chunk);

        let surface = mesh_chunk(&terrain, chunk).filter(|surface| !surface.is_empty());
        match (surface, chunk_meshes.0.get(&chunk).copied()) {
            (Some(surface), Some(entity)) => {
                commands
                    .entity(entity)
                    .insert(Mesh3d(meshes.add(to_render_mesh(surface))));
            }
            (Some(surface), None) => {
                let entity = commands
                    .spawn((
                        Name::new(format!("Terrain chunk {}", chunk.0)),
                        Mesh3d(meshes.add(to_render_mesh(surface))),
                        MeshMaterial3d(material.0.clone()),
                        Transform::from_translation(chunk.origin().as_vec3()),
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

fn to_render_mesh(surface: SurfaceMesh) -> Mesh {
    let colors: Vec<[f32; 4]> = surface.materials.iter().map(|&m| ground_color(m)).collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, surface.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, surface.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(surface.indices))
}

/// Vertex color of each ground material, in linear space.
fn ground_color(material: Material) -> [f32; 4] {
    let color = match material {
        Material::Grass => Color::srgb(0.36, 0.52, 0.28),
        Material::Soil => Color::srgb(0.45, 0.33, 0.22),
        Material::Stone => Color::srgb(0.52, 0.5, 0.47),
        Material::Sand => Color::srgb(0.82, 0.74, 0.52),
    };
    color.to_linear().to_f32_array()
}
