//! The land out to the horizon: past the terrain this client has, the
//! valley is drawn from its shape alone, coarsely, in the same ground.
//!
//! The world is cut into tiles of a few columns, each one mesh of the
//! valley's heights on a coarse grid, painted by the ground material from
//! what the ground is made of at each point. Columns whose terrain is drawn
//! in full are left out of their tile, and skirts hang from the edges beside
//! them, so no gap shows where the coarse land meets the full terrain. Tiles
//! are rebuilt as the columns drawn in full change around the camera.

use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use messoria_shared::valley::Valley;
use messoria_voxel::{CHUNK_SIZE, Material};
use messoria_worldgen::{COLUMNS, Landscape};

use crate::{
    camera::WorldCamera,
    ground::GroundLooks,
    terrain::{FullColumns, FullColumnsChanged},
};

/// Columns along each side of a tile.
const TILE_COLUMNS: i32 = 4;
/// Grid points along each side of a column.
const STEPS_PER_COLUMN: i32 = 4;
/// Tiles built or rebuilt at most in one frame, nearest first.
const TILES_PER_FRAME: usize = 2;
/// How far the coarse land sits under the valley's shape, so the full
/// terrain wins where the two meet.
const SINK: f32 = 0.25;
/// How far skirts hang below the coarse land's edges.
const SKIRT: f32 = 6.0;
#[expect(clippy::cast_precision_loss, reason = "small constants")]
const COLUMN_METERS: f32 = CHUNK_SIZE as f32;
#[expect(clippy::cast_precision_loss, reason = "small constants")]
const TILE_METERS: f32 = (TILE_COLUMNS * CHUNK_SIZE) as f32;
/// Distance between grid points.
#[expect(clippy::cast_precision_loss, reason = "small constants")]
const STEP: f32 = COLUMN_METERS / STEPS_PER_COLUMN as f32;

pub(crate) struct HorizonPlugin;

impl Plugin for HorizonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Horizon>()
            .add_systems(Update, (note_changed_tiles, build_tiles).chain());
    }
}

/// The tiles drawn, and those that no longer match what is drawn in full.
#[derive(Resource, Default)]
struct Horizon {
    tiles: HashMap<IVec2, Entity>,
    stale: HashSet<IVec2>,
}

fn tile_of(column: IVec2) -> IVec2 {
    column.div_euclid(IVec2::splat(TILE_COLUMNS))
}

/// Every tile of the world.
fn all_tiles() -> impl Iterator<Item = IVec2> {
    let tiles = tile_of(IVec2::splat(COLUMNS.start))..=tile_of(IVec2::splat(COLUMNS.end - 1));
    let (low, high) = (tiles.start().x, tiles.end().x);
    (low..=high).flat_map(move |z| (low..=high).map(move |x| IVec2::new(x, z)))
}

fn note_changed_tiles(
    mut changed: MessageReader<FullColumnsChanged>,
    mut horizon: ResMut<Horizon>,
) {
    for &FullColumnsChanged(column) in changed.read() {
        // A tile's skirts hang beside its neighbors' columns too.
        for offset in [IVec2::ZERO, IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
            horizon.stale.insert(tile_of(column + offset));
        }
    }
}

fn build_tiles(
    valley: Option<Res<Valley>>,
    full: Res<FullColumns>,
    ground: Res<GroundLooks>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut horizon: ResMut<Horizon>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let Some(valley) = valley else {
        return;
    };
    let viewer = camera.translation.xz() / TILE_METERS;
    let horizon = &mut *horizon;
    let mut wanted: Vec<IVec2> = all_tiles()
        .filter(|tile| !horizon.tiles.contains_key(tile) || horizon.stale.contains(tile))
        .collect();
    wanted.sort_unstable_by(|a, b| {
        let distance = |tile: &IVec2| (tile.as_vec2() + 0.5).distance_squared(viewer);
        distance(a).total_cmp(&distance(b))
    });
    for tile in wanted.into_iter().take(TILES_PER_FRAME) {
        horizon.stale.remove(&tile);
        let mesh = tile_mesh(&valley, tile, |column| full.contains(column));
        match (mesh, horizon.tiles.get(&tile).copied()) {
            (Some(mesh), Some(entity)) => {
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }
            (Some(mesh), None) => {
                let entity = commands
                    .spawn((
                        Name::new(format!("Horizon {tile}")),
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(ground.material.clone()),
                        Transform::default(),
                        // Far beyond where shadows are drawn.
                        NotShadowCaster,
                    ))
                    .id();
                horizon.tiles.insert(tile, entity);
            }
            (None, Some(entity)) => {
                commands.entity(entity).despawn();
                horizon.tiles.remove(&tile);
            }
            (None, None) => {}
        }
    }
}

