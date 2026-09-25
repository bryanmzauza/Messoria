//! Terrain meshes, rebuilt as chunks load and change.
//!
//! The terrain of the loaded columns near the camera is drawn in full; past
//! them the land is drawn coarsely (`horizon`). A hosted world's client has
//! the terrain around every player, and draws in full only its own share.
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
    ecs::message::Message,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use messoria_shared::terrain::{ChunkChanged, Terrain, column_chunks};
use messoria_voxel::{ChunkPos, Material, SurfaceMesh, mesh_chunk};
use messoria_worldgen::column_of;

use crate::{camera::WorldCamera, ground::GroundLooks};

/// Time per frame spent rebuilding chunk meshes.
const MESHING_BUDGET: Duration = Duration::from_millis(4);
/// Distance, in columns, within which loaded terrain is drawn in full.
const FULL_RADIUS: i32 = 6;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StaleChunks>()
            .init_resource::<ChunkMeshes>()
            .init_resource::<FullColumns>()
            .add_message::<FullColumnsChanged>()
            .add_systems(
                Update,
                (choose_full_columns, collect_stale_chunks, rebuild_meshes).chain(),
            );
    }
}

/// The columns whose terrain is drawn in full.
#[derive(Resource, Default)]
pub(crate) struct FullColumns(HashSet<IVec2>);

impl FullColumns {
    pub(crate) fn contains(&self, column: IVec2) -> bool {
        self.0.contains(&column)
    }
}

/// A column started or stopped being drawn in full.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct FullColumnsChanged(pub IVec2);

/// Draws in full the loaded columns near the camera, and no others.
fn choose_full_columns(
    terrain: Res<Terrain>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut full: ResMut<FullColumns>,
    mut stale: ResMut<StaleChunks>,
    mut chunk_meshes: ResMut<ChunkMeshes>,
    mut changed: MessageWriter<FullColumnsChanged>,
    mut commands: Commands,
) {
    let center = column_of(camera.translation.xz());
    let wanted: HashSet<IVec2> = (-FULL_RADIUS..=FULL_RADIUS)
        .flat_map(|z| (-FULL_RADIUS..=FULL_RADIUS).map(move |x| IVec2::new(x, z)))
        .filter(|offset| offset.length_squared() <= FULL_RADIUS * FULL_RADIUS)
        .map(|offset| center + offset)
        .filter(|&column| column_chunks(column).all(|chunk| terrain.contains(chunk)))
        .collect();
    if wanted == full.0 {
        return;
    }
    for &column in wanted.difference(&full.0) {
        stale.0.extend(column_chunks(column));
        changed.write(FullColumnsChanged(column));
    }
    for &column in full.0.difference(&wanted) {
        for chunk in column_chunks(column) {
            if let Some(entity) = chunk_meshes.0.remove(&chunk) {
                commands.entity(entity).despawn();
            }
        }
        changed.write(FullColumnsChanged(column));
    }
    full.0 = wanted;
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
    full: Res<FullColumns>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut stale: ResMut<StaleChunks>,
    mut chunk_meshes: ResMut<ChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    if stale.0.is_empty() {
        return;
    }
    // Chunks of columns not drawn in full are meshed when they are.
    stale.0.retain(|chunk| full.contains(chunk.0.xz()));
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
