//! The terrain as this app knows it: the whole world on a server, the chunks
//! in range on a client. A hosted world shares one copy between both.

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::{Client, MessageReceiver, MessageSystems};
use messoria_voxel::{Chunk, ChunkMap, ChunkPos};

use crate::protocol::TerrainUpdate;

/// The loaded terrain. Movement collides against it, so it must hold the same
/// voxels on the server and on a predicting client.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct Terrain(pub ChunkMap);

/// A chunk was loaded, edited or unloaded, so meshes that sample it are stale.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkChanged(pub ChunkPos);

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Terrain>()
            .add_message::<ChunkChanged>()
            // Before the fixed-step loop, so prediction sees this frame's terrain.
            .add_systems(
                PreUpdate,
                apply_terrain_updates.after(MessageSystems::Receive),
            );
    }
}

fn apply_terrain_updates(
    mut receivers: Query<&mut MessageReceiver<TerrainUpdate>, With<Client>>,
    mut terrain: ResMut<Terrain>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
) {
    for mut receiver in &mut receivers {
        for update in receiver.receive() {
            match update {
                TerrainUpdate::Loaded { chunk, data } => match Chunk::decode(&data) {
                    Ok(voxels) => {
                        terrain.insert(chunk, voxels);
                        chunk_changed.write_batch(chunk.neighborhood().map(ChunkChanged));
                    }
                    Err(error) => error!("discarding chunk {:?}: {error}", chunk.0),
                },
                TerrainUpdate::Changed(changes) => {
                    if terrain.apply_changes(&changes) {
                        chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
                    }
                }
                TerrainUpdate::Unloaded(chunk) => {
                    if terrain.remove(chunk).is_some() {
                        chunk_changed.write_batch(chunk.neighborhood().map(ChunkChanged));
                    }
                }
            }
        }
    }
}
