//! The terrain file: every chunk changed since the world was generated.
//!
//! The file is binary: a magic tag, the format version and the number of
//! chunks, then each chunk's position and its encoding from
//! `Chunk::encode`, with its length. Integers are little-endian.

use glam::IVec3;
use messoria_voxel::{Chunk, ChunkPos};

use crate::{error::Problem, files::unreadable};

const MAGIC: &[u8; 4] = b"MTRN";
/// Version of the format this game writes.
const VERSION: u32 = 1;

pub(crate) fn encode<'a>(chunks: impl IntoIterator<Item = (ChunkPos, &'a Chunk)>) -> Vec<u8> {
    let chunks: Vec<_> = chunks.into_iter().collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    let count = u32::try_from(chunks.len()).expect("fewer than 2³² chunks");
    bytes.extend_from_slice(&count.to_le_bytes());
    for (position, chunk) in chunks {
        for coordinate in position.0.to_array() {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        let encoded = chunk.encode();
        let length = u32::try_from(encoded.len()).expect("chunks encode to less than 4 GiB");
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<(ChunkPos, Chunk)>, Problem> {
    let mut reader = Reader(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(Problem::NotTerrain);
    }
    let version = reader.u32()?;
    if version != VERSION {
        // Older versions of the format are read and upgraded here.
        return Err(unreadable(version, VERSION));
    }
    let count = reader.u32()?;
    let mut chunks = Vec::new();
    for _ in 0..count {
        let position = ChunkPos(IVec3::new(reader.i32()?, reader.i32()?, reader.i32()?));
        let length = usize::try_from(reader.u32()?).map_err(|_| Problem::Truncated)?;
        chunks.push((position, Chunk::decode(reader.take(length)?)?));
    }
    Ok(chunks)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Problem> {
        if self.0.len() < length {
            return Err(Problem::Truncated);
        }
        let (taken, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(taken)
    }

    fn array(&mut self) -> Result<[u8; 4], Problem> {
        Ok(self.take(4)?.try_into().expect("took exactly four bytes"))
    }

    fn u32(&mut self) -> Result<u32, Problem> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, Problem> {
        Ok(i32::from_le_bytes(self.array()?))
    }
}
