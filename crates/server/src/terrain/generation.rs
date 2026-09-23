//! The farm valley every world starts with.
//!
//! The terrain is a heightfield: gently rolling ground, one hill to reshape,
//! a level, paved village square, and a rim of higher hills that closes the
//! valley well before the edge of the world.

use bevy::math::{IVec3, Vec2, Vec3};
use messoria_shared::village;
use messoria_voxel::{CHUNK_SIZE, Chunk, ChunkMap, ChunkPos, Material, Voxel};

/// Chunks generated along x and z, centered on the origin.
const HORIZONTAL_CHUNKS: std::ops::Range<i32> = -4..4;
/// Chunks generated along y.
const VERTICAL_CHUNKS: std::ops::Range<i32> = -1..2;
/// Half the width of the world, in meters.
const HALF_EXTENT: i32 = HORIZONTAL_CHUNKS.end * CHUNK_SIZE;
/// Bottom and top of the generated world, in meters.
const BOTTOM_HEIGHT: i32 = VERTICAL_CHUNKS.start * CHUNK_SIZE;
const SKY_HEIGHT: i32 = VERTICAL_CHUNKS.end * CHUNK_SIZE;
/// Edits must stay this far from the bottom and top of the world, so nobody
/// digs through the floor or builds past the sky.
const EDIT_MARGIN: f32 = 3.0;

const FLOOR_HEIGHT: f32 = 8.0;
const RIM_HEIGHT: f32 = 26.0;
/// Slope, as rise over run, beyond which bare stone shows instead of grass.
const STEEP_SLOPE: f32 = 1.2;
/// Distance beyond the village over which its level ground blends into the
/// valley.
const VILLAGE_BLEND: f32 = 8.0;
/// Radius of the paved square in the middle of the village.
const SQUARE_RADIUS: f32 = 9.0;
/// Height steps the village's level is rounded to, matching the levels
/// shovels bring ground to.
const LEVEL_STEP: f32 = 0.5;
/// Depth of the top layer: grass, or the village's paving. More than a
/// sample apart, so it covers the sample just under the surface even where
/// the surface falls exactly on a sample, which then counts as air.
const TOPSOIL_DEPTH: f32 = 1.5;
const SOIL_DEPTH: f32 = 4.0;

/// Generates the whole farm valley.
pub(super) fn farm() -> ChunkMap {
    let mut map = ChunkMap::default();
    for z in HORIZONTAL_CHUNKS {
        for x in HORIZONTAL_CHUNKS {
            let columns = Columns::sample(x, z);
            for y in VERTICAL_CHUNKS {
                let position = ChunkPos(IVec3::new(x, y, z));
                map.insert(position, columns.chunk(position));
            }
        }
    }
    map
}

/// Height of the ground at `(x, z)`, if that part of the world is loaded.
pub(crate) fn ground_height(terrain: &ChunkMap, x: f32, z: f32) -> Option<f32> {
    // Interpolation reads the sample above a point, so stay a voxel inside
    // the top and bottom of the world.
    let top = world(SKY_HEIGHT) - 1.5;
    terrain.surface_below(Vec3::new(x, top, z), top - world(BOTTOM_HEIGHT) - 1.0)
}

/// Whether terrain at `point` may be edited.
pub(crate) fn editable(point: Vec3) -> bool {
    (world(BOTTOM_HEIGHT) + EDIT_MARGIN..world(SKY_HEIGHT) - EDIT_MARGIN).contains(&point.y)
}

/// Terrain height of the valley at `(x, z)`, in meters: the natural ground,
/// leveled around the village.
fn height(x: f32, z: f32) -> f32 {
    let village_level =
        (natural_height(village::CENTER.x, village::CENTER.y) / LEVEL_STEP).round() * LEVEL_STEP;
    let from_village = Vec2::new(x, z).distance(village::CENTER);
    let leveled = 1.0
        - smoothstep(
            village::RADIUS,
            village::RADIUS + VILLAGE_BLEND,
            from_village,
        );
    natural_height(x, z) + (village_level - natural_height(x, z)) * leveled
}

/// Whether the ground at `(x, z)` is part of the village's paved square.
fn paved(x: f32, z: f32) -> bool {
    Vec2::new(x, z).distance(village::CENTER) < SQUARE_RADIUS
}

