//! Geometric queries against the terrain surface.

use glam::{IVec3, Vec3};

use crate::{map::ChunkMap, voxel::Material};

/// Distance between samples when marching along a ray or down a column.
/// Small enough not to skip over any feature a one-meter grid can represent.
const MARCH_STEP: f32 = 0.1;
/// Bisection rounds after a crossing is bracketed: 0.1 m / 2⁸ < 1 mm.
const REFINE_ROUNDS: u32 = 8;
/// Offset for estimating the surface normal by central differences.
const NORMAL_EPSILON: f32 = 0.5;

/// Where a ray met the terrain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub point: Vec3,
    /// Unit vector pointing out of the ground.
    pub normal: Vec3,
    /// Distance along the ray to `point`.
    pub distance: f32,
}

impl ChunkMap {
    /// The first point where a ray enters the ground, within `max_distance`.
    ///
    /// Returns `None` if the ray starts underground, reaches terrain that is
    /// not loaded, or hits nothing. `direction` must be normalized.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
        let at = |t: f32| origin + direction * t;
        let crossing = first_crossing(|t| self.distance(at(t)), max_distance)?;
        let point = at(crossing);
        Some(RayHit {
            point,
            normal: self.normal(point)?,
            distance: crossing,
        })
    }

    /// Height of the first surface below `point`, searching at most
    /// `max_depth` meters down. `point` must be above ground.
    pub fn surface_below(&self, point: Vec3, max_depth: f32) -> Option<f32> {
        let at = |depth: f32| point - Vec3::Y * depth;
        let depth = first_crossing(|depth| self.distance(at(depth)), max_depth)?;
        Some(point.y - depth)
    }

    /// The ground material at the surface near `point`: of the eight samples
    /// around it, the solid one closest to the surface. `None` if none of them
    /// is solid, or they are not loaded.
    pub fn surface_material(&self, point: Vec3) -> Option<Material> {
        let base = point.floor().as_ivec3();
        let mut nearest: Option<(f32, Material)> = None;
        for z in 0..=1 {
            for y in 0..=1 {
                for x in 0..=1 {
                    let voxel = self.voxel(base + IVec3::new(x, y, z))?;
                    if voxel.is_solid()
                        && nearest.is_none_or(|(distance, _)| voxel.distance() > distance)
                    {
                        nearest = Some((voxel.distance(), voxel.material()));
                    }
                }
            }
        }
        nearest.map(|(_, material)| material)
    }

    /// Unit vector pointing out of the ground at `point`, if it can be
    /// estimated there.
    pub fn normal(&self, point: Vec3) -> Option<Vec3> {
        let difference = |axis: Vec3| {
            let offset = axis * NORMAL_EPSILON;
            Some(self.distance(point + offset)? - self.distance(point - offset)?)
        };
        Vec3::new(
            difference(Vec3::X)?,
            difference(Vec3::Y)?,
            difference(Vec3::Z)?,
        )
        .try_normalize()
    }
}

/// Smallest `t` in `0..=max_t` where `distance(t)` goes from air to
/// ground, given that `distance(0)` is air.
fn first_crossing(distance: impl Fn(f32) -> Option<f32>, max_t: f32) -> Option<f32> {
    if distance(0.0)? <= 0.0 {
        return None;
    }
    let mut air = 0.0;
    // Stepping by multiplication rather than accumulation keeps samples on
    // the same grid regardless of floating-point drift.
    for step in 1..=u16::MAX {
        let t = (MARCH_STEP * f32::from(step)).min(max_t);
        if distance(t)? <= 0.0 {
            let mut ground = t;
            for _ in 0..REFINE_ROUNDS {
                let middle = f32::midpoint(air, ground);
                if distance(middle)? <= 0.0 {
                    ground = middle;
                } else {
                    air = middle;
                }
            }
            return Some(ground);
        }
        if t >= max_t {
            return None;
        }
        air = t;
    }
    None
}

#[cfg(test)]
mod tests {
    use glam::IVec3;

    use crate::map::tests::flat_world;

    use super::*;

    #[test]
    fn ray_hits_flat_ground() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        let direction = Vec3::new(1.0, -1.0, 0.0).normalize();
        let hit = map
            .raycast(Vec3::new(5.0, 15.0, 5.0), direction, 20.0)
            .unwrap();

        assert!((hit.point.y - 10.25).abs() < 0.01, "hit at {}", hit.point);
        assert!(hit.normal.abs_diff_eq(Vec3::Y, 1e-3));
        assert!((hit.distance - 4.75 * 2.0_f32.sqrt()).abs() < 0.01);
    }

    #[test]
    fn ray_that_falls_short_misses() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        assert_eq!(
            map.raycast(Vec3::new(5.0, 15.0, 5.0), Vec3::NEG_Y, 3.0),
            None
        );
    }

    #[test]
    fn ray_into_unloaded_terrain_misses() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        assert_eq!(
            map.raycast(Vec3::new(5.0, 15.0, 5.0), Vec3::NEG_X, 20.0),
            None
        );
    }

    #[test]
    fn the_surface_material_is_the_topmost_ground() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        assert_eq!(
            map.surface_material(Vec3::new(4.5, 10.25, 7.5)),
            Some(Material::Grass)
        );
        assert_eq!(map.surface_material(Vec3::new(4.5, 20.0, 7.5)), None);
    }

    #[test]
    fn surface_below_finds_the_ground() {
        let map = flat_world(10.25, [IVec3::ZERO]);
        let height = map.surface_below(Vec3::new(4.2, 12.0, 7.9), 5.0).unwrap();
        assert!((height - 10.25).abs() < 0.01, "height was {height}");
        assert_eq!(map.surface_below(Vec3::new(4.2, 9.0, 7.9), 5.0), None);
    }
}
