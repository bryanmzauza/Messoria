//! Reshaping terrain.
//!
//! Edits move the ground up or down rather than carving shapes out of it.
//! Within a brush's footprint, the surface of each column of samples shifts
//! vertically, and the samples around the surface shift with it. The ground
//! under a brush is treated as a heightfield: only the topmost surface near
//! the target moves.
//!
//! Edits bring ground to levels shared by the whole world, at every multiple
//! of a brush's step, so edits made at the same level by anyone join into flat
//! ground, and digging along a slope cuts terraces.

use std::collections::BTreeMap;

use glam::{IVec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    coords::{ChunkPos, local_index, local_position, split},
    map::ChunkMap,
    voxel::{Material, Voxel},
};

/// Share of a brush's radius over which it moves the ground all the way.
/// Beyond it, the movement fades smoothly to nothing at the rim.
const FLAT_SHARE: f32 = 0.5;
/// How far above or below a level a target may be and still count as on it,
/// which absorbs the rounding of stored distances.
const LEVEL_TOLERANCE: f32 = 0.05;
/// How much of an edit's full movement ground must get to take on the edit's
/// material. Ground at the edge of an edit, which barely moves, keeps its
/// own, so the change of material follows the change of shape.
const RESURFACE_FALLOFF: f32 = 0.5;
/// Samples rewritten above and below a moved surface, so that every sample
/// shaping it moves along.
const BAND: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrushMode {
    /// Lowers the ground, exposing what lies under its surface.
    Lower,
    /// Raises the ground with the given material.
    Raise(Material),
}

/// An edit that moves the ground within a disc up or down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    /// The point on the surface the edit is aimed at.
    pub center: Vec3,
    /// Radius of the disc of ground that moves, in meters.
    pub radius: f32,
    /// Height between levels, in meters. The edit brings the middle of its
    /// disc to the next level below or above the target.
    pub step: f32,
    pub mode: BrushMode,
}

impl Brush {
    /// How far above and below the target a column's surface is looked for.
    /// Columns whose surface lies further away, such as the top of a cliff,
    /// are left alone.
    pub const SURFACE_SEARCH: f32 = 3.0;

    /// The height the edit brings the middle of its disc to.
    pub fn level(&self) -> f32 {
        let steps = match self.mode {
            BrushMode::Lower => ((self.center.y - LEVEL_TOLERANCE) / self.step).floor(),
            BrushMode::Raise(_) => ((self.center.y + LEVEL_TOLERANCE) / self.step).ceil(),
        };
        steps * self.step
    }

    /// How far the ground at `column` (x and z), whose surface is at height
    /// `surface`, moves up (positive) or down (negative).
    ///
    /// Ground in the middle of the disc moves to the edit's level, by at
    /// most one step; towards the rim it moves only part of the way. Ground
    /// already past the level stays where it is.
    pub fn shift(&self, column: Vec2, surface: f32) -> f32 {
        let towards_level = self.level() - surface;
        let full = match self.mode {
            BrushMode::Lower => towards_level.clamp(-self.step, 0.0),
            BrushMode::Raise(_) => towards_level.clamp(0.0, self.step),
        };
        full * self.falloff(column)
    }

    /// One within the flat middle of the disc, easing to zero at its rim.
    fn falloff(&self, column: Vec2) -> f32 {
        let distance = column.distance(Vec2::new(self.center.x, self.center.z));
        let fade = self.radius * (1.0 - FLAT_SHARE);
        let t = ((self.radius - distance) / fade).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// The material ground that was `moved` at `column` ends up with.
    fn material(&self, column: Vec2, moved: Material) -> Material {
        if self.falloff(column) < RESURFACE_FALLOFF {
            return moved;
        }
        match self.mode {
            BrushMode::Lower => moved.exposed(),
            BrushMode::Raise(material) => material,
        }
    }
}

/// The voxels an edit changed in one chunk, as their new absolute values.
///
/// Absolute values make applying changes idempotent and independent of
/// floating-point behavior, so every peer ends up with identical terrain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkChanges {
    pub chunk: ChunkPos,
    /// Storage index within the chunk, and the voxel's new value.
    pub voxels: Vec<(u16, Voxel)>,
}

impl ChunkChanges {
    /// Chunks whose meshes these changes invalidate, including neighbors that
    /// sample the changed voxels through their borders.
    pub fn affected_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        let mut affected: Vec<_> = self
            .voxels
            .iter()
            .flat_map(|&(index, _)| {
                self.chunk
                    .chunks_sampling(local_position(usize::from(index)))
            })
            .collect();
        affected.sort_unstable();
        affected.dedup();
        affected.into_iter()
    }
}

