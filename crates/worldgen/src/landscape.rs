//! The shape of the valley and what its ground is made of.
//!
//! The ground is a heightfield built up in layers: broad rolling land, low
//! and gentle across the farmland around the village and rising into hills
//! beyond it; mountains closing the world at its edge; roads graded into the
//! ground; the village leveled, its square paved, over the roads that leave
//! it; and the river's channel carved last, so it crosses roads as fords.

use glam::{IVec2, IVec3, Vec2, Vec3};
use messoria_voxel::{CHUNK_SIZE, Chunk, ChunkPos, Material, Voxel};

use crate::{
    noise::{fractal, mix, smoothstep},
    river::{RIVER_HALF_WIDTH, River, RiverPoint},
    roads::{ROAD_HALF_WIDTH, Roads},
};

/// Chunk columns along x and z, centered on the origin: the world is 2 km
/// across.
pub const COLUMNS: std::ops::Range<i32> = -32..32;
/// Chunks in each column, along y.
pub const LAYERS: std::ops::Range<i32> = -1..2;
/// Half the width of the world, in meters.
pub const HALF_WIDTH: f32 = 1024.0;
/// Bottom and top of the world, in meters.
pub const BOTTOM: f32 = -32.0;
pub const TOP: f32 = 64.0;

/// The middle of the village, which stands on level ground out to its
/// radius, with a paved square in its middle.
pub const VILLAGE_CENTER: Vec2 = Vec2::new(0.0, -30.0);
pub const VILLAGE_RADIUS: f32 = 14.0;
const SQUARE_RADIUS: f32 = 9.0;
/// Distance beyond the village over which its level ground blends into the
/// land around it.
const VILLAGE_BLEND: f32 = 8.0;
/// Height steps the village's level is rounded to, matching the levels
/// shovels bring ground to.
const LEVEL_STEP: f32 = 0.5;

/// Height of the land's middle, and of its broad rolls: gentle within the
/// farmland, hilly in the wilds past it.
const FLOOR_HEIGHT: f32 = 12.0;
const FARMLAND_ROLL: f32 = 2.5;
const WILDS_ROLL: f32 = 14.0;
const FARMLAND_RADIUS: f32 = 320.0;
const WILDS_RADIUS: f32 = 760.0;
/// Size of the land's broad rolls and of the small ones over them.
const ROLL_SIZE: f32 = 220.0;
const RIPPLE_SIZE: f32 = 45.0;
const RIPPLE_HEIGHT: f32 = 0.6;
/// Ridges rising in the wilds, their height and size.
const RIDGE_HEIGHT: f32 = 12.0;
const RIDGE_SIZE: f32 = 300.0;
/// Mountains closing the world: their height, and where they rise, as a
/// share of the way from the middle to the edge.
const RIM_HEIGHT: f32 = 30.0;
const RIM_FOOT: f32 = 0.8;
const RIM_TOP: f32 = 0.97;
/// Heights above this are rounded off toward `PEAK_HEIGHT`, which stays
/// well under the top of the world.
const PEAK_KNEE: f32 = 46.0;
const PEAK_HEIGHT: f32 = 56.0;

/// Slope, as rise over run, beyond which bare stone shows instead of grass.
const STEEP_SLOPE: f32 = 1.2;
/// How far past the channel's edge the river's banks are sand.
const SANDY_BANKS: f32 = 1.8;
/// How far the water's surface reaches past the channel's edge, up the
/// banks.
const WATER_REACH: f32 = 0.6;
/// Depth of the top layer: grass, or the village's paving. More than a
/// sample apart, so it covers the sample just under the surface even where
/// the surface falls exactly on a sample, which then counts as air.
const TOPSOIL_DEPTH: f32 = 1.5;
const SOIL_DEPTH: f32 = 4.0;
/// Edits must stay this far from the bottom and top of the world, so nobody
/// digs through the floor or builds past the sky.
const EDIT_MARGIN: f32 = 3.0;

