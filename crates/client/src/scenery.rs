//! Scenery on screen: every prop near the camera drawn with its model, or as
//! what it leaves behind once gathered, and with its fruit while it has
//! some.
//!
//! The props of the columns this client has loaded are drawn as the scenery
//! knows them, gathered or not. Past them, a few hundred meters out, the
//! props that stand tall enough to be seen from afar (those in the way of
//! walkers, not bushes) are grown here from the seed alone and drawn
//! standing: what was gathered that far away is not known, and cannot be
//! told apart.

use std::{
    collections::{HashMap, HashSet},
    f32::consts::TAU,
};

use bevy::prelude::*;
use messoria_content::{PropDef, PropId, Remains};
use messoria_shared::{
    content::Content,
    scenery::{Scenery, SceneryChanged, column_of_prop},
    valley::{PlacedProp, PropKey, Valley},
};
use messoria_worldgen::{column_of, in_world, props_in_column};

use crate::{
    art::{Models, srgb},
    camera::WorldCamera,
};

/// Fruit drawn on a prop, as small blocks over a dome around its middle, in
/// meters before the prop's scale: they stand just proud of a bush's leaves.
const FRUIT_COUNT: u32 = 14;
const FRUIT_SIZE: f32 = 0.08;
const FRUIT_DOME: Vec3 = Vec3::new(0.62, 0.68, 0.62);
const FRUIT_DOME_CENTER: f32 = 0.3;
/// Distance, in columns, out to which loaded columns are drawn, and out to
/// which columns are grown from the seed and drawn.
const NEAR_COLUMNS: i32 = 6;
const FAR_COLUMNS: i32 = 10;
/// Loaded columns drawn, and far columns grown and drawn, in one frame,
/// nearest first, so that walking into new land never stalls a frame.
const NEAR_COLUMNS_PER_FRAME: usize = 4;
const FAR_COLUMNS_PER_FRAME: usize = 3;

pub(crate) struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FruitMaterials>()
            .init_resource::<DrawnColumns>()
            .add_systems(Startup, make_fruit_mesh)
            .add_systems(Update, (draw_columns, redraw_changed).chain());
    }
}

#[derive(Resource)]
struct FruitMesh(Handle<Mesh>);

/// One material for each kind of prop that bears fruit.
#[derive(Resource, Default)]
struct FruitMaterials(HashMap<PropId, Handle<StandardMaterial>>);

/// The columns whose props are drawn, and how.
#[derive(Resource, Default)]
struct DrawnColumns(HashMap<IVec2, DrawnColumn>);

struct DrawnColumn {
    /// Whether it is drawn as the scenery knows it, or grown from the seed.
    loaded: bool,
    props: HashMap<PropKey, Entity>,
}

fn make_fruit_mesh(mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    commands.insert_resource(FruitMesh(meshes.add(Cuboid::from_length(FRUIT_SIZE))));
}

/// What draws props, for the systems that spawn them.
#[derive(bevy::ecs::system::SystemParam)]
struct PropArt<'w> {
    content: Res<'w, Content>,
    models: Res<'w, Models>,
    fruit_mesh: Res<'w, FruitMesh>,
    fruit_materials: ResMut<'w, FruitMaterials>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl PropArt<'_> {
    /// Spawns `prop`, gathered on the day `gathered` if it was, and returns
    /// its entity.
    fn spawn(&mut self, commands: &mut Commands, prop: &PlacedProp, gathered: bool) -> Entity {
        let definition = self.content.prop(prop.key.kind);
        let root = commands
            .spawn((
                Name::new(definition.name.clone()),
                Transform::from_translation(prop.position)
                    .with_rotation(Quat::from_rotation_y(prop.turn))
                    .with_scale(Vec3::splat(prop.scale)),
                Visibility::default(),
            ))
            .id();
        let Some((scene, scale)) = model_of(definition, prop, gathered)
            .and_then(|(path, scale)| Some((self.models.scene(path)?, scale)))
        else {
            return root;
        };
        let model = commands
            .spawn((
                scene,
                Transform::from_scale(Vec3::splat(scale)),
                ChildOf(root),
            ))
            .id();
        let fruit = definition.gather.as_ref().and_then(|gather| gather.fruit);
        if let (Some(color), false) = (fruit, gathered) {
            let materials = &mut self.materials;
            let material = self
                .fruit_materials
                .0
                .entry(prop.key.kind)
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
                    Mesh3d(self.fruit_mesh.0.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(spot),
                    ChildOf(model),
                ));
            }
        }
        root
    }
}

