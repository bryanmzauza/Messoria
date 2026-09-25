//! The valley every world grows from its seed: the shape of its ground, the
//! river and roads across it, and where its scenery stands.
//!
//! Everything here is a pure function of the seed, so the server and every
//! client work out the same valley on their own: the server generates
//! terrain only where players are, and clients draw the far land and grow
//! the scenery near them without either being sent over the network.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod landscape;
mod noise;
mod river;
mod roads;
mod scatter;

pub use crate::{
    landscape::{
        BOTTOM, COLUMNS, HALF_WIDTH, LAYERS, Landscape, TOP, VILLAGE_CENTER, VILLAGE_RADIUS,
        column_of, editable, in_world,
    },
    river::{RIVER_HALF_WIDTH, RiverPoint},
    roads::ROAD_HALF_WIDTH,
    scatter::{PlacedProp, PropKey, props_in_column},
};