/// The valley a world grows from its seed.
pub struct Landscape {
    seed: u64,
    village_level: f32,
    roads: Roads,
    river: River,
}

impl Landscape {
    pub fn new(seed: u64) -> Self {
        let village_level =
            (natural_height(seed, VILLAGE_CENTER) / LEVEL_STEP).round() * LEVEL_STEP;
        let valley = |point| leveled(natural_height(seed, point), village_level, point);
        let roads = Roads::lay(seed, VILLAGE_CENTER, SQUARE_RADIUS, village_level, valley);
        let river = River::lay(seed, HALF_WIDTH, |point| {
            leveled(
                roads.grade(point, natural_height(seed, point)),
                village_level,
                point,
            )
        });
        Self {
            seed,
            village_level,
            roads,
            river,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Height of the ground at `point`, in meters, before anyone dug it.
    pub fn height(&self, point: Vec2) -> f32 {
        let natural = natural_height(self.seed, point);
        let graded = self.roads.grade(point, natural);
        self.river
            .carve(point, leveled(graded, self.village_level, point))
    }

    /// Steepness of the ground at `point`, as rise over run.
    pub fn slope(&self, point: Vec2) -> f32 {
        let across = |offset: Vec2| self.height(point + offset) - self.height(point - offset);
        Vec2::new(across(Vec2::X * 0.5), across(Vec2::Y * 0.5)).length()
    }

    /// The upward direction of the ground at `point`.
    pub fn normal(&self, point: Vec2) -> Vec3 {
        let across = |offset: Vec2| self.height(point + offset) - self.height(point - offset);
        Vec3::new(-across(Vec2::X * 0.5), 1.0, -across(Vec2::Y * 0.5)).normalize()
    }

    /// What the surface at `point` is made of.
    pub fn surface(&self, point: Vec2) -> Material {
        self.material_at(point, self.slope(point))
    }

    /// How far into the wilds `point` is: 0 in the farmland around the
    /// village, rising to 1 in the hills beyond it.
    pub fn wildness(&self, point: Vec2) -> f32 {
        smoothstep(
            FARMLAND_RADIUS,
            WILDS_RADIUS,
            point.distance(VILLAGE_CENTER),
        )
    }

    /// What the surface around `point` looks like from far enough away that
    /// only one point in every `spacing` meters is seen: roads and the
    /// river's banks are widened to be seen at all.
    pub fn surface_from_afar(&self, point: Vec2, spacing: f32) -> Material {
        let reach = spacing / 2.0;
        if self.river.distance(point) < RIVER_HALF_WIDTH + SANDY_BANKS + reach {
            Material::Sand
        } else if self
            .roads
            .distance(point)
            .is_some_and(|distance| distance < ROAD_HALF_WIDTH + reach)
        {
            Material::Soil
        } else {
            self.surface(point)
        }
    }

    /// Height of the river's water at `point`, if it flows there.
    pub fn water_level(&self, point: Vec2) -> Option<f32> {
        (self.river.distance(point) < RIVER_HALF_WIDTH + WATER_REACH)
            .then(|| self.river.level(point.y))
    }

    /// The river's course from one edge of the world to the other.
    pub fn river(&self) -> impl Iterator<Item = RiverPoint> + '_ {
        self.river.course()
    }

    /// Distance from `point` to the middle of the nearest road, if it is on
    /// one or beside it.
    pub fn road_distance(&self, point: Vec2) -> Option<f32> {
        self.roads.distance(point)
    }

    /// Distance from `point` to the middle of the river.
    pub fn river_distance(&self, point: Vec2) -> f32 {
        self.river.distance(point)
    }

    /// The chunks of a column of the world, from the bottom up.
    pub fn column(&self, column: IVec2) -> Vec<(ChunkPos, Chunk)> {
        let ground = ColumnGround::sample(self, column);
        LAYERS
            .map(|y| {
                let position = ChunkPos(IVec3::new(column.x, y, column.y));
                (position, ground.chunk(position))
            })
            .collect()
    }

    /// What the surface at `point`, of steepness `slope`, is made of.
    pub(crate) fn material_at(&self, point: Vec2, slope: f32) -> Material {
        if point.distance(VILLAGE_CENTER) < SQUARE_RADIUS {
            Material::Stone
        } else if self.river.distance(point) < RIVER_HALF_WIDTH + SANDY_BANKS {
            Material::Sand
        } else if self
            .roads
            .distance(point)
            .is_some_and(|distance| distance < ROAD_HALF_WIDTH)
        {
            Material::Soil
        } else if slope > STEEP_SLOPE {
            Material::Stone
        } else {
            Material::Grass
        }
    }
}

/// Whether ground at height `y` may be dug or raised.
pub fn editable(y: f32) -> bool {
    (BOTTOM + EDIT_MARGIN..TOP - EDIT_MARGIN).contains(&y)
}

/// The chunk column containing `point`.
pub fn column_of(point: Vec2) -> IVec2 {
    #[expect(clippy::cast_precision_loss, reason = "the chunk size is small")]
    let size = CHUNK_SIZE as f32;
    (point / size).floor().as_ivec2()
}

/// Whether `column` is part of the world.
pub fn in_world(column: IVec2) -> bool {
    COLUMNS.contains(&column.x) && COLUMNS.contains(&column.y)
}

/// Height of the land at `point` before the village, roads and river shaped
/// it.
fn natural_height(seed: u64, point: Vec2) -> f32 {
    let wildness = smoothstep(
        FARMLAND_RADIUS,
        WILDS_RADIUS,
        point.distance(VILLAGE_CENTER),
    );
    let roll = FARMLAND_ROLL + (WILDS_ROLL - FARMLAND_ROLL) * wildness;
    let broad = fractal(mix(&[seed, 0x301]), point / ROLL_SIZE, 4) * roll;
    let ripples = fractal(mix(&[seed, 0x302]), point / RIPPLE_SIZE, 2) * RIPPLE_HEIGHT;
    let ridge = 1.0 - fractal(mix(&[seed, 0x303]), point / RIDGE_SIZE, 3).abs();
    let ridges = RIDGE_HEIGHT * ridge * ridge * wildness;
    // Rounded-square distance from the middle: 0 there, 1 at the world's edge.
    let edge = ((point.x / HALF_WIDTH).powi(4) + (point.y / HALF_WIDTH).powi(4)).powf(0.25);
    let rim = RIM_HEIGHT * smoothstep(RIM_FOOT, RIM_TOP, edge);
    let height = FLOOR_HEIGHT + broad + ripples + ridges + rim;
    if height > PEAK_KNEE {
        let room = PEAK_HEIGHT - PEAK_KNEE;
        PEAK_KNEE + room * ((height - PEAK_KNEE) / room).tanh()
    } else {
        height
    }
}

/// `height` at `point`, leveled across the village and blending into the
/// land around it.
fn leveled(height: f32, village_level: f32, point: Vec2) -> f32 {
    let from_village = point.distance(VILLAGE_CENTER);
    let level = 1.0 - smoothstep(VILLAGE_RADIUS, VILLAGE_RADIUS + VILLAGE_BLEND, from_village);
    height + (village_level - height) * level
}

/// Height, slope and surface of every column of voxels in one chunk column,
/// sampled once for all its chunks.
struct ColumnGround {
    origin: IVec2,
    heights: Vec<f32>,
    slopes: Vec<f32>,
    materials: Vec<Material>,
    lowest: f32,
    highest: f32,
}

impl ColumnGround {
    fn sample(landscape: &Landscape, column: IVec2) -> Self {
        let origin = column * CHUNK_SIZE;
        // One sample of border on each side, for slopes by central difference.
        let side = CHUNK_SIZE + 2;
        let mut border = Vec::new();
        for z in -1..=CHUNK_SIZE {
            for x in -1..=CHUNK_SIZE {
                border.push(landscape.height(meters(origin + IVec2::new(x, z))));
            }
        }
        let at = |x: i32, z: i32| {
            border[usize::try_from((x + 1) + (z + 1) * side).expect("inside the border")]
        };
        let (mut heights, mut slopes, mut materials) = (Vec::new(), Vec::new(), Vec::new());
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let slope = Vec2::new(
                    (at(x + 1, z) - at(x - 1, z)) / 2.0,
                    (at(x, z + 1) - at(x, z - 1)) / 2.0,
                )
                .length();
                heights.push(at(x, z));
                slopes.push(slope);
                materials.push(landscape.material_at(meters(origin + IVec2::new(x, z)), slope));
            }
        }
        let lowest = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let highest = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        Self {
            origin,
            heights,
            slopes,
            materials,
            lowest,
            highest,
        }
    }

    fn chunk(&self, position: ChunkPos) -> Chunk {
        let origin = position.origin();
        debug_assert_eq!((origin.x, origin.z), (self.origin.x, self.origin.y));
        let (bottom, top) = (meter(origin.y), meter(origin.y + CHUNK_SIZE - 1));
        // Far enough above or below every surface in the column, every
        // sample saturates the same way.
        let steepest = self.slopes.iter().copied().fold(0.0, f32::max);
        let reach = Voxel::MAX_DISTANCE * (1.0 + steepest * steepest).sqrt() + 1.0;
        if bottom > self.highest + reach {
            return Chunk::uniform(Voxel::AIR);
        }
        if top < self.lowest - reach.max(SOIL_DEPTH) {
            return Chunk::uniform(Voxel::new(-Voxel::MAX_DISTANCE, Material::Stone));
        }
        Chunk::from_fn(|local| {
            let index = cell_index(local.x, local.z);
            let (height, slope) = (self.heights[index], self.slopes[index]);
            let y = meter(origin.y + local.y);
            let depth = height - y;
            // Vertical distance overstates the true distance on slopes;
            // scaling by the slope keeps it a fair estimate.
            let distance = -depth / (1.0 + slope * slope).sqrt();
            if distance >= Voxel::MAX_DISTANCE {
                return Voxel::AIR;
            }
            let material = if depth < TOPSOIL_DEPTH {
                self.materials[index]
            } else if depth < SOIL_DEPTH && self.materials[index] != Material::Stone {
                Material::Soil
            } else {
                Material::Stone
            };
            Voxel::new(distance, material)
        })
    }
}