impl ChunkMap {
    /// Applies `brush` to every loaded column it covers and returns what
    /// changed, ordered by chunk.
    pub fn apply_brush(&mut self, brush: &Brush) -> Vec<ChunkChanges> {
        let reach = Vec2::splat(brush.radius);
        let middle = Vec2::new(brush.center.x, brush.center.z);
        let min = (middle - reach).floor().as_ivec2();
        let max = (middle + reach).ceil().as_ivec2();

        let mut changes: BTreeMap<ChunkPos, Vec<(u16, Voxel)>> = BTreeMap::new();
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                for (position, voxel) in self.moved_column(brush, x, z) {
                    let (chunk_pos, local) = split(position);
                    let Some(chunk) = self.get_mut(chunk_pos) else {
                        continue;
                    };
                    let index = local_index(local);
                    if chunk.set_index(index, voxel) {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "chunk indices are below 2¹⁵"
                        )]
                        let index = index as u16;
                        changes.entry(chunk_pos).or_default().push((index, voxel));
                    }
                }
            }
        }

        changes
            .into_iter()
            .map(|(chunk, voxels)| ChunkChanges { chunk, voxels })
            .collect()
    }

    /// The new voxels of the column at `x`, `z` once `brush` moves its
    /// ground. Empty if its ground does not move, or the column is not fully
    /// loaded around the target.
    fn moved_column(&self, brush: &Brush, x: i32, z: i32) -> Vec<(IVec3, Voxel)> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "brush sizes are a few meters"
        )]
        let (search_bottom, search_top, margin) = (
            (brush.center.y - Brush::SURFACE_SEARCH).floor() as i32,
            (brush.center.y + Brush::SURFACE_SEARCH).ceil() as i32,
            BAND + 2 * brush.step.ceil() as i32 + 1,
        );
        let Some(column) = Column::read(self, x, z, search_bottom - margin, search_top + margin)
        else {
            return Vec::new();
        };
        if column.solid(search_top) {
            return Vec::new();
        }
        let Some(ground) = (search_bottom..search_top).rev().find(|&y| column.solid(y)) else {
            return Vec::new();
        };

        // The surface crosses between the topmost solid sample and the air
        // above it.
        let (below, above) = (column.distance(ground), column.distance(ground + 1));
        #[expect(clippy::cast_precision_loss, reason = "world coordinates are small")]
        let (surface, position) = (
            ground as f32 + below / (below - above),
            Vec2::new(x as f32, z as f32),
        );
        let shift = brush.shift(position, surface);
        if shift == 0.0 {
            return Vec::new();
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "within the column read above"
        )]
        let (lowest, highest) = (
            surface.min(surface + shift).floor() as i32 - BAND,
            surface.max(surface + shift).ceil() as i32 + BAND,
        );
        (lowest..=highest)
            .map(|y| {
                #[expect(clippy::cast_precision_loss, reason = "world coordinates are small")]
                let source = y as f32 - shift;
                let voxel = Voxel::new(
                    column.distance_at(source),
                    brush.material(position, column.material_at(source)),
                );
                (IVec3::new(x, y, z), voxel)
            })
            .collect()
    }

    /// Applies changes produced by [`Self::apply_brush`], typically on another
    /// peer. Returns `false` if the chunk is not loaded here.
    pub fn apply_changes(&mut self, changes: &ChunkChanges) -> bool {
        let Some(chunk) = self.get_mut(changes.chunk) else {
            return false;
        };
        for &(index, voxel) in &changes.voxels {
            chunk.set_index(usize::from(index), voxel);
        }
        true
    }
}

/// The samples of one column, as they were before an edit.
struct Column {
    bottom: i32,
    voxels: Vec<Voxel>,
}

impl Column {
    fn read(map: &ChunkMap, x: i32, z: i32, bottom: i32, top: i32) -> Option<Self> {
        let voxels = (bottom..=top)
            .map(|y| map.voxel(IVec3::new(x, y, z)))
            .collect::<Option<_>>()?;
        Some(Self { bottom, voxels })
    }

    fn voxel(&self, y: i32) -> Voxel {
        let index = usize::try_from(y - self.bottom).expect("above the column's bottom");
        self.voxels[index]
    }

    fn solid(&self, y: i32) -> bool {
        self.voxel(y).is_solid()
    }

    fn distance(&self, y: i32) -> f32 {
        self.voxel(y).distance()
    }

