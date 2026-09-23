//! The authoritative terrain: generating it, streaming it to clients and
//! applying players' edits.

mod generation;
mod shovel;
mod streaming;

use bevy::{ecs::message::Message, prelude::*};
use messoria_shared::terrain::{ChunkChanged, Terrain};
use messoria_voxel::{Brush, ChunkChanges};

use generation::editable;
pub(crate) use generation::ground_height;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TerrainEdited>()
            .add_message::<GroundReshaped>()
            .add_systems(Startup, generate_world)
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

fn generate_world(mut terrain: ResMut<Terrain>, mut chunk_changed: MessageWriter<ChunkChanged>) {
    **terrain = generation::farm();
    chunk_changed.write_batch(terrain.positions().map(ChunkChanged));
    info!("generated {} terrain chunks", terrain.len());
}