/// World coordinates of voxel `(x, z)`, in meters.
fn meters(voxel: IVec2) -> Vec2 {
    Vec2::new(meter(voxel.x), meter(voxel.y))
}

/// A voxel coordinate, in meters.
fn meter(voxel: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "world coordinates are far below 2²⁴"
    )]
    let meters = voxel as f32;
    meters
}

/// Index of the column of voxels at local `(x, z)` in a chunk column's
/// samples.
fn cell_index(x: i32, z: i32) -> usize {
    usize::try_from(x + z * CHUNK_SIZE).expect("local coordinates are not negative")
}

#[cfg(test)]
mod tests {
    use glam::Vec3Swizzles;
    use messoria_voxel::ChunkMap;

    use super::*;

    fn map_around(landscape: &Landscape, center: Vec2, radius: i32) -> ChunkMap {
        let mut map = ChunkMap::default();
        let middle = column_of(center);
        for z in -radius..=radius {
            for x in -radius..=radius {
                for (position, chunk) in landscape.column(middle + IVec2::new(x, z)) {
                    map.insert(position, chunk);
                }
            }
        }
        map
    }

    fn surface(map: &ChunkMap, point: Vec2) -> f32 {
        map.surface_below(point.extend(TOP - 1.5).xzy(), TOP - BOTTOM - 3.0)
            .expect("the ground is generated")
    }

