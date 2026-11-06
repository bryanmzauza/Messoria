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
mod palette;
mod people;
mod scenery;
mod shop;
mod structure;
mod village;
mod villager;

pub use crate::{
    catalog::{Catalog, Sources},
    crop::{CropDef, CropId},
    error::{ContentError, Problem},
    item::{ItemDef, ItemId, ItemKind, Quality, Tool},
    palette::{Palette, Rgb},
    people::{Characters, Cube, Expressions, Grip, Limb, Limbs, Look, Wardrobe},
    scenery::{CoverDef, Gathering, PropDef, PropId, Remains},
    shop::{Listing, MarketRules, Offer, ShopDef, ShopId},
    structure::{Part, Placement, Purpose, Solid, StructureDef, StructureId},
    village::{Lots, Paths, Spot, SpotPose, Stall, Village},
    villager::{
        Activity, Gift, GiftLines, HEART, Line, MOST_HEARTS, Reaction, Slot, VillagerDef,
        VillagerId, hearts,
    },
};
