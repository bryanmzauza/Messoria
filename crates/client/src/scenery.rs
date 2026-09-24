//! Scenery on screen: each prop the server scattered, drawn with its model,
//! or as what it leaves behind once gathered, and with its fruit while it
//! has some.

use std::{
    collections::{HashMap, HashSet},
    f32::consts::TAU,
};

use bevy::prelude::*;
use messoria_content::{PropDef, PropId, Remains};
use messoria_shared::{
    content::Content,
    protocol::{Gathered, Prop},
};

use crate::art::{Models, srgb};

/// Fruit drawn on a prop, as spheres over a dome around its middle, in the
/// model's own units (before the prop's scale).
const FRUIT_COUNT: u32 = 14;
const FRUIT_RADIUS: f32 = 0.014;
const FRUIT_DOME: Vec3 = Vec3::new(0.13, 0.12, 0.13);
const FRUIT_DOME_CENTER: f32 = 0.05;

pub(crate) struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FruitMaterials>()
            .add_systems(Startup, make_fruit_mesh)
            .add_systems(Update, draw_props);
    }
}

/// What a prop is drawn with, a child of its entity, replaced when the prop
/// is gathered or grows back.
#[derive(Component)]
struct PropModel(Entity);

#[derive(Resource)]
struct FruitMesh(Handle<Mesh>);

/// One material for each kind of prop that bears fruit.
#[derive(Resource, Default)]
struct FruitMaterials(HashMap<PropId, Handle<StandardMaterial>>);

fn make_fruit_mesh(mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    commands.insert_resource(FruitMesh(meshes.add(Sphere::new(FRUIT_RADIUS))));
}

/// Draws props that arrived, were gathered or grew back since the last
/// frame.
fn draw_props(
    content: Res<Content>,
    models: Res<Models>,
    fruit_mesh: Res<FruitMesh>,
    mut fruit_materials: ResMut<FruitMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    arrived: Query<Entity, Or<(Added<Prop>, Added<Gathered>)>>,
    mut regrown: RemovedComponents<Gathered>,
    props: Query<(&Prop, Has<Gathered>, Option<&PropModel>)>,
    mut commands: Commands,
) {
    let changed: HashSet<Entity> = arrived.iter().chain(regrown.read()).collect();
    for entity in changed {
        let Ok((prop, gathered, drawn)) = props.get(entity) else {
            continue;
        };
        let definition = content.prop(prop.kind);
        if let Some(PropModel(old)) = drawn {
            commands.entity(*old).despawn();
        }
        if drawn.is_none() {
            commands.entity(entity).insert((
                Name::new(definition.name.clone()),
                Transform::from_translation(prop.position)
                    .with_rotation(Quat::from_rotation_y(prop.turn))
                    .with_scale(Vec3::splat(prop.scale)),
                Visibility::default(),
            ));
        }
        let Some((scene, scale)) = model_of(definition, prop, gathered)
            .and_then(|(path, scale)| Some((models.scene(path)?, scale)))
        else {
            commands.entity(entity).remove::<PropModel>();
            continue;
        };
        let model = commands
            .spawn((
                scene,
                Transform::from_scale(Vec3::splat(scale)),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(PropModel(model));

        let fruit = definition.gather.as_ref().and_then(|gather| gather.fruit);
        if let (Some(color), false) = (fruit, gathered) {
            let material = fruit_materials
                .0
                .entry(prop.kind)
                .or_insert_with(|| {
                    materials.add(StandardMaterial {
                        base_color: srgb(color),
                        perceptual_roughness: 0.5,
                        ..default()
                    })
                })
                .clone();
            for spot in fruit_spots(prop) {
                commands.spawn((
                    Mesh3d(fruit_mesh.0.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(spot),
                    ChildOf(model),
                ));
            }
        }
    }
}

/// The model a prop is drawn with, and its scale relative to the prop's, if
/// anything stands there.
fn model_of<'a>(definition: &'a PropDef, prop: &Prop, gathered: bool) -> Option<(&'a str, f32)> {
    let standing = definition
        .models
        .get(usize::from(prop.model))
        .or_else(|| definition.models.first())
        .map(|model| (model.as_str(), 1.0));
    if !gathered {
        return standing;
    }
    match definition.gather.as_ref().map(|gather| &gather.remains) {
        Some(Remains::Model { model, scale }) => Some((model, *scale)),
        Some(Remains::Itself) | None => standing,
        Some(Remains::Nothing) => None,
    }
}

/// Where fruit hangs on `prop`: spread over the upper half of a dome, turned
/// differently on each prop so that bushes do not all look alike.
fn fruit_spots(prop: &Prop) -> impl Iterator<Item = Vec3> {
    // The golden angle spreads points evenly around the dome.
    let golden_angle = TAU * (1.0 - 1.0 / std::f32::consts::GOLDEN_RATIO);
    let start = prop.turn;
    (0..FRUIT_COUNT).map(move |index| {
        #[expect(clippy::cast_precision_loss, reason = "a handful of fruit")]
        let height = (index as f32 + 0.5) / FRUIT_COUNT as f32;
        #[expect(clippy::cast_precision_loss, reason = "a handful of fruit")]
        let angle = start + golden_angle * index as f32;
        let around = (1.0 - height * height).sqrt();
        Vec3::new(
            angle.cos() * around * FRUIT_DOME.x,
            FRUIT_DOME_CENTER + height * FRUIT_DOME.y,
            angle.sin() * around * FRUIT_DOME.z,
        )
    })
}
