//! Rules for using tools, shared so clients aim exactly where the server
//! allows.

use std::time::Duration;

use bevy::math::Vec3;
use messoria_voxel::{Brush, BrushMode, Material};

use crate::protocol::ItemAction;

/// How far from a character's eyes tools reach, in meters.
pub const REACH: f32 = 4.5;
/// Minimum time between two item uses by the same player.
pub const USE_INTERVAL: Duration = Duration::from_millis(250);

/// Whether a tool can reach `target` from eyes at `eyes`.
pub fn in_reach(eyes: Vec3, target: Vec3) -> bool {
    eyes.distance(target) <= REACH
}

/// Radius of the disc of ground a shovel lowers or raises, in meters.
pub const BRUSH_RADIUS: f32 = 1.5;
/// Height between the levels the shovel brings ground to, in meters. One
/// use lowers or raises ground by at most this much.
pub const SHOVEL_STEP: f32 = 0.5;
/// Energy each shovel use costs.
pub const SHOVEL_ENERGY: u16 = 2;
/// Energy tilling one field costs.
pub const HOE_ENERGY: u16 = 2;
/// Energy watering one field costs.
pub const WATERING_ENERGY: u16 = 1;

/// What a shovel does with each of an item's actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShovelAction {
    /// Removes ground, which goes into the inventory.
    Dig,
    /// Adds soil taken from the inventory.
    Raise,
}

impl From<ItemAction> for ShovelAction {
    fn from(action: ItemAction) -> Self {
        match action {
            ItemAction::Primary => Self::Dig,
            ItemAction::Secondary => Self::Raise,
        }
    }
}

/// Ground raising builds with.
pub const RAISED_MATERIAL: Material = Material::Soil;

/// The terrain edit a shovel use at `target` makes.
pub fn shovel_brush(target: Vec3, action: ShovelAction) -> Brush {
    Brush {
        center: target,
        radius: BRUSH_RADIUS,
        step: SHOVEL_STEP,
        mode: match action {
            ShovelAction::Dig => BrushMode::Lower,
            ShovelAction::Raise => BrushMode::Raise(RAISED_MATERIAL),
        },
    }
}
