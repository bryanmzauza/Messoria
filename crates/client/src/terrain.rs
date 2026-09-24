//! Terrain meshes, rebuilt as chunks load and change.
//!
//! Meshing runs on the main thread under a per-frame time budget, nearest
//! chunks first, so loading a whole area never stalls a frame.
//!
//! Terrain is shaded smoothly and painted by the ground material (`ground`),
//! from the ground material at each vertex.

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

use crate::{camera::WorldCamera, ground::GroundLooks};

/// Time per frame spent rebuilding chunk meshes.
const MESHING_BUDGET: Duration = Duration::from_millis(4);

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StaleChunks>()
            .init_resource::<ChunkMeshes>()
            .add_systems(Update, (collect_stale_chunks, rebuild_meshes).chain());
    }
}

/// Chunks whose meshes no longer match the terrain.
#[derive(Resource, Default)]
struct StaleChunks(HashSet<ChunkPos>);

/// The entity displaying each chunk that has a visible surface.
#[derive(Resource, Default)]
struct ChunkMeshes(HashMap<ChunkPos, Entity>);

fn collect_stale_chunks(mut changes: MessageReader<ChunkChanged>, mut stale: ResMut<StaleChunks>) {
    stale.0.extend(changes.read().map(|changed| changed.0));
}

fn rebuild_meshes(
    terrain: Res<Terrain>,
    ground: Res<GroundLooks>,
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
                commands
                    .entity(entity)
                    .insert(Mesh3d(meshes.add(to_render_mesh(&surface))));
            }
            (Some(surface), None) => {
                let entity = commands
                    .spawn((
                        Name::new(format!("Terrain chunk {}", chunk.0)),
                        Mesh3d(meshes.add(to_render_mesh(&surface))),
                        MeshMaterial3d(ground.material.clone()),
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

/// A chunk's surface as a mesh to draw. Its vertex colors carry the
/// ground material at each vertex, as a weight of one in that material's
/// channel, for the ground shader to blend.
fn to_render_mesh(surface: &SurfaceMesh) -> Mesh {
    let weights: Vec<[f32; 4]> = surface
        .materials
        .iter()
        .map(|&material| {
            let mut weight = [0.0; 4];
            weight[Material::ALL
                .iter()
                .position(|&each| each == material)
                .expect("every material is listed")] = 1.0;
            weight
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, surface.positions.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, surface.normals.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, weights)
    .with_inserted_indices(Indices::U32(surface.indices.clone()))
}
