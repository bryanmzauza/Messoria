//! A single terrain sample.

use serde::{Deserialize, Serialize};

/// What solid ground is made of. Only meaningful for solid voxels.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Material {
    Grass = 0,
    Soil = 1,
    Stone = 2,
    Sand = 3,
}

impl Material {
    pub const ALL: [Self; 4] = [Self::Grass, Self::Soil, Self::Stone, Self::Sand];
}

impl TryFrom<u8> for Material {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        match value {
            0 => Ok(Self::Grass),
            1 => Ok(Self::Soil),
            2 => Ok(Self::Stone),
            3 => Ok(Self::Sand),
            other => Err(other),
        }
    }
}

/// Resolution of stored distances.
const STEPS_PER_METER: f32 = 16.0;
const MAX_STEPS: i8 = 127;

/// A terrain sample: the signed distance to the nearest surface, negative
/// inside solid ground, and the material of that ground.
///
/// Distances are quantized to 1/16 m and saturate a little under 8 m, which
/// is far more range than meshing and collision need: only samples next to
/// the surface shape it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Voxel {
    distance: i8,
    material: Material,
}

impl Voxel {
    /// Largest distance a voxel can store, in meters.
    pub const MAX_DISTANCE: f32 = MAX_STEPS as f32 / STEPS_PER_METER;

    /// Open air, far from any surface.
    pub const AIR: Self = Self {
        distance: MAX_STEPS,
        material: Material::Soil,
    };

    /// A voxel `distance` meters from the surface (negative underground).
    /// Distances beyond [`Self::MAX_DISTANCE`] saturate.
    pub fn new(distance: f32, material: Material) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "clamped to the i8 range first"
        )]
        let steps = (distance * STEPS_PER_METER)
            .round()
            .clamp(-f32::from(MAX_STEPS), f32::from(MAX_STEPS)) as i8;
        Self {
            distance: steps,
            material,
        }
    }

    /// Signed distance to the surface in meters, negative underground.
    pub fn distance(self) -> f32 {
        f32::from(self.distance) / STEPS_PER_METER
    }

    pub fn material(self) -> Material {
        self.material
    }

    pub fn is_solid(self) -> bool {
        self.distance < 0
    }

    pub(crate) fn to_bytes(self) -> [u8; 2] {
        [self.distance.to_ne_bytes()[0], self.material as u8]
    }

    pub(crate) fn from_bytes(distance: u8, material: u8) -> Option<Self> {
        Some(Self {
            distance: i8::from_ne_bytes([distance]),
            material: Material::try_from(material).ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_quantized_and_saturate() {
        assert!((Voxel::new(0.3, Material::Soil).distance() - 0.3125).abs() < 1e-6);
        assert!((Voxel::new(-100.0, Material::Soil).distance() + Voxel::MAX_DISTANCE).abs() < 1e-6);
        assert!(!Voxel::new(-0.01, Material::Soil).is_solid());
        assert!(Voxel::new(-0.1, Material::Soil).is_solid());
    }

    #[test]
    fn bytes_round_trip() {
        let voxel = Voxel::new(-2.5, Material::Stone);
        let [distance, material] = voxel.to_bytes();
        assert_eq!(Voxel::from_bytes(distance, material), Some(voxel));
        assert_eq!(Voxel::from_bytes(distance, 200), None);
    }
}
