//! Why a save could not be read or written.

use std::{io, path::PathBuf};

use messoria_voxel::DecodeError;
use thiserror::Error;

/// A save file that could not be read or written, and why.
#[derive(Debug, Error)]
#[error("{}: {problem}", file.display())]
pub struct SaveError {
    pub file: PathBuf,
    pub problem: Problem,
}

#[derive(Debug, Error)]
pub enum Problem {
    #[error("cannot be read or written: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Syntax(Box<ron::error::SpannedError>),
    #[error("cannot be written: {0}")]
    Format(#[from] ron::Error),
    #[error(
        "was saved by a newer version of the game (format {found}; this version reads up to {supported})"
    )]
    Newer { found: u32, supported: u32 },
    #[error("has format version {0}, which this version of the game cannot read")]
    UnknownVersion(u32),
    #[error(
        "was saved in a valley this version of the game no longer grows (format {0}); start a new world"
    )]
    OlderValley(u32),
    #[error("is not a terrain file")]
    NotTerrain,
    #[error("is truncated")]
    Truncated,
    #[error("holds a chunk that cannot be decoded: {0}")]
    Chunk(#[from] DecodeError),
    #[error("holds {0}, which is out of range")]
    OutOfRange(String),
}

impl From<ron::error::SpannedError> for Problem {
    fn from(error: ron::error::SpannedError) -> Self {
        Self::Syntax(Box::new(error))
    }
}
