//! The village: a fixed area of the valley, a walk away from where players
//! arrive, where the shops stand. Its ground is protected; nobody can dig,
//! raise or till it.

use bevy::math::{Vec2, Vec3, Vec3Swizzles};

/// Middle of the village square, on the ground plane.
pub const CENTER: Vec2 = messoria_worldgen::VILLAGE_CENTER;
/// Radius of the protected ground around the center, in meters.
pub const RADIUS: f32 = messoria_worldgen::VILLAGE_RADIUS;

/// Whether a disc of `radius` around `point` reaches onto the village's
/// ground.
pub fn reaches(point: Vec3, radius: f32) -> bool {
    point.xz().distance(CENTER) < RADIUS + radius
}
