//! Where the parts of a structure stand in the world, shared so that every
//! peer lays out a structure exactly as the server does.
//!
//! A structure's definition is written in its own frame, front toward -Z;
//! a built structure is that frame moved to its position and turned by its
//! facing.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use messoria_content::{StructureDef, StructureId};

use crate::protocol::Structure;

impl Structure {
    /// Where a point of the structure's frame (x, height, z) is in the
    /// world.
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.position + Quat::from_rotation_y(self.facing) * local
    }

    /// Where a point of the world is in the structure's frame, on the
    /// ground plane.
    pub fn to_local(&self, point: Vec2) -> Vec2 {
        Vec2::from_angle(self.facing).rotate(point - self.position.xz())
    }

    /// Whether a disc of `radius` around `point` reaches the ground of
    /// `definition`, standing as this structure.
    pub fn covers(&self, definition: &StructureDef, point: Vec2, radius: f32) -> bool {
        let half = Vec2::new(definition.size.0, definition.size.1) / 2.0;
        let outside = (self.to_local(point).abs() - half).max(Vec2::ZERO);
        outside.length() < radius || outside == Vec2::ZERO
    }

    /// The structures built along with this one, standing where
    /// `definition` places them.
    pub fn contained<'a>(
        &'a self,
        definition: &'a StructureDef,
    ) -> impl Iterator<Item = Structure> + 'a {
        definition.contains.iter().map(|placement| Structure {
            kind: placement.structure,
            position: self.to_world(Vec3::new(placement.at.0, 0.0, placement.at.1)),
            facing: self.facing + placement.turn,
        })
    }
}

/// The facing a structure gets when someone looking along `yaw` builds it:
/// its front toward them, squared to the world's axes.
pub fn facing_builder(yaw: f32) -> f32 {
    ((yaw + PI) / FRAC_PI_2).round() * FRAC_PI_2
}

/// A structure of `kind` to be built with the middle of its front at
/// `front`, facing whoever builds it while looking along `yaw`: it stands
/// on the far side of where they aim.
pub fn planned(kind: StructureId, definition: &StructureDef, front: Vec3, yaw: f32) -> Structure {
    let facing = facing_builder(yaw);
    let depth = Quat::from_rotation_y(facing) * Vec3::new(0.0, 0.0, definition.size.1 / 2.0);
    Structure {
        kind,
        position: front + depth,
        facing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::load_content;

    #[test]
    fn a_structure_faces_whoever_builds_it() {
        // Looking toward -Z, the front (-Z of the frame) must face +Z.
        let front = Quat::from_rotation_y(facing_builder(0.1)) * Vec3::NEG_Z;
        assert!(front.abs_diff_eq(Vec3::Z, 1e-5), "front faces {front}");
        let front = Quat::from_rotation_y(facing_builder(FRAC_PI_2)) * Vec3::NEG_Z;
        assert!(front.abs_diff_eq(Vec3::X, 1e-5), "front faces {front}");
    }

    #[test]
    fn a_turned_structure_covers_its_turned_ground() {
        let content = load_content().expect("the shipped content is valid");
        let cabin = content.home().expect("players have a home");
        let definition = content.structure(cabin);
        let built = Structure {
            kind: cabin,
            position: Vec3::new(10.0, 3.0, -4.0),
            facing: FRAC_PI_2,
        };
        let (width, depth) = definition.size;
        assert!(built.covers(definition, Vec2::new(10.0, -4.0), 0.0));
        // Turned a quarter, its width runs along the world's z.
        let along_z = Vec2::new(10.0, -4.0 + width / 2.0 - 0.1);
        assert!(built.covers(definition, along_z, 0.0));
        let beyond = Vec2::new(10.0 + depth / 2.0 + 0.5, -4.0);
        assert!(!built.covers(definition, beyond, 0.0));
        assert!(built.covers(definition, beyond, 0.6));

        let inside: Vec<Structure> = built.contained(definition).collect();
        assert!(!inside.is_empty());
        for structure in inside {
            assert!(built.covers(definition, structure.position.xz(), 0.0));
            assert_ne!(structure.kind, cabin);
        }
    }
}