/// Height of the valley at `(x, z)` before anything was built on it.
fn natural_height(x: f32, z: f32) -> f32 {
    let rolling = 1.2 * (x * 0.045).sin() * (z * 0.038).cos()
        + 0.5 * (x * 0.11 + 1.7).sin() * (z * 0.09 - 0.4).sin();
    let hill = 7.0 * (-(Vec2::new(x - 36.0, z + 28.0).length_squared()) / 338.0).exp();
    // Rounded-square distance from the center: 0 there, 1 at the world's edge.
    let half_extent = world(HALF_EXTENT);
    let edge = ((x / half_extent).powi(4) + (z / half_extent).powi(4)).powf(0.25);
    let rim = RIM_HEIGHT * smoothstep(0.72, 0.97, edge);
    FLOOR_HEIGHT + rolling + hill + rim
}

fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Height and slope of every column in one chunk column.
struct Columns {
    origin_x: i32,
    origin_z: i32,
    heights: Vec<f32>,
    slopes: Vec<f32>,
}

impl Columns {
    fn sample(chunk_x: i32, chunk_z: i32) -> Self {
        let origin_x = chunk_x * CHUNK_SIZE;
        let origin_z = chunk_z * CHUNK_SIZE;
        let mut heights = Vec::new();
        let mut slopes = Vec::new();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (x, z) = (world(origin_x + x), world(origin_z + z));
                heights.push(height(x, z));
                slopes.push(
                    Vec2::new(
                        height(x + 0.5, z) - height(x - 0.5, z),
                        height(x, z + 0.5) - height(x, z - 0.5),
                    )
                    .length(),
                );
            }
        }
        Self {
            origin_x,
            origin_z,
            heights,
            slopes,
        }
    }

    fn chunk(&self, position: ChunkPos) -> Chunk {
        let origin = position.origin();
        debug_assert_eq!((origin.x, origin.z), (self.origin_x, self.origin_z));
        Chunk::from_fn(|local| {
            let column =
                usize::try_from(local.x + local.z * CHUNK_SIZE).expect("local coordinates");
            let (height, slope) = (self.heights[column], self.slopes[column]);
            let y = world(origin.y + local.y);
            let depth = height - y;
            let steep = slope > STEEP_SLOPE;
            let paved = paved(world(origin.x + local.x), world(origin.z + local.z));
            let material = if depth < TOPSOIL_DEPTH && paved {
                Material::Stone
            } else if depth < TOPSOIL_DEPTH && !steep {
                Material::Grass
            } else if depth < SOIL_DEPTH && !steep {
                Material::Soil
            } else {
                Material::Stone
            };
            // Vertical distance overstates the true distance on slopes;
            // scaling by the slope keeps it a fair estimate.
            let distance = -depth / (1.0 + slope * slope).sqrt();
            Voxel::new(distance, material)
        })
    }
}

/// World coordinate of a voxel, in meters.
fn world(voxel: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "world coordinates are far below 2²⁴"
    )]
    let meters = voxel as f32;
    meters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_start_on_the_valley_floor() {
        let terrain = farm();
        let ground = ground_height(&terrain, 0.0, 0.0).expect("the center is generated");
        assert!(
            (ground - height(0.0, 0.0)).abs() < 0.1,
            "ground at {ground}"
        );
    }

    #[test]
    fn the_rim_encloses_the_valley() {
        assert!(height(world(HALF_EXTENT) - 4.0, 0.0) > FLOOR_HEIGHT + RIM_HEIGHT * 0.9);
        assert!(height(0.0, 0.0) < FLOOR_HEIGHT + 2.0);
    }

    #[test]
    fn the_village_stands_on_level_ground() {
        let terrain = farm();
        let at = |offset: Vec2| {
            let point = village::CENTER + offset;
            ground_height(&terrain, point.x, point.y).expect("the village is generated")
        };
        let middle = at(Vec2::ZERO);
        assert!((middle / LEVEL_STEP - (middle / LEVEL_STEP).round()).abs() < 0.05);
        for offset in [
            Vec2::new(10.0, 0.0),
            Vec2::new(-7.0, 9.0),
            Vec2::new(0.0, -13.0),
        ] {
            assert!(
                (at(offset) - middle).abs() < 0.05,
                "ground at {offset} is not level"
            );
        }
        let square = Vec3::new(village::CENTER.x, middle - 0.25, village::CENTER.y);
        assert_eq!(terrain.surface_material(square), Some(Material::Stone));
    }

    #[test]
    fn generation_is_deterministic() {
        let (a, b) = (farm(), farm());
        for position in a.positions() {
            assert_eq!(a.get(position), b.get(position));
        }
    }
}
