//! The authoritative terrain: generating it where players are, streaming it
//! to clients and applying players' edits.
//!
//! The valley grows from the world's seed a column of chunks at a time,
//! around each player, and is unloaded again once nobody is near. A saved
//! world only keeps the chunks players changed, which take the place of the
//! generated ones as their columns load.

mod loading;
mod shovel;
mod streaming;

use std::collections::{HashMap, HashSet};

use bevy::{ecs::message::Message, prelude::*};
use messoria_shared::terrain::{ChunkChanged, Terrain};
use messoria_voxel::{Brush, Chunk, ChunkChanges, ChunkPos, Reshape};

use crate::{Beginning, WorldStart};

pub(crate) use loading::LoadedColumns;
pub(crate) use streaming::VIEW_RADIUS;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TerrainEdited>()
            .add_message::<GroundReshaped>()
            .init_resource::<EditedChunks>()
            .add_systems(Startup, restore_edits.in_set(Restoring))
            .add_systems(PostUpdate, note_edited_chunks)
            .add_plugins((
                loading::LoadingPlugin,
                shovel::ShovelPlugin,
                streaming::StreamingPlugin,
            ));
    }
}

/// Setting up at startup what the save kept of the world: systems that
/// build on it run after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Restoring;

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
    /// Edited chunks whose columns are not loaded, kept until they are.
    parked: HashMap<ChunkPos, Chunk>,
    /// Whether any of them changed since the terrain was last saved.
    pub unsaved: bool,
}

impl EditedChunks {
    /// Every edited chunk as it is now: loaded in `terrain`, or parked.
    pub(crate) fn current<'a>(
        &'a self,
        terrain: &'a Terrain,
    ) -> impl Iterator<Item = (ChunkPos, &'a Chunk)> {
        self.chunks.iter().filter_map(|&position| {
            terrain
                .get(position)
                .or_else(|| self.parked.get(&position))
                .map(|chunk| (position, chunk))
        })
    }
}

fn restore_edits(beginning: Res<Beginning>, mut edited: ResMut<EditedChunks>) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        for (position, chunk) in &saved.terrain {
            edited.chunks.insert(*position);
            edited.parked.insert(*position, chunk.clone());
        }
        info!(
            "{} chunks of terrain are as players left them",
            edited.chunks.len()
        );
    }
}

fn note_edited_chunks(mut edits: MessageReader<TerrainEdited>, mut edited: ResMut<EditedChunks>) {
    for TerrainEdited(changes) in edits.read() {
        edited.chunks.insert(changes.chunk);
        edited.unsaved = true;
    }
}
