//! The village: a fixed area of the valley, a walk away from where players
//! arrive, where the shops stand. Its ground is protected; nobody can dig,
//! raise or till it.

use bevy::math::{Vec2, Vec3, Vec3Swizzles};

/// Middle of the village square, on the ground plane.
pub const CENTER: Vec2 = Vec2::new(0.0, -30.0);
/// Radius of the protected ground around the center, in meters.
pub const RADIUS: f32 = 14.0;

/// Whether a disc of `radius` around `point` reaches onto the village's
/// ground.
pub fn reaches(point: Vec3, radius: f32) -> bool {
    point.xz().distance(CENTER) < RADIUS + radius
}
