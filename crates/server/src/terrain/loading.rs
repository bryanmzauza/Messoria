//! Keeping the terrain loaded around every player: generating the columns
//! that come near someone, nearest first and several at once, and unloading
//! the ones nobody is near any more. Edited chunks are kept aside while
//! their columns are unloaded, and take the place of the generated ones
//! when they load again.

use std::collections::HashSet;

use bevy::{prelude::*, tasks::ComputeTaskPool};
use messoria_shared::{
    protocol::{PlayerId, Position},
    terrain::{ChunkChanged, LoadColumns, Terrain, column_chunks},
    valley::{ColumnLoaded, ColumnUnloaded, Valley},
};
use messoria_worldgen::{column_of, in_world};

use super::{EditedChunks, streaming::VIEW_RADIUS};

/// Distance, in columns, within which the server keeps the terrain loaded
/// around each player: every column clients are sent, and one more, so that
/// what stands at the edge of their range still collides.
const LOAD_RADIUS: i32 = VIEW_RADIUS + 1;
/// Distance beyond which columns are unloaded again. The gap to
/// `LOAD_RADIUS` stops columns at the border from being generated over and
/// over as a player walks back and forth.
const UNLOAD_RADIUS: i32 = LOAD_RADIUS + 2;
/// Columns generated at most in one frame, side by side on every core.
const COLUMNS_PER_FRAME: usize = 16;

pub(super) struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedColumns>()
            .add_systems(PreUpdate, load_around_players.in_set(LoadColumns));
    }
}

/// The columns whose terrain is loaded.
#[derive(Resource, Default)]
pub(crate) struct LoadedColumns(HashSet<IVec2>);

impl LoadedColumns {
    pub(crate) fn contains(&self, column: IVec2) -> bool {
        self.0.contains(&column)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

/// The columns within `radius` columns of `center`.
fn disc(center: IVec2, radius: i32) -> impl Iterator<Item = IVec2> {
    (-radius..=radius)
        .flat_map(move |z| (-radius..=radius).map(move |x| IVec2::new(x, z)))
        .filter(move |offset| offset.length_squared() <= radius * radius)
        .map(move |offset| center + offset)
}

fn load_around_players(
    valley: Res<Valley>,
    characters: Query<&Position, With<PlayerId>>,
    mut loaded: ResMut<LoadedColumns>,
    mut terrain: ResMut<Terrain>,
    mut edited: ResMut<EditedChunks>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut column_loaded: MessageWriter<ColumnLoaded>,
    mut column_unloaded: MessageWriter<ColumnUnloaded>,
) {
    let centers: Vec<IVec2> = characters
        .iter()
        .map(|feet| column_of(feet.0.xz()))
        .collect();
    let nearest = |column: IVec2| {
        centers
            .iter()
            .map(|&center| (column - center).length_squared())
            .min()
    };

    let far: Vec<IVec2> = loaded
        .0
        .iter()
        .copied()
        .filter(|&column| nearest(column).is_none_or(|distance| distance > UNLOAD_RADIUS.pow(2)))
        .collect();
    for column in far {
        for position in column_chunks(column) {
            if let Some(chunk) = terrain.remove(position) {
                if edited.chunks.contains(&position) {
                    edited.parked.insert(position, chunk);
                }
                chunk_changed.write_batch(position.neighborhood().map(ChunkChanged));
            }
        }
        loaded.0.remove(&column);
        column_unloaded.write(ColumnUnloaded(column));
    }

    let mut missing: Vec<IVec2> = centers
        .iter()
        .flat_map(|&center| disc(center, LOAD_RADIUS))
        .filter(|&column| in_world(column) && !loaded.contains(column))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    missing.sort_unstable_by_key(|&column| (nearest(column), column.x, column.y));
    missing.truncate(COLUMNS_PER_FRAME);
    let valley = &valley;
    let generated = ComputeTaskPool::get().scope(|scope| {
        for &column in &missing {
            scope.spawn(async move { (column, valley.column(column)) });
        }
    });
    for (column, chunks) in generated {
        for (position, chunk) in chunks {
            let chunk = edited.parked.remove(&position).unwrap_or(chunk);
            terrain.insert(position, chunk);
            chunk_changed.write_batch(position.neighborhood().map(ChunkChanged));
        }
        loaded.0.insert(column);
        column_loaded.write(ColumnLoaded(column));
    }
}
