//! Sending each client the valley around its character.
//!
//! A client first learns the world's seed, then receives the terrain a
//! column at a time, nearest first and a few per tick, so joining does not
//! flood the connection and the ground under a player arrives first. With
//! each column comes what players gathered on it; changes to the terrain and
//! the scenery of columns a client has follow as they happen. A hosted
//! world's own client shares the server's valley and receives nothing.

use std::collections::HashSet;

use bevy::prelude::*;
use lightyear::{
    connection::host::HostClient,
    prelude::{server::ClientOf, *},
};
use messoria_shared::{
    protocol::{Position, TerrainChannel, TerrainUpdate},
    scenery::{Scenery, SceneryChanged, column_of_prop},
    terrain::{Terrain, column_chunks},
    valley::Valley,
};
use messoria_worldgen::column_of;

use super::{LoadedColumns, TerrainEdited};
use crate::players::ControlledCharacter;

/// Distance, in columns, within which a client receives terrain. Matches how
/// far the client draws the ground in full.
pub(crate) const VIEW_RADIUS: i32 = 5;
/// Distance, in columns, beyond which a client forgets terrain. The gap to
/// `VIEW_RADIUS` stops columns at the border from being resent over and over
/// as a player walks back and forth.
const UNLOAD_RADIUS: i32 = VIEW_RADIUS + 1;
/// Columns sent to one client per tick.
const COLUMNS_PER_TICK: usize = 2;

pub(super) struct StreamingPlugin;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        // Changes are forwarded every frame so none are missed; new columns
        // go out once per tick, which paces them independently of frame rate.
        app.add_observer(track_new_client)
            .add_systems(PostUpdate, (forward_edits, forward_scenery))
            .add_systems(FixedPostUpdate, stream_columns);
    }
}

/// What a client has been sent: the seed, and the columns it has not been
/// told to unload.
#[derive(Component, Default)]
struct Sent {
    seed: bool,
    columns: HashSet<IVec2>,
}

fn track_new_client(
    trigger: On<Add, Connected>,
    remote_clients: Query<(), (With<ClientOf>, Without<HostClient>)>,
    mut commands: Commands,
) {
    if remote_clients.contains(trigger.entity) {
        commands.entity(trigger.entity).insert(Sent::default());
    }
}

/// Relays edits to every client that has the edited chunk's column. Clients
/// without it will receive the edited state when the column comes into range.
fn forward_edits(
    mut edits: MessageReader<TerrainEdited>,
    mut clients: Query<(&Sent, &mut MessageSender<TerrainUpdate>)>,
) {
    for TerrainEdited(changes) in edits.read() {
        let column = changes.chunk.0.xz();
        for (sent, mut sender) in &mut clients {
            if sent.columns.contains(&column) {
                sender.send::<TerrainChannel>(TerrainUpdate::Changed(changes.clone()));
            }
        }
    }
}

/// Relays gathering and growing back to every client that has the prop's
/// column.
fn forward_scenery(
    mut changes: MessageReader<SceneryChanged>,
    scenery: Res<Scenery>,
    mut clients: Query<(&Sent, &mut MessageSender<TerrainUpdate>)>,
) {
    for &SceneryChanged(prop) in changes.read() {
        let Some(column) = column_of_prop(&scenery, prop) else {
            continue;
        };
        let update = match scenery.gathered(prop) {
            Some(day) => TerrainUpdate::Gathered { prop, day },
            None => TerrainUpdate::Regrown(prop),
        };
        for (sent, mut sender) in &mut clients {
            if sent.columns.contains(&column) {
                sender.send::<TerrainChannel>(update.clone());
            }
        }
    }
}

fn stream_columns(
    valley: Res<Valley>,
    terrain: Res<Terrain>,
    loaded: Res<LoadedColumns>,
    scenery: Res<Scenery>,
    characters: Query<&Position>,
    mut clients: Query<(
        &mut Sent,
        &mut MessageSender<TerrainUpdate>,
        &ControlledCharacter,
    )>,
) {
    for (mut sent, mut sender, character) in &mut clients {
        if !sent.seed {
            sender.send::<TerrainChannel>(TerrainUpdate::World {
                seed: valley.seed(),
            });
            sent.seed = true;
        }
        let Ok(position) = characters.get(character.0) else {
            continue;
        };
        let center = column_of(position.0.xz());
        let range = |column: IVec2| (column - center).length_squared();

        sent.columns.retain(|&column| {
            let keep = range(column) <= UNLOAD_RADIUS.pow(2);
            if !keep {
                sender.send::<TerrainChannel>(TerrainUpdate::ColumnUnloaded(column));
            }
            keep
        });

        let mut missing: Vec<IVec2> = (-VIEW_RADIUS..=VIEW_RADIUS)
            .flat_map(|z| (-VIEW_RADIUS..=VIEW_RADIUS).map(move |x| center + IVec2::new(x, z)))
            .filter(|&column| {
                range(column) <= VIEW_RADIUS.pow(2)
                    && loaded.contains(column)
                    && !sent.columns.contains(&column)
            })
            .collect();
        missing.sort_unstable_by_key(|&column| (range(column), column.x, column.y));
        for column in missing.into_iter().take(COLUMNS_PER_TICK) {
            let chunks = column_chunks(column)
                .map(|position| {
                    terrain
                        .get(position)
                        .expect("loaded columns have all their chunks")
                        .encode()
                })
                .collect();
            sender.send::<TerrainChannel>(TerrainUpdate::Column {
                column,
                chunks,
                gathered: scenery.gathered_in(column),
            });
            sent.columns.insert(column);
        }
    }
}
