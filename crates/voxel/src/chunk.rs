//! Storage for one chunk of voxels.

use glam::IVec3;
use thiserror::Error;

use crate::{
    coords::{CHUNK_VOLUME, local_index, local_position},
    voxel::Voxel,
};

/// A cube of `CHUNK_SIZE`³ voxels.
///
/// Chunks that are entirely air or entirely rock, which is most of them, are
/// stored as a single voxel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk(Storage);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Storage {
    Uniform(Voxel),
    Dense(Box<[Voxel]>),
}

/// Why a chunk could not be decoded.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("chunk data is empty")]
    Empty,
    #[error("unknown chunk format {0}")]
    UnknownFormat(u8),
    #[error("chunk data is truncated or corrupt")]
    Corrupt,
}

const FORMAT_UNIFORM: u8 = 0;
const FORMAT_DENSE: u8 = 1;

impl Chunk {
    /// A chunk filled with `voxel`.
    pub fn uniform(voxel: Voxel) -> Self {
        Self(Storage::Uniform(voxel))
    }

    /// A chunk whose voxel at each local position is `voxel_at(local)`.
    pub fn from_fn(mut voxel_at: impl FnMut(IVec3) -> Voxel) -> Self {
        let voxels: Box<[Voxel]> = (0..CHUNK_VOLUME)
            .map(|index| voxel_at(local_position(index)))
            .collect();
        Self::from_voxels(voxels)
    }

    fn from_voxels(voxels: Box<[Voxel]>) -> Self {
        debug_assert_eq!(voxels.len(), CHUNK_VOLUME);
        let first = voxels[0];
        if voxels.iter().all(|&voxel| voxel == first) {
            Self::uniform(first)
        } else {
            Self(Storage::Dense(voxels))
        }
    }

    /// The voxel at a local position.
    pub fn get(&self, local: IVec3) -> Voxel {
        self.get_index(local_index(local))
    }

    /// Replaces the voxel at a local position, returning whether it changed.
    pub fn set(&mut self, local: IVec3, voxel: Voxel) -> bool {
        self.set_index(local_index(local), voxel)
    }

    /// The voxel filling the whole chunk, if it is uniform.
    pub fn uniform_voxel(&self) -> Option<Voxel> {
        match self.0 {
            Storage::Uniform(voxel) => Some(voxel),
            Storage::Dense(_) => None,
        }
    }

    pub(crate) fn get_index(&self, index: usize) -> Voxel {
        match &self.0 {
            Storage::Uniform(voxel) => *voxel,
            Storage::Dense(voxels) => voxels[index],
        }
    }

    pub(crate) fn set_index(&mut self, index: usize, voxel: Voxel) -> bool {
        match &mut self.0 {
            Storage::Uniform(current) if *current == voxel => false,
            Storage::Uniform(current) => {
                let mut voxels = vec![*current; CHUNK_VOLUME].into_boxed_slice();
                voxels[index] = voxel;
                self.0 = Storage::Dense(voxels);
                true
            }
            Storage::Dense(voxels) => {
                let changed = voxels[index] != voxel;
                voxels[index] = voxel;
                changed
            }
        }
    }

    /// Serializes the chunk compactly for storage or transmission.
    pub fn encode(&self) -> Vec<u8> {
        match &self.0 {
            Storage::Uniform(voxel) => {
                let [distance, material] = voxel.to_bytes();
                vec![FORMAT_UNIFORM, distance, material]
            }
            Storage::Dense(voxels) => {
                // Distances and materials are stored in separate planes because
                // materials form long runs that compress far better on their own.
                let mut planes = vec![0; CHUNK_VOLUME * 2];
                let (distances, materials) = planes.split_at_mut(CHUNK_VOLUME);
                for (index, voxel) in voxels.iter().enumerate() {
                    [distances[index], materials[index]] = voxel.to_bytes();
                }
                let mut encoded = vec![FORMAT_DENSE];
                encoded.extend(lz4_flex::compress_prepend_size(&planes));
                encoded
            }
        }
    }

    /// Reverses [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Fails if `bytes` was not produced by [`Self::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (&format, payload) = bytes.split_first().ok_or(DecodeError::Empty)?;
        match format {
            FORMAT_UNIFORM => match *payload {
                [distance, material] => Voxel::from_bytes(distance, material)
                    .map(Self::uniform)
                    .ok_or(DecodeError::Corrupt),
                _ => Err(DecodeError::Corrupt),
            },
            FORMAT_DENSE => {
                let planes = lz4_flex::decompress_size_prepended(payload)
                    .map_err(|_| DecodeError::Corrupt)?;
                if planes.len() != CHUNK_VOLUME * 2 {
                    return Err(DecodeError::Corrupt);
                }
                let (distances, materials) = planes.split_at(CHUNK_VOLUME);
                let voxels = distances
                    .iter()
                    .zip(materials)
                    .map(|(&distance, &material)| Voxel::from_bytes(distance, material))
                    .collect::<Option<Box<[Voxel]>>>()
                    .ok_or(DecodeError::Corrupt)?;
                Ok(Self::from_voxels(voxels))
            }
            other => Err(DecodeError::UnknownFormat(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::Material;

    fn layered() -> Chunk {
        Chunk::from_fn(|local| Voxel::new(local.as_vec3().y - 10.3, Material::Soil))
    }

    #[test]
    fn identical_voxels_collapse_to_uniform() {
        let chunk = Chunk::from_fn(|_| Voxel::AIR);
        assert_eq!(chunk.uniform_voxel(), Some(Voxel::AIR));
        assert_eq!(layered().uniform_voxel(), None);
    }

    #[test]
    fn writing_a_different_voxel_densifies() {
        let mut chunk = Chunk::uniform(Voxel::AIR);
        let rock = Voxel::new(-1.0, Material::Stone);

        assert!(!chunk.set(IVec3::ONE, Voxel::AIR));
        assert!(chunk.set(IVec3::ONE, rock));
        assert_eq!(chunk.get(IVec3::ONE), rock);
        assert_eq!(chunk.get(IVec3::ZERO), Voxel::AIR);
    }

    #[test]
    fn encoding_round_trips() {
        for chunk in [Chunk::uniform(Voxel::AIR), layered()] {
            assert_eq!(Chunk::decode(&chunk.encode()), Ok(chunk));
        }
    }

    #[test]
    fn layered_terrain_compresses_well() {
        assert!(layered().encode().len() < 1024);
    }

    #[test]
    fn corrupt_data_is_rejected() {
        assert_eq!(Chunk::decode(&[]), Err(DecodeError::Empty));
        assert_eq!(Chunk::decode(&[9]), Err(DecodeError::UnknownFormat(9)));
        assert_eq!(
            Chunk::decode(&[FORMAT_DENSE, 1, 2, 3]),
            Err(DecodeError::Corrupt)
        );
        assert_eq!(
            Chunk::decode(&[FORMAT_UNIFORM, 0, 99]),
            Err(DecodeError::Corrupt)
        );
    }
}
