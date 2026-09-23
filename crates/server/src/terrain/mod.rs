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
use messoria_voxel::{Brush, ChunkChanges, ChunkPos};

use crate::{Beginning, WorldStart};

use generation::editable;
pub(crate) use generation::ground_height;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TerrainEdited>()
            .add_message::<GroundReshaped>()
            .init_resource::<EditedChunks>()
            .add_systems(Startup, generate_world)
            .add_systems(PostUpdate, note_edited_chunks)
            .add_plugins((shovel::ShovelPlugin, streaming::StreamingPlugin));
    }
}

/// Voxels changed by an edit, to be forwarded to clients that have the chunk.
#[derive(Message, Clone, Debug)]
struct TerrainEdited(ChunkChanges);

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

fn generate_world(
    beginning: Res<Beginning>,
    mut terrain: ResMut<Terrain>,
    mut edited: ResMut<EditedChunks>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
) {
    **terrain = generation::farm();
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