    /// Distance at a height between samples, interpolated linearly.
    fn distance_at(&self, y: f32) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "within the column read above"
        )]
        let below = y.floor() as i32;
        let t = y - y.floor();
        self.distance(below) * (1.0 - t) + self.distance(below + 1) * t
    }

    fn material_at(&self, y: f32) -> Material {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "within the column read above"
        )]
        let nearest = y.round() as i32;
        self.voxel(nearest).material()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::flat_world;

    /// Ground height of the flat test world, on a level.
    const GROUND: f32 = 0.5;
    const STEP: f32 = 0.5;

    fn chunks_around_origin() -> impl Iterator<Item = IVec3> {
        (-1..=0)
            .flat_map(|x| (-1..=0).flat_map(move |y| (-1..=0).map(move |z| IVec3::new(x, y, z))))
    }

    fn brush(x: f32, y: f32, z: f32, mode: BrushMode) -> Brush {
        Brush {
            center: Vec3::new(x, y, z),
            radius: 2.0,
            step: STEP,
            mode,
        }
    }

    fn height(map: &ChunkMap, x: f32, z: f32) -> f32 {
        map.surface_below(Vec3::new(x, 5.0, z), 10.0)
            .expect("ground below")
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.04,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn levels_are_the_next_multiples_of_the_step() {
        let at = |y, mode| brush(0.0, y, 0.0, mode).level();
        assert_near(at(10.3, BrushMode::Lower), 10.0);
        assert_near(at(10.0, BrushMode::Lower), 9.5);
        assert_near(at(10.02, BrushMode::Lower), 9.5);
        assert_near(at(10.3, BrushMode::Raise(Material::Soil)), 10.5);
        assert_near(at(9.98, BrushMode::Raise(Material::Soil)), 10.5);
    }

    #[test]
    fn lowering_moves_the_ground_down_a_level() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.0, GROUND, -8.0, BrushMode::Lower));

        assert_near(height(&map, -8.0, -8.0), GROUND - STEP);
        assert_near(height(&map, -11.0, -8.0), GROUND);
        let rim = height(&map, -9.5, -8.0);
        assert!(GROUND - STEP < rim && rim < GROUND, "rim at {rim}");
    }

    #[test]
    fn lowering_exposes_what_lies_under_grass() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.0, GROUND, -8.0, BrushMode::Lower));

        let dug = Vec3::new(-8.0, GROUND - STEP - 0.25, -8.0);
        assert_eq!(map.surface_material(dug), Some(Material::Soil));
        let untouched = Vec3::new(-12.0, GROUND, -8.0);
        assert_eq!(map.surface_material(untouched), Some(Material::Grass));
    }

    #[test]
    fn ground_at_the_edge_moves_but_keeps_its_material() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.3, GROUND, -8.0, BrushMode::Lower));

        assert!(height(&map, -10.0, -8.0) < GROUND - 0.05);
        let edge = map.voxel(IVec3::new(-10, 0, -8)).unwrap();
        assert!(edge.is_solid());
        assert_eq!(edge.material(), Material::Grass);
    }

    #[test]
    fn lowering_again_digs_a_level_deeper() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.0, GROUND, -8.0, BrushMode::Lower));
        let floor = height(&map, -8.0, -8.0);
        map.apply_brush(&brush(-8.0, floor, -8.0, BrushMode::Lower));

        assert_near(height(&map, -8.0, -8.0), GROUND - 2.0 * STEP);
    }

    /// Digging at the sloping rim of a dig, as a player widening it does,
    /// extends its floor rather than digging deeper.
    #[test]
    fn digging_at_the_rim_widens_the_floor() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.0, GROUND, -8.0, BrushMode::Lower));
        let rim = height(&map, -6.5, -8.0);
        map.apply_brush(&brush(-6.5, rim, -8.0, BrushMode::Lower));

        for x in [-8.0, -7.0, -6.5, -6.0] {
            assert_near(height(&map, x, -8.0), GROUND - STEP);
        }
    }

    #[test]
    fn raising_builds_up_with_its_material() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        map.apply_brush(&brush(-8.0, GROUND, -8.0, BrushMode::Raise(Material::Sand)));

        assert_near(height(&map, -8.0, -8.0), GROUND + STEP);
        let raised = Vec3::new(-8.0, GROUND + STEP - 0.25, -8.0);
        assert_eq!(map.surface_material(raised), Some(Material::Sand));
    }

    #[test]
    fn ground_beyond_the_search_is_left_alone() {
        let mut map = flat_world(GROUND, chunks_around_origin());
        let far_above = GROUND + Brush::SURFACE_SEARCH + 1.0;
        assert!(
            map.apply_brush(&brush(-8.0, far_above, -8.0, BrushMode::Lower))
                .is_empty()
        );
    }

    #[test]
    fn edits_across_chunk_borders_touch_every_chunk() {
        let mut map = flat_world(0.0, chunks_around_origin());
        let changes = map.apply_brush(&brush(0.0, 0.0, 0.0, BrushMode::Lower));

        let chunks: Vec<_> = changes.iter().map(|changes| changes.chunk).collect();
        assert_eq!(chunks.len(), 8, "changed chunks: {chunks:?}");
    }

    #[test]
    fn replaying_changes_reproduces_the_edit() {
        let mut authority = flat_world(0.0, chunks_around_origin());
        let mut replica = authority.clone();

        for changes in authority.apply_brush(&brush(0.3, -0.2, -0.7, BrushMode::Lower)) {
            assert!(replica.apply_changes(&changes));
        }

        for position in authority.positions() {
            assert_eq!(authority.get(position), replica.get(position));
        }
    }

    #[test]
    fn changes_at_a_chunk_border_affect_the_neighbor() {
        let changes = ChunkChanges {
            chunk: ChunkPos(IVec3::ZERO),
            voxels: vec![(0, Voxel::AIR)],
        };
        assert_eq!(changes.affected_chunks().count(), 8);
    }
}
