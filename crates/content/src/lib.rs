//! Game content: what exists in the world, as described by data files.
//!
//! Content is read from RON files in the `data` folder of the game's assets
//! and checked thoroughly when it is loaded, so a mistake in a data file
//! stops the game at startup with an error that names the file and the
//! problem, instead of surfacing later during play.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod catalog;
mod crop;
mod error;
mod item;

pub use crate::{
    catalog::Catalog,
    crop::{CropDef, CropId},
    error::{ContentError, Problem},
    item::{ItemDef, ItemId, ItemKind, Quality, Tool},
};
