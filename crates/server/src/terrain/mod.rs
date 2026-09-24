//! The authoritative terrain: generating it, streaming it to clients and
//! applying players' edits.
//!
//! The valley is generated the same way every time, so a saved world only
//! keeps the chunks players changed and lays them over a freshly generated
//! valley.

mod generation;
mod shovel;
mod streaming;

use std::collections::HashSet;

use bevy::{ecs::message::Message, prelude::*};
use messoria_shared::terrain::{ChunkChanged, Terrain};
use messoria_voxel::{Brush, ChunkChanges, ChunkPos, Reshape};

use crate::{Beginning, WorldStart};

use generation::editable;
pub(crate) use generation::{farm, ground_height, half_width};

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TerrainEdited>()
            .add_message::<GroundReshaped>()
            .init_resource::<EditedChunks>()
            .configure_sets(
                Startup,
                (WorldBuilding::Generate, WorldBuilding::Restore).chain(),
            )
            .add_systems(
                Startup,
                (
                    generate_valley.in_set(WorldBuilding::Generate),
                    restore_edits.in_set(WorldBuilding::Restore),
                ),
            )
            .add_systems(PostUpdate, note_edited_chunks)
            .add_plugins((shovel::ShovelPlugin, streaming::StreamingPlugin));
    }
}

/// Building the terrain at startup: first the valley as generated, then the
/// changes players made to it laid over it. Whatever is placed from the seed
/// alone, such as scenery, is placed in between, on the generated valley, so
/// that the same seed always places it the same way.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WorldBuilding {
    Generate,
    Restore,
}

/// Voxels changed by an edit, to be forwarded to clients that have the chunk.
#[derive(Message, Clone, Debug)]
pub(crate) struct TerrainEdited(ChunkChanges);

/// Applies `edit` to the terrain, and tells the systems that keep the
/// terrain's copies and meshes about it.
pub(crate) fn reshape(
    terrain: &mut Terrain,
    edit: &impl Reshape,
    edited: &mut MessageWriter<TerrainEdited>,
    chunk_changed: &mut MessageWriter<ChunkChanged>,
) {
    for changes in terrain.reshape(edit) {
        chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
        edited.write(TerrainEdited(changes));
    }
}

/// The ground was moved by `Brush`, so anything resting on it may be
/// disturbed.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct GroundReshaped(pub Brush);

/// Chunks that differ from the generated valley, which is what a save keeps.
#[derive(Resource, Default)]
pub(crate) struct EditedChunks {
    pub chunks: HashSet<ChunkPos>,
    /// Whether any of them changed since the terrain was last saved.
    pub unsaved: bool,
}

fn generate_valley(mut terrain: ResMut<Terrain>) {
    **terrain = farm();
}

fn restore_edits(
    beginning: Res<Beginning>,
    mut terrain: ResMut<Terrain>,
    mut edited: ResMut<EditedChunks>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        for (position, chunk) in &saved.terrain {
            terrain.insert(*position, chunk.clone());
            edited.chunks.insert(*position);
        }
    }
    chunk_changed.write_batch(terrain.positions().map(ChunkChanged));
    info!(
        "generated {} terrain chunks, {} of them as players left them",
        terrain.len(),
        edited.chunks.len()
    );
}

fn note_edited_chunks(mut edits: MessageReader<TerrainEdited>, mut edited: ResMut<EditedChunks>) {
    for TerrainEdited(changes) in edits.read() {
        edited.chunks.insert(changes.chunk);
        edited.unsaved = true;
    }
}