/// Draws the columns that came near the camera, the way the scenery knows
/// them or grown from the seed, and takes away those that went far.
fn draw_columns(
    camera: Single<&Transform, With<WorldCamera>>,
    scenery: Res<Scenery>,
    valley: Option<Res<Valley>>,
    mut art: PropArt,
    mut drawn: ResMut<DrawnColumns>,
    mut commands: Commands,
) {
    let Some(valley) = valley else {
        return;
    };
    let center = column_of(camera.translation.xz());
    let distance = |column: IVec2| (column - center).length_squared();
    let wanted_loaded =
        |column: IVec2| scenery.is_loaded(column) && distance(column) <= NEAR_COLUMNS.pow(2);

    // Columns out of reach, and loaded columns that are no longer, go at
    // once; a far column that loaded keeps its props until it is redrawn.
    drawn.0.retain(|&column, drawn| {
        let keep =
            distance(column) <= FAR_COLUMNS.pow(2) && (!drawn.loaded || wanted_loaded(column));
        if !keep {
            despawn_column(&mut commands, drawn);
        }
        keep
    });

    let (mut near, mut far) = (Vec::new(), Vec::new());
    for z in -FAR_COLUMNS..=FAR_COLUMNS {
        for x in -FAR_COLUMNS..=FAR_COLUMNS {
            let column = center + IVec2::new(x, z);
            if distance(column) > FAR_COLUMNS.pow(2) || !in_world(column) {
                continue;
            }
            let loaded = wanted_loaded(column);
            match drawn.0.get(&column) {
                Some(drawn) if drawn.loaded == loaded => {}
                _ if loaded => near.push(column),
                Some(_) => {}
                None => far.push(column),
            }
        }
    }
    let order = |column: &IVec2| (distance(*column), column.x, column.y);
    near.sort_unstable_by_key(order);
    for column in near.into_iter().take(NEAR_COLUMNS_PER_FRAME) {
        if let Some(old) = drawn.0.remove(&column) {
            despawn_column(&mut commands, &old);
        }
        let props = scenery
            .column(column)
            .map(|prop| {
                let gathered = scenery.gathered(prop.key).is_some();
                (prop.key, art.spawn(&mut commands, prop, gathered))
            })
            .collect();
        drawn.0.insert(
            column,
            DrawnColumn {
                loaded: true,
                props,
            },
        );
    }
    far.sort_unstable_by_key(order);
    for column in far.into_iter().take(FAR_COLUMNS_PER_FRAME) {
        let tall: Vec<PlacedProp> = props_in_column(&valley, &art.content, column)
            .into_iter()
            .filter(|prop| art.content.prop(prop.key.kind).blocks)
            .collect();
        let props = tall
            .iter()
            .map(|prop| (prop.key, art.spawn(&mut commands, prop, false)))
            .collect();
        drawn.0.insert(
            column,
            DrawnColumn {
                loaded: false,
                props,
            },
        );
    }
}

fn despawn_column(commands: &mut Commands, drawn: &DrawnColumn) {
    for &entity in drawn.props.values() {
        commands.entity(entity).despawn();
    }
}

/// Redraws props that were gathered or grew back.
fn redraw_changed(
    mut changes: MessageReader<SceneryChanged>,
    scenery: Res<Scenery>,
    mut art: PropArt,
    mut drawn: ResMut<DrawnColumns>,
    mut commands: Commands,
) {
    let props: HashSet<PropKey> = changes.read().map(|changed| changed.0).collect();
    for key in props {
        let (Some(column), Some(prop)) = (column_of_prop(&scenery, key), scenery.prop(key)) else {
            continue;
        };
        let Some(column) = drawn.0.get_mut(&column).filter(|column| column.loaded) else {
            continue;
        };
        if let Some(old) = column.props.remove(&key) {
            commands.entity(old).despawn();
        }
        let gathered = scenery.gathered(key).is_some();
        column
            .props
            .insert(key, art.spawn(&mut commands, prop, gathered));
    }
}

/// The model a prop is drawn with, and its scale relative to the prop's, if
/// anything stands there.
fn model_of<'a>(
    definition: &'a PropDef,
    prop: &PlacedProp,
    gathered: bool,
) -> Option<(&'a str, f32)> {
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
fn fruit_spots(prop: &PlacedProp) -> impl Iterator<Item = Vec3> {
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