    #[test]
    fn the_voxels_follow_the_heightfield() {
        let landscape = Landscape::new(7);
        let map = map_around(&landscape, Vec2::ZERO, 1);
        for point in [Vec2::ZERO, Vec2::new(10.5, -7.25), Vec2::new(-20.0, 15.0)] {
            let ground = surface(&map, point);
            assert!(
                (ground - landscape.height(point)).abs() < 0.1,
                "ground at {point} is {ground}, not {}",
                landscape.height(point)
            );
        }
    }

    #[test]
    fn the_village_stands_on_level_paved_ground() {
        let landscape = Landscape::new(7);
        let middle = landscape.height(VILLAGE_CENTER);
        assert!((middle / LEVEL_STEP - (middle / LEVEL_STEP).round()).abs() < 1e-4);
        for offset in [
            Vec2::new(10.0, 0.0),
            Vec2::new(-7.0, 9.0),
            Vec2::new(0.0, -13.0),
        ] {
            let height = landscape.height(VILLAGE_CENTER + offset);
            assert!((height - middle).abs() < 0.05, "{offset} is not level");
        }
        assert_eq!(landscape.surface(VILLAGE_CENTER), Material::Stone);
    }

    #[test]
    fn the_whole_world_stays_between_its_floor_and_its_sky() {
        let landscape = Landscape::new(7);
        let mut step = -HALF_WIDTH;
        while step < HALF_WIDTH {
            for other in [-HALF_WIDTH + 1.0, -300.0, 0.0, 450.0, HALF_WIDTH - 1.0] {
                for point in [Vec2::new(step, other), Vec2::new(other, step)] {
                    let height = landscape.height(point);
                    assert!(editable(height - 1.0), "{height} at {point}");
                    assert!(height < PEAK_HEIGHT + 0.01);
                }
            }
            step += 16.0;
        }
    }

