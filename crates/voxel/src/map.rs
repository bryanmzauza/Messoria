//! The loaded part of the world.

use std::collections::HashMap;

use glam::{IVec3, Vec3};

use crate::{
    chunk::Chunk,
    coords::{ChunkPos, split},
    voxel::Voxel,
};

/// The chunks of the world that are loaded, keyed by position.
///
/// Queries that touch a chunk that is not loaded return `None` rather than
/// guessing, so callers can tell "air" apart from "unknown".
#[derive(Clone, Debug, Default)]
pub struct ChunkMap {
    chunks: HashMap<ChunkPos, Chunk>,
}

impl ChunkMap {
    pub fn insert(&mut self, position: ChunkPos, chunk: Chunk) -> Option<Chunk> {
        self.chunks.insert(position, chunk)
    }

    pub fn remove(&mut self, position: ChunkPos) -> Option<Chunk> {
        self.chunks.remove(&position)
    }

    pub fn get(&self, position: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&position)
    }

    pub(crate) fn get_mut(&mut self, position: ChunkPos) -> Option<&mut Chunk> {
        self.chunks.get_mut(&position)
    }

    pub fn contains(&self, position: ChunkPos) -> bool {
        self.chunks.contains_key(&position)
    }

    /// Positions of all loaded chunks, in no particular order.
    pub fn positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// The voxel at voxel coordinates `position`, if its chunk is loaded.
    pub fn voxel(&self, position: IVec3) -> Option<Voxel> {
        let (chunk, local) = split(position);
        self.chunks.get(&chunk).map(|chunk| chunk.get(local))
    }

    /// Signed distance from `point` to the terrain surface, negative inside
    /// solid ground, interpolated between the eight surrounding samples.
    pub fn distance(&self, point: Vec3) -> Option<f32> {
        let base = point.floor();
        let fraction = point - base;
        let base = base.as_ivec3();
        let sample = |offset: IVec3| self.voxel(base + offset).map(Voxel::distance);

        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let mut along_x = [[0.0; 2]; 2];
        for (y, row) in along_x.iter_mut().enumerate() {
            for (z, value) in row.iter_mut().enumerate() {
                let offset = IVec3::new(0, i32::from(y == 1), i32::from(z == 1));
                *value = lerp(sample(offset)?, sample(offset + IVec3::X)?, fraction.x);
            }
        }
        let along_y = [
            lerp(along_x[0][0], along_x[1][0], fraction.y),
            lerp(along_x[0][1], along_x[1][1], fraction.y),
        ];
        Some(lerp(along_y[0], along_y[1], fraction.z))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::voxel::Material;

    /// A world of `chunks` whose ground is flat at `height` meters.
    pub(crate) fn flat_world(height: f32, chunks: impl IntoIterator<Item = IVec3>) -> ChunkMap {
        let mut map = ChunkMap::default();
        for position in chunks {
            let position = ChunkPos(position);
            let origin = position.origin();
            let chunk = Chunk::from_fn(|local| {
                let y = (origin + local).as_vec3().y;
                Voxel::new(y - height, Material::Grass)
            });
            map.insert(position, chunk);
        }
        map
    }

    #[test]
    fn distance_interpolates_between_samples() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        let distance = map.distance(Vec3::new(3.7, 12.5, 8.1)).unwrap();
        assert!((distance - 2.25).abs() < 1e-5, "distance was {distance}");
    }

    #[test]
    fn distance_is_unknown_outside_loaded_chunks() {
        let map = flat_world(10.0, [IVec3::ZERO]);
        assert_eq!(map.distance(Vec3::new(-0.5, 5.0, 5.0)), None);
        assert_eq!(map.distance(Vec3::new(31.5, 5.0, 5.0)), None);
    }
}
