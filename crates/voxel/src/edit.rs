//! Reshaping terrain.

use std::collections::BTreeMap;

use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    coords::{ChunkPos, local_index, local_position, split},
    map::ChunkMap,
    voxel::{Material, Voxel},
};

/// How far outside a brush's sphere distances are updated. Only samples next
/// to the surface shape it, so refreshing a thin shell is enough and keeps
/// edits small on the wire.
const INFLUENCE_MARGIN: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrushMode {
    /// Carves a spherical hole.
    Dig,
    /// Adds a sphere of the given material.
    Raise(Material),
}

/// A spherical edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    pub center: Vec3,
    pub radius: f32,
    pub mode: BrushMode,
}

impl Brush {
    /// The voxel at `position` after applying the brush to `voxel`.
    fn apply(&self, position: IVec3, voxel: Voxel) -> Voxel {
        // Signed distance to the brush sphere, negative inside it.
        let sphere = position.as_vec3().distance(self.center) - self.radius;
        match self.mode {
            BrushMode::Dig => Voxel::new(voxel.distance().max(-sphere), voxel.material()),
            BrushMode::Raise(material) if sphere < voxel.distance() => Voxel::new(sphere, material),
            BrushMode::Raise(_) => voxel,
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
    /// Applies `brush` to every loaded chunk it touches and returns what
    /// changed, ordered by chunk.
    pub fn apply_brush(&mut self, brush: &Brush) -> Vec<ChunkChanges> {
        let reach = Vec3::splat(brush.radius + INFLUENCE_MARGIN);
        let min = (brush.center - reach).floor().as_ivec3();
        let max = (brush.center + reach).ceil().as_ivec3();

        let mut changes: BTreeMap<ChunkPos, Vec<(u16, Voxel)>> = BTreeMap::new();
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let position = IVec3::new(x, y, z);
                    let (chunk_pos, local) = split(position);
                    let Some(chunk) = self.get_mut(chunk_pos) else {
                        continue;
                    };
                    let index = local_index(local);
                    let voxel = brush.apply(position, chunk.get_index(index));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::flat_world;

    fn chunks_around_origin() -> impl Iterator<Item = IVec3> {
        (-1..=0)
            .flat_map(|x| (-1..=0).flat_map(move |y| (-1..=0).map(move |z| IVec3::new(x, y, z))))
    }

    fn dig_at(center: Vec3) -> Brush {
        Brush {
            center,
            radius: 1.5,
            mode: BrushMode::Dig,
        }
    }

    #[test]
    fn digging_carves_a_hole() {
        let mut map = flat_world(0.0, chunks_around_origin());
        map.apply_brush(&dig_at(Vec3::new(-5.0, 0.0, -5.0)));

        assert!(!map.voxel(IVec3::new(-5, -1, -5)).unwrap().is_solid());
        assert!(map.voxel(IVec3::new(-5, -3, -5)).unwrap().is_solid());
        assert!(map.voxel(IVec3::new(-9, -1, -5)).unwrap().is_solid());
    }

    #[test]
    fn raising_adds_material() {
        let mut map = flat_world(0.0, chunks_around_origin());
        map.apply_brush(&Brush {
            center: Vec3::new(-5.0, 0.0, -5.0),
            radius: 1.5,
            mode: BrushMode::Raise(Material::Soil),
        });

        let raised = map.voxel(IVec3::new(-5, 1, -5)).unwrap();
        assert!(raised.is_solid());
        assert_eq!(raised.material(), Material::Soil);
        assert!(!map.voxel(IVec3::new(-5, 3, -5)).unwrap().is_solid());
    }

    #[test]
    fn edits_across_chunk_borders_touch_every_chunk() {
        let mut map = flat_world(0.0, chunks_around_origin());
        let changes = map.apply_brush(&dig_at(Vec3::ZERO));

        let chunks: Vec<_> = changes.iter().map(|changes| changes.chunk).collect();
        assert_eq!(chunks.len(), 8, "changed chunks: {chunks:?}");
    }

    #[test]
    fn replaying_changes_reproduces_the_edit() {
        let mut authority = flat_world(0.0, chunks_around_origin());
        let mut replica = authority.clone();

        for changes in authority.apply_brush(&dig_at(Vec3::new(0.3, -0.2, -0.7))) {
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