    #[test]
    fn mountains_close_the_world_and_the_farmland_is_gentle() {
        let landscape = Landscape::new(7);
        let edge = landscape.height(Vec2::new(HALF_WIDTH - 10.0, 0.0));
        let farm = landscape.height(Vec2::new(120.0, 80.0));
        assert!(
            edge > farm + 15.0,
            "the edge at {edge}, the farmland at {farm}"
        );
        let mut steep = 0;
        for x in -10..10 {
            for z in -10..10 {
                #[expect(clippy::cast_precision_loss, reason = "a small grid")]
                let point = VILLAGE_CENTER + Vec2::new(x as f32, z as f32) * 20.0;
                if landscape.slope(point) > 0.35 {
                    steep += 1;
                }
            }
        }
        assert!(steep < 10, "{steep} steep spots in the farmland");
    }

    #[test]
    fn the_river_flows_below_its_banks_and_roads_are_dirt() {
        let landscape = Landscape::new(7);
        let point = landscape
            .river()
            .nth(256)
            .expect("the middle of the course");
        let at = point.position;
        assert_eq!(landscape.water_level(at), Some(point.level));
        assert!(landscape.height(at) < point.level - 0.5);
        assert_eq!(landscape.surface(at), Material::Sand);
        let beside = at + Vec2::X * 40.0;
        assert_eq!(landscape.water_level(beside), None);
        let road = VILLAGE_CENTER + Vec2::new(0.0, 60.0);
        assert_eq!(landscape.surface(road), Material::Soil);
    }

    #[test]
    fn generation_is_deterministic_and_depends_on_the_seed() {
        let (a, b, c) = (Landscape::new(7), Landscape::new(7), Landscape::new(8));
        let column = IVec2::new(3, -2);
        assert_eq!(a.column(column), b.column(column));
        assert_ne!(a.column(column), c.column(column));
    }
}
