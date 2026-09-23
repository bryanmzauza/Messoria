//! Sending each client the terrain around its character.
//!
//! Chunks are sent nearest first and a few per tick, so joining does not
//! flood the connection and the ground under a player arrives first. A hosted
//! world's own client shares the server's terrain and receives nothing.

use std::collections::HashSet;

use bevy::prelude::*;
use lightyear::{
    connection::host::HostClient,
    prelude::{server::ClientOf, *},
};
use messoria_shared::{
    protocol::{Position, TerrainChannel, TerrainUpdate},
    terrain::Terrain,
};
use messoria_voxel::ChunkPos;

use super::TerrainEdited;
use crate::players::ControlledCharacter;

/// Horizontal distance, in chunks, within which a client receives terrain.
/// Matches how far the client's fog lets it see.
const VIEW_RADIUS: i32 = 5;
/// Horizontal distance, in chunks, beyond which a client forgets terrain. The
/// gap to `VIEW_RADIUS` stops chunks at the border from being resent over and
/// over as a player walks back and forth.
const UNLOAD_RADIUS: i32 = 6;
/// Chunks sent to one client per tick.
const CHUNKS_PER_TICK: usize = 4;

pub(super) struct StreamingPlugin;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        // Edits are forwarded every frame so none are missed; new chunks go
        // out once per tick, which paces them independently of frame rate.
        app.add_observer(track_new_client)
            .add_systems(PostUpdate, forward_edits)
            .add_systems(FixedPostUpdate, stream_chunks);
    }
}

/// Chunks a client has been sent and not told to unload.
#[derive(Component, Default)]
struct SentChunks(HashSet<ChunkPos>);

fn track_new_client(
    trigger: On<Add, Connected>,
    remote_clients: Query<(), (With<ClientOf>, Without<HostClient>)>,
    mut commands: Commands,
) {
    if remote_clients.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(SentChunks::default());
    }
}

/// Relays edits to every client that has the edited chunk. Clients without it
/// will receive the edited state when the chunk comes into range.
fn forward_edits(
    mut edits: MessageReader<TerrainEdited>,
    mut clients: Query<(&SentChunks, &mut MessageSender<TerrainUpdate>)>,
) {
    for TerrainEdited(changes) in edits.read() {
        for (sent, mut sender) in &mut clients {
            if sent.0.contains(&changes.chunk) {
                sender.send::<TerrainChannel>(TerrainUpdate::Changed(changes.clone()));
            }
        }
    }
}

fn stream_chunks(
    terrain: Res<Terrain>,
    characters: Query<&Position>,
    mut clients: Query<(
        &mut SentChunks,
        &mut MessageSender<TerrainUpdate>,
        &ControlledCharacter,
    )>,
) {
    for (mut sent, mut sender, character) in &mut clients {
        let Ok(position) = characters.get(character.0) else {
            continue;
        };
        let center = ChunkPos::containing(position.0.floor().as_ivec3());
        let range = |chunk: &ChunkPos| {
            let offset = chunk.0 - center.0;
            offset.x * offset.x + offset.z * offset.z
        };

        sent.0.retain(|chunk| {
            let keep = range(chunk) <= UNLOAD_RADIUS * UNLOAD_RADIUS;
            if !keep {
                sender.send::<TerrainChannel>(TerrainUpdate::Unloaded(*chunk));
            }
            keep
        });

        let mut missing: Vec<ChunkPos> = terrain
            .positions()
            .filter(|chunk| range(chunk) <= VIEW_RADIUS * VIEW_RADIUS && !sent.0.contains(chunk))
            .collect();
        missing.sort_unstable_by_key(|chunk| (range(chunk), *chunk));
        for chunk in missing.into_iter().take(CHUNKS_PER_TICK) {
            let data = terrain
                .get(chunk)
                .expect("listed chunks are loaded")
                .encode();
            sender.send::<TerrainChannel>(TerrainUpdate::Loaded { chunk, data });
            sent.0.insert(chunk);
        }
    }
}
