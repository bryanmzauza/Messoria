//! The terrain as this app knows it: the columns near players on a server,
//! the columns in range on a client. A hosted world shares one copy between
//! both.

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::{Client, MessageReceiver, MessageSystems};
use messoria_voxel::{Chunk, ChunkMap, ChunkPos};
use messoria_worldgen::{LAYERS, Landscape};

use crate::{
    protocol::TerrainUpdate,
    scenery::{Scenery, SceneryChanged},
    valley::{ColumnLoaded, ColumnUnloaded, Valley},
};

/// The loaded terrain. Movement collides against it, so it must hold the same
/// voxels on the server and on a predicting client.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct Terrain(pub ChunkMap);

/// A chunk was loaded, edited or unloaded, so meshes that sample it are stale.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkChanged(pub ChunkPos);

/// Loads and unloads columns of terrain: generating them on a server,
/// receiving them on a client. Scenery grows on them after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoadColumns;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Terrain>()
            .add_message::<ChunkChanged>()
            // Before the fixed-step loop, so prediction sees this frame's terrain.
            .add_systems(
                PreUpdate,
                apply_terrain_updates
                    .in_set(LoadColumns)
                    .after(MessageSystems::Receive),
            );
    }
}

/// The chunks of `column`, from the bottom up.
pub fn column_chunks(column: IVec2) -> impl Iterator<Item = ChunkPos> {
    LAYERS.map(move |y| ChunkPos(IVec3::new(column.x, y, column.y)))
}

fn apply_terrain_updates(
    mut receivers: Query<&mut MessageReceiver<TerrainUpdate>, With<Client>>,
    mut terrain: ResMut<Terrain>,
    mut scenery: ResMut<Scenery>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut loaded: MessageWriter<ColumnLoaded>,
    mut unloaded: MessageWriter<ColumnUnloaded>,
    mut scenery_changed: MessageWriter<SceneryChanged>,
    mut commands: Commands,
) {
    for mut receiver in &mut receivers {
        for update in receiver.receive() {
            match update {
                TerrainUpdate::World { seed } => {
                    commands.insert_resource(Valley(Landscape::new(seed)));
                }
                TerrainUpdate::Column {
                    column,
                    chunks,
                    gathered,
                } => {
                    let decoded: Result<Vec<Chunk>, _> =
                        chunks.iter().map(|data| Chunk::decode(data)).collect();
                    match decoded {
                        Ok(decoded) if decoded.len() == LAYERS.len() => {
                            for (position, chunk) in column_chunks(column).zip(decoded) {
                                terrain.insert(position, chunk);
                                chunk_changed
                                    .write_batch(position.neighborhood().map(ChunkChanged));
                            }
                            scenery.remember_gathered(gathered);
                            loaded.write(ColumnLoaded(column));
                        }
                        Ok(_) => {
                            error!("discarding column {column}: it has the wrong number of chunks");
                        }
                        Err(problem) => error!("discarding column {column}: {problem}"),
                    }
                }
                TerrainUpdate::Changed(changes) => {
                    if terrain.apply_changes(&changes) {
                        chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
                    }
                }
                TerrainUpdate::ColumnUnloaded(column) => {
                    scenery.forget_gathered_in(column);
                    for position in column_chunks(column) {
                        if terrain.remove(position).is_some() {
                            chunk_changed.write_batch(position.neighborhood().map(ChunkChanged));
                        }
                    }
                    unloaded.write(ColumnUnloaded(column));
                }
                TerrainUpdate::Gathered { prop, day } => {
                    scenery.gather(prop, day);
                    scenery_changed.write(SceneryChanged(prop));
                }
                TerrainUpdate::Regrown(prop) => {
                    scenery.regrow(prop);
                    scenery_changed.write(SceneryChanged(prop));
                }
            }
        }
    }
}
