//! Rules of the shovel, shared so clients aim exactly where the server allows.

use std::time::Duration;

use bevy::math::Vec3;
use messoria_voxel::{Brush, BrushMode, Material};

use crate::protocol::{ShovelAction, ShovelRequest};

/// How far from a character's eyes the shovel reaches, in meters.
pub const REACH: f32 = 4.5;
/// Radius of the sphere of ground dug or raised per use, in meters.
pub const BRUSH_RADIUS: f32 = 1.3;
/// Minimum time between two uses by the same player.
pub const COOLDOWN: Duration = Duration::from_millis(250);
/// Energy each use costs.
pub const ENERGY_COST: u16 = 2;

/// The terrain edit a request asks for.
pub fn brush(request: &ShovelRequest) -> Brush {
    Brush {
        center: request.target,
        radius: BRUSH_RADIUS,
        mode: match request.action {
            ShovelAction::Dig => BrushMode::Dig,
            ShovelAction::Raise => BrushMode::Raise(Material::Soil),
        },
    }
}

/// Whether the shovel can reach `target` from eyes at `eyes`.
pub fn in_reach(eyes: Vec3, target: Vec3) -> bool {
    eyes.distance(target) <= REACH
}
