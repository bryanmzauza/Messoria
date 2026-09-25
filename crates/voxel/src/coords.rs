//! Chunk and voxel coordinates.
//!
//! Voxel coordinates are world positions in meters, as integers. Local
//! coordinates address a voxel inside its chunk and range over
//! `0..CHUNK_SIZE` on each axis.

use std::cmp::Ordering;

use glam::IVec3;
use serde::{Deserialize, Serialize};

/// Voxels along each edge of a chunk.
pub const CHUNK_SIZE: i32 = 32;

/// Voxels in one chunk.
pub(crate) const CHUNK_VOLUME: usize = 32 * 32 * 32;

/// Position of a chunk in chunk units: chunk `(1, 0, 0)` starts at voxel `(32, 0, 0)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkPos(pub IVec3);

/// Lexicographic by x, then y, then z. Gives edits a stable order.
impl Ord for ChunkPos {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.to_array().cmp(&other.0.to_array())
    }
}

impl PartialOrd for ChunkPos {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl ChunkPos {
    /// The chunk that contains `voxel`.
    pub fn containing(voxel: IVec3) -> Self {
        Self(voxel.div_euclid(IVec3::splat(CHUNK_SIZE)))
    }

    /// Voxel coordinates of the chunk's minimum corner.
    pub fn origin(self) -> IVec3 {
        self.0 * CHUNK_SIZE
    }

    /// This chunk and the 26 chunks around it.
    pub fn neighborhood(self) -> impl Iterator<Item = ChunkPos> {
        (-1..=1).flat_map(move |z| {
            (-1..=1)
                .flat_map(move |y| (-1..=1).map(move |x| ChunkPos(self.0 + IVec3::new(x, y, z))))
        })
    }

    /// Chunks whose meshes depend on the voxel at `local` in this chunk: this
    /// chunk, plus the neighbors whose one-voxel border reaches into it.
    pub fn chunks_sampling(self, local: IVec3) -> impl Iterator<Item = ChunkPos> {
        let span =
            |coordinate: i32| -i32::from(coordinate == 0)..=i32::from(coordinate == CHUNK_SIZE - 1);
        span(local.x).flat_map(move |dx| {
            span(local.y).flat_map(move |dy| {
                span(local.z).map(move |dz| ChunkPos(self.0 + IVec3::new(dx, dy, dz)))
            })
        })
    }
}

/// Splits voxel coordinates into the containing chunk and the local position within it.
pub(crate) fn split(voxel: IVec3) -> (ChunkPos, IVec3) {
    (
        ChunkPos::containing(voxel),
        voxel.rem_euclid(IVec3::splat(CHUNK_SIZE)),
    )
}

/// Index of a local position in chunk storage. Horizontal layers are
/// contiguous, which keeps runs of air and rock together.
pub(crate) fn local_index(local: IVec3) -> usize {
    debug_assert!(
        local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE)).all(),
        "{local} is outside a chunk"
    );
    #[expect(
        clippy::cast_sign_loss,
        reason = "local coordinates are never negative"
    )]
    let index = (local.x + local.z * CHUNK_SIZE + local.y * CHUNK_SIZE * CHUNK_SIZE) as usize;
    index
}

/// Inverse of [`local_index`].
pub(crate) fn local_position(index: usize) -> IVec3 {
    let size = CHUNK_SIZE as usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "index < CHUNK_VOLUME"
    )]
    let coordinate = |value: usize| value as i32;
    IVec3::new(
        coordinate(index % size),
        coordinate(index / (size * size)),
        coordinate(index / size % size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_voxels_belong_to_negative_chunks() {
        assert_eq!(
            ChunkPos::containing(IVec3::new(-1, 0, 31)),
            ChunkPos(IVec3::new(-1, 0, 0))
        );
        assert_eq!(
            split(IVec3::new(-1, 32, 0)),
            (ChunkPos(IVec3::new(-1, 1, 0)), IVec3::new(31, 0, 0))
        );
    }

    #[test]
    fn local_index_round_trips() {
        for index in [0, 1, 31, 32, 1023, 1024, CHUNK_VOLUME - 1] {
            assert_eq!(local_index(local_position(index)), index);
        }
    }

    #[test]
    fn interior_voxels_affect_only_their_chunk() {
        let chunk = ChunkPos(IVec3::ZERO);
        assert_eq!(
            chunk.chunks_sampling(IVec3::splat(5)).collect::<Vec<_>>(),
            [chunk]
        );
    }

    #[test]
    fn corner_voxels_affect_all_chunks_meeting_there() {
        let chunk = ChunkPos(IVec3::ZERO);
        assert_eq!(chunk.chunks_sampling(IVec3::ZERO).count(), 8);
        assert_eq!(chunk.chunks_sampling(IVec3::new(31, 5, 0)).count(), 4);
    }
}
