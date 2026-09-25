//! Structures on screen: each one drawn from its parts, turned the way it
//! faces, and lit by its lamps.

use bevy::{light::NotShadowCaster, prelude::*};
use messoria_shared::{content::Content, protocol::Structure};

use crate::art::Models;

/// A lamp's warm light, how bright it is, in lumens, and how far it reaches,
/// in meters.
const LAMP_COLOR: Color = Color::srgb(1.0, 0.78, 0.5);
const LAMP_LUMENS: f32 = 30_000.0;
const LAMP_RANGE: f32 = 8.0;
/// The glowing bulb drawn where each lamp hangs, so the light has a source.
const BULB_RADIUS: f32 = 0.11;
const BULB_GLOW: f32 = 12.0;

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
    mut bulb: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let (bulb_mesh, bulb_material) = bulb
        .get_or_insert_with(|| {
            (
                meshes.add(
                    Sphere::new(BULB_RADIUS)
                        .mesh()
                        .ico(1)
                        .expect("a small subdivision"),
                ),
                materials.add(StandardMaterial {
                    base_color: LAMP_COLOR,
                    emissive: LinearRgba::from(LAMP_COLOR) * BULB_GLOW,
                    ..default()
                }),
            )
        })
        .clone();
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
                        Mesh3d(bulb_mesh.clone()),
                        MeshMaterial3d(bulb_material.clone()),
                        NotShadowCaster,
                        Transform::from_xyz(x, y, z),
                    ));
                }
            });
    }
}