/// The coarse land of `tile`, leaving out the columns `full` says are drawn
/// in full, or `None` if they all are.
fn tile_mesh(valley: &Landscape, tile: IVec2, full: impl Fn(IVec2) -> bool) -> Option<Mesh> {
    let mut builder = CoarseLand::default();
    let first = tile * TILE_COLUMNS;
    for column_z in 0..TILE_COLUMNS {
        for column_x in 0..TILE_COLUMNS {
            let column = first + IVec2::new(column_x, column_z);
            if full(column) {
                continue;
            }
            let origin = column.as_vec2() * COLUMN_METERS;
            for z in 0..STEPS_PER_COLUMN {
                for x in 0..STEPS_PER_COLUMN {
                    let corner =
                        |dx: i32, dz: i32| origin + IVec2::new(x + dx, z + dz).as_vec2() * STEP;
                    builder.quad(
                        valley,
                        [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)],
                    );
                }
            }
            // Skirts along the sides that meet terrain drawn in full.
            let side = |from: Vec2, to: Vec2, builder: &mut CoarseLand| {
                let direction = (to - from).normalize();
                for index in 0..STEPS_PER_COLUMN {
                    let at = |step: i32| from + direction * along(step);
                    builder.skirt(valley, at(index), at(index + 1));
                }
            };
            let (low, high) = (origin, origin + Vec2::splat(COLUMN_METERS));
            if full(column - IVec2::X) {
                side(Vec2::new(low.x, high.y), low, &mut builder);
            }
            if full(column + IVec2::X) {
                side(Vec2::new(high.x, low.y), high, &mut builder);
            }
            if full(column - IVec2::Y) {
                side(low, Vec2::new(high.x, low.y), &mut builder);
            }
            if full(column + IVec2::Y) {
                side(high, Vec2::new(low.x, high.y), &mut builder);
            }
        }
    }
    builder.build()
}

/// Distance covered by `steps` steps of the grid.
fn along(steps: i32) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "a handful of steps")]
    let steps = steps as f32;
    steps * STEP
}

/// Triangles of coarse land gathered into one mesh, with the ground
/// material's weights in their vertex colors. Neighboring triangles share
/// the vertices at the points of the grid, each worked out once.
#[derive(Default)]
struct CoarseLand {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
    /// The vertex at each point of the grid, on the land or at the foot of
    /// a skirt.
    shared: HashMap<(IVec2, bool), u32>,
}

impl CoarseLand {
    fn vertex(&mut self, valley: &Landscape, point: Vec2, hanging: bool) -> u32 {
        let key = ((point / STEP).round().as_ivec2(), hanging);
        if let Some(&index) = self.shared.get(&key) {
            return index;
        }
        let index = u32::try_from(self.positions.len()).unwrap_or(u32::MAX);
        self.shared.insert(key, index);
        let drop = if hanging { SKIRT } else { 0.0 };
        let height = valley.height(point) - SINK - drop;
        self.positions.push([point.x, height, point.y]);
        self.normals.push(valley.normal(point).to_array());
        let mut weight = [0.0; 4];
        let material = valley.surface_from_afar(point, STEP);
        weight[Material::ALL
            .iter()
            .position(|&each| each == material)
            .expect("every material is listed")] = 1.0;
        self.weights.push(weight);
        index
    }

    /// A quad through four corners going around counterclockwise from
    /// above.
    fn quad(&mut self, valley: &Landscape, corners: [Vec2; 4]) {
        let [a, b, c, d] = corners.map(|corner| self.vertex(valley, corner, false));
        self.indices.extend([a, c, b, a, d, c]);
    }

    /// A curtain hanging below the edge from `from` to `to`, facing out of
    /// the coarse land.
    fn skirt(&mut self, valley: &Landscape, from: Vec2, to: Vec2) {
        let [a, b] = [from, to].map(|point| self.vertex(valley, point, false));
        let [c, d] = [to, from].map(|point| self.vertex(valley, point, true));
        self.indices.extend([a, b, c, a, c, d, a, c, b, a, d, c]);
    }

    fn build(self) -> Option<Mesh> {
        if self.indices.is_empty() {
            return None;
        }
        Some(
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.weights)
            .with_inserted_indices(Indices::U32(self.indices)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(mesh: &Mesh) -> (usize, usize) {
        (
            mesh.count_vertices(),
            mesh.indices().map_or(0, Indices::len),
        )
    }

    #[test]
    fn tiles_share_their_grid_and_leave_out_what_is_drawn_in_full() {
        let valley = Landscape::new(7);
        let tile = IVec2::new(1, 2);
        let whole = tile_mesh(&valley, tile, |_| false).expect("nothing is drawn in full");
        let side = usize::try_from(TILE_COLUMNS * STEPS_PER_COLUMN + 1).expect("small");
        let quads = (side - 1) * (side - 1);
        assert_eq!(counts(&whole), (side * side, quads * 6));

        let middle = tile * TILE_COLUMNS + IVec2::ONE;
        let holed = tile_mesh(&valley, tile, |column| column == middle)
            .expect("most of the tile is coarse");
        let per_column = usize::try_from(STEPS_PER_COLUMN * STEPS_PER_COLUMN).expect("small");
        let skirts = 4 * usize::try_from(STEPS_PER_COLUMN).expect("small");
        assert_eq!(counts(&holed).1, (quads - per_column) * 6 + skirts * 12);

        let all = tile_mesh(&valley, tile, |_| true);
        assert!(all.is_none(), "a tile drawn in full has no coarse land");
    }
}
