//! Why content failed to load.

use std::{io, path::PathBuf};

use messoria_voxel::Material;
use thiserror::Error;

/// A content file that could not be loaded, and why.
#[derive(Debug, Error)]
#[error("{}: {problem}", file.display())]
pub struct ContentError {
    pub file: PathBuf,
    pub problem: Problem,
}

#[derive(Debug, Error)]
pub enum Problem {
    #[error("cannot be read: {0}")]
    Unreadable(#[from] io::Error),
    #[error("{0}")]
    Syntax(Box<ron::error::SpannedError>),
    #[error("item `{0}` is defined more than once")]
    DuplicateItem(String),
    #[error("{context} refers to unknown item `{item}`")]
    UnknownItem { context: String, item: String },
    #[error("item `{0}` has a stack size of 0")]
    EmptyStack(String),
    #[error("item `{0}` is a tool, and tools do not stack")]
    StackedTool(String),
    #[error("item `{0}` has a shelf life but does not say what it spoils into")]
    SpoilsIntoNothing(String),
    #[error("item `{0}` spoils into itself")]
    SpoilsIntoItself(String),
    #[error("no item is dug from {0:?}")]
    UndiggableMaterial(Material),
    #[error("{material:?} is dug as both `{first}` and `{second}`")]
    AmbiguousMaterial {
        material: Material,
        first: String,
        second: String,
    },
    #[error("the starting inventory lists `{0}` with a count of 0")]
    EmptyStartingStack(String),
}

impl From<ron::error::SpannedError> for Problem {
    fn from(error: ron::error::SpannedError) -> Self {
        Self::Syntax(Box::new(error))
    }
}
