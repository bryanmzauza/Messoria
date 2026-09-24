//! Structures on screen: each one drawn from its parts, turned the way it
//! faces, and lit by its lamps.

use bevy::prelude::*;
use messoria_shared::{content::Content, protocol::Structure};

use crate::art::Models;

/// A lamp's warm light, how bright it is, in lumens, and how far it reaches,
/// in meters.
const LAMP_COLOR: Color = Color::srgb(1.0, 0.78, 0.5);
const LAMP_LUMENS: f32 = 30_000.0;
const LAMP_RANGE: f32 = 8.0;

pub(crate) struct StructuresPlugin;

impl Plugin for StructuresPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw_structures);
    }
}

fn draw_structures(
    content: Res<Content>,
    models: Res<Models>,
    built: Query<(Entity, &Structure), Added<Structure>>,
    mut commands: Commands,
) {
    for (entity, structure) in &built {
        let definition = content.structure(structure.kind);
        commands
            .entity(entity)
            .insert((
                Name::new(definition.name.clone()),
                Transform::from_translation(structure.position)
                    .with_rotation(Quat::from_rotation_y(structure.facing)),
                Visibility::default(),
            ))
            .with_children(|parts| {
                for part in &definition.parts {
                    let Some(scene) = models.scene(&part.model) else {
                        continue;
                    };
                    let (x, y, z) = part.at;
                    parts.spawn((
                        scene,
                        Transform::from_xyz(x, y, z)
                            .with_rotation(Quat::from_rotation_y(part.turn))
                            .with_scale(Vec3::splat(part.scale)),
                    ));
                }
                for &(x, y, z) in &definition.lamps {
                    parts.spawn((
                        PointLight {
                            color: LAMP_COLOR,
                            intensity: LAMP_LUMENS,
                            range: LAMP_RANGE,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_xyz(x, y, z),
                    ));
                }
            });
    }
}
