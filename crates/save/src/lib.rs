//! Saved worlds and players, and the local player's profile and settings.
//!
//! A world is saved as a folder:
//!
//! - `world.ron`: the world's seed, clock, market and fields, and the scenery
//!   players gathered.
//! - `terrain.bin`: every chunk changed since the world was generated. The
//!   rest is generated again, identically, when the world is loaded.
//! - `players/<key>.ron`: one file per player who has joined.
//!
//! Every file starts with the version of the format it was written in. A
//! reader recognizes older versions by that number and upgrades them; a file
//! written by a newer version of the game is refused rather than misread.
//!
//! Saves refer to items, crops and shops by their text ids, never by their
//! positions in the catalog, so they survive changes to the content. Whatever
//! a save refers to that the content no longer defines is left out, and
//! reported.
//!
//! Files are written under a temporary name and then renamed over the old
//! ones, so a crash while saving never leaves a file half written.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod error;
mod files;
mod player;
mod profile;
mod save_dir;
mod settings;
mod stacks;
mod terrain;
mod world;

pub use crate::{
    error::{Problem, SaveError},
    player::PlayerState,
    profile::Profile,
    save_dir::{SaveDir, SavedWorld, is_valid_player_key},
    settings::{Graphics, Settings},
    world::{FieldState, GatheredProp, StructureState, WorldState},
};
