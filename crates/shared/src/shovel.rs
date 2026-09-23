//! Rules of the shovel, shared so clients aim exactly where the server allows.

use std::time::Duration;

use bevy::math::Vec3;
use messoria_voxel::{Brush, BrushMode, Material};

use crate::protocol::ItemAction;

/// How far from a character's eyes the shovel reaches, in meters.
pub const REACH: f32 = 4.5;
/// Radius of the sphere of ground dug or raised per use, in meters.
pub const BRUSH_RADIUS: f32 = 1.3;
/// Minimum time between two uses by the same player.
pub const COOLDOWN: Duration = Duration::from_millis(250);
/// Energy each use costs.
pub const ENERGY_COST: u16 = 2;

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
pub fn brush(target: Vec3, action: ShovelAction) -> Brush {
    Brush {
        center: target,
        radius: BRUSH_RADIUS,
        mode: match action {
            ShovelAction::Dig => BrushMode::Dig,
            ShovelAction::Raise => BrushMode::Raise(RAISED_MATERIAL),
        },
    }
}

/// Whether the shovel can reach `target` from eyes at `eyes`.
pub fn in_reach(eyes: Vec3, target: Vec3) -> bool {
    eyes.distance(target) <= REACH
}
