//! Fields: squares of tilled soil, one meter on a side, aligned with the
//! world grid, where crops are planted.

use bevy::math::{IVec2, Vec2, Vec3, Vec3Swizzles};
use messoria_voxel::{ChunkMap, Material};

/// Least upward component a surface normal may have for its ground to be
/// tilled; steeper slopes cannot be farmed.
const TILLABLE_FLATNESS: f32 = 0.85;
/// How far above and below a targeted point to look for a field's ground.
const GROUND_SEARCH: f32 = 1.5;

/// The field square containing `point`.
pub fn tile_at(point: Vec3) -> IVec2 {
    point.xz().floor().as_ivec2()
}

/// The middle of a field square, on the ground plane.
pub fn tile_center(tile: IVec2) -> Vec2 {
    tile.as_vec2() + Vec2::splat(0.5)
}

/// Height of the ground in the middle of `tile`, near the height `around`,
/// if it can be tilled: soft ground that is not too steep.
pub fn tillable_ground(terrain: &ChunkMap, tile: IVec2, around: f32) -> Option<f32> {
    let center = tile_center(tile);
    let height = terrain.surface_below(
        Vec3::new(center.x, around + GROUND_SEARCH, center.y),
        2.0 * GROUND_SEARCH,
    )?;
    let ground = Vec3::new(center.x, height, center.y);
    let soft = matches!(
        terrain.surface_material(ground)?,
        Material::Grass | Material::Soil
    );
    let flat = terrain.normal(ground)?.y >= TILLABLE_FLATNESS;
    (soft && flat).then_some(height)
}
