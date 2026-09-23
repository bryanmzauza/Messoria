//! Why content failed to load.

use std::{io, path::PathBuf};

use messoria_calendar::Season;
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
    #[error("crop `{0}` is defined more than once")]
    DuplicateCrop(String),
    #[error("crop `{crop}` grows from `{item}`, which is not a seed")]
    NotASeed { crop: String, item: String },
    #[error("seed `{seed}` grows both `{first}` and `{second}`")]
    SharedSeed {
        seed: String,
        first: String,
        second: String,
    },
    #[error("seed `{0}` does not grow any crop")]
    UnplantedSeed(String),
    #[error("crop `{crop}` is invalid: {reason}")]
    InvalidCrop { crop: String, reason: String },
    #[error("shop `{0}` is defined more than once")]
    DuplicateShop(String),
    #[error("shop `{shop}` is invalid: {reason}")]
    InvalidShop { shop: String, reason: String },
    #[error("the market rules are invalid: {0}")]
    InvalidMarket(String),
    #[error("prop `{0}` is defined more than once")]
    DuplicateProp(String),
    #[error("prop `{prop}` is invalid: {reason}")]
    InvalidProp { prop: String, reason: String },
    #[error("ground cover {index} is invalid: {reason}")]
    InvalidCover { index: usize, reason: String },
    #[error("model `{0}` is not in the models folder")]
    MissingModel(String),
    #[error("color `{0}` has a channel outside 0 to 1")]
    InvalidColor(String),
    #[error("the palette has no color for `{0}`")]
    UncoloredGround(String),
    #[error("{season:?} changes color `{name}`, which the palette does not define")]
    UnknownColor { season: Season, name: String },
}

impl From<ron::error::SpannedError> for Problem {
    fn from(error: ron::error::SpannedError) -> Self {
        Self::Syntax(Box::new(error))
    }
}
