//! Fields and crops on screen: tilled squares that darken when watered, and
//! plants drawn with their crop's model for each stage, showing their
//! produce when ripe.

use std::collections::HashMap;

use bevy::prelude::*;
use messoria_content::CropId;
use messoria_farming::Planting;
use messoria_shared::{
    content::Content,
    fields::tile_center,
    protocol::{Crop, Field, Watered},
};

use crate::art::Models;

/// Side of a tilled square, a little under the tile so rows read as rows.
const SOIL_SIZE: f32 = 0.92;
/// Lifts the soil above the terrain so it is not hidden inside it.
const SOIL_LIFT: f32 = 0.04;
/// How far the soil reaches down into the ground, so that it still meets the
/// terrain where the ground around the field slopes away.
const SOIL_DEPTH: f32 = 0.2;
const DRY_SOIL: Color = Color::srgb(0.42, 0.3, 0.2);
const WET_SOIL: Color = Color::srgb(0.25, 0.17, 0.11);
/// Scale the plant models are drawn at.
const PLANT_SCALE: f32 = 1.4;
/// How much a plant turns each day it grows, so rows do not look stamped.
const PLANT_TURN: f32 = 2.4;
/// Where produce hangs on a ripe plant that does not show it, and its size.
const PRODUCE_SPOTS: [Vec3; 3] = [
    Vec3::new(0.12, 0.32, 0.05),
    Vec3::new(-0.1, 0.42, -0.06),
    Vec3::new(0.02, 0.24, -0.13),
];
const PRODUCE_SIZE: f32 = 0.11;

pub(crate) struct FieldsPlugin;

impl Plugin for FieldsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_field_art)
            .add_observer(show_field)
            .add_observer(show_watered)
            .add_observer(show_dried)
            .add_observer(remove_plant)
            .add_systems(Update, show_crops);
    }
}

#[derive(Resource)]
struct FieldArt {
    soil: Handle<Mesh>,
    dry_soil: Handle<StandardMaterial>,
    wet_soil: Handle<StandardMaterial>,
    produce: Handle<Mesh>,
    produce_materials: HashMap<CropId, Handle<StandardMaterial>>,
}

/// The soil square drawn for a field, kept on the field entity.
#[derive(Component)]
struct SoilPatch(Entity);

/// The plant drawn in a field, kept on the field entity.
#[derive(Component)]
struct PlantModel(Entity);

fn load_field_art(
    content: Res<Content>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let matte = |color: Color| StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.9,
        ..default()
    };
    let produce_materials = content
        .crops()
        .map(|(id, crop)| {
            let [red, green, blue] = crop.color;
            (id, materials.add(matte(Color::srgb(red, green, blue))))
        })
        .collect();
    commands.insert_resource(FieldArt {
        soil: meshes.add(Cuboid::new(SOIL_SIZE, SOIL_DEPTH, SOIL_SIZE)),
        dry_soil: materials.add(matte(DRY_SOIL)),
        wet_soil: materials.add(matte(WET_SOIL)),
        produce: meshes.add(Sphere::new(0.5)),
        produce_materials,
    });
}

fn show_field(
    trigger: On<Add, Field>,
    fields: Query<(&Field, Has<Watered>)>,
    art: Res<FieldArt>,
    mut commands: Commands,
) {
    let Ok((field, watered)) = fields.get(trigger.entity) else {
        return;
    };
    let center = tile_center(field.tile);
    let soil = commands
        .spawn((
            Mesh3d(art.soil.clone()),
            MeshMaterial3d(soil_material(&art, watered)),
            Transform::from_xyz(0.0, SOIL_LIFT - SOIL_DEPTH / 2.0, 0.0),
        ))
        .id();
    commands
        .entity(trigger.entity)
        .insert((
            Transform::from_xyz(center.x, field.height, center.y),
            Visibility::default(),
            SoilPatch(soil),
        ))
        .add_child(soil);
}

fn show_watered(
    trigger: On<Add, Watered>,
    art: Res<FieldArt>,
    fields: Query<&SoilPatch>,
    mut soils: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    set_soil(
        &fields,
        &mut soils,
        trigger.entity,
        soil_material(&art, true),
    );
}

fn show_dried(
    trigger: On<Remove, Watered>,
    art: Res<FieldArt>,
    fields: Query<&SoilPatch>,
    mut soils: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    set_soil(
        &fields,
        &mut soils,
        trigger.entity,
        soil_material(&art, false),
    );
}

fn soil_material(art: &FieldArt, watered: bool) -> Handle<StandardMaterial> {
    if watered {
        art.wet_soil.clone()
    } else {
        art.dry_soil.clone()
    }
}

/// Recolors a field's soil. Fields that are not shown yet pick their color
/// when they are.
fn set_soil(
    fields: &Query<&SoilPatch>,
    soils: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
    field: Entity,
    material: Handle<StandardMaterial>,
) {
    if let Ok(patch) = fields.get(field)
        && let Ok(mut current) = soils.get_mut(patch.0)
    {
        current.0 = material;
    }
}

/// Rebuilds a field's plant whenever its crop changes.
fn show_crops(
    content: Res<Content>,
    models: Res<Models>,
    art: Res<FieldArt>,
    crops: Query<(Entity, &Crop, Option<&PlantModel>), Changed<Crop>>,
    mut commands: Commands,
) {
    for (field, crop, previous) in &crops {
        if let Some(previous) = previous {
            commands.entity(previous.0).despawn();
        }
        let plant = spawn_plant(&mut commands, &content, &models, &art, crop.0);
        commands
            .entity(field)
            .add_child(plant)
            .insert(PlantModel(plant));
    }
}

fn remove_plant(trigger: On<Remove, Crop>, plants: Query<&PlantModel>, mut commands: Commands) {
    if let Ok(plant) = plants.get(trigger.entity) {
        commands.entity(plant.0).despawn();
        commands.entity(trigger.entity).remove::<PlantModel>();
    }
}

/// The plant's model for its stage, with its produce on it once ripe where
/// the model does not show it.
fn spawn_plant(
    commands: &mut Commands,
    content: &Content,
    models: &Models,
    art: &FieldArt,
    planting: Planting,
) -> Entity {
    let crop = content.crop(planting.crop);
    let stage = planting.stage(crop).min(crop.models.len() - 1);
    let ripe = planting.is_ripe(crop);
    let turn = Quat::from_rotation_y(f32::from(planting.days_grown) * PLANT_TURN);
    commands
        .spawn((
            Name::new(crop.name.clone()),
            Transform::from_xyz(0.0, SOIL_LIFT, 0.0).with_rotation(turn),
            Visibility::default(),
        ))
        .with_children(|plant| {
            if let Some(scene) = models.scene(&crop.models[stage]) {
                plant.spawn((scene, Transform::from_scale(Vec3::splat(PLANT_SCALE))));
            }
            if ripe && !crop.produce_shown {
                for spot in PRODUCE_SPOTS {
                    plant.spawn((
                        Mesh3d(art.produce.clone()),
                        MeshMaterial3d(art.produce_materials[&planting.crop].clone()),
                        Transform::from_translation(spot).with_scale(Vec3::splat(PRODUCE_SIZE)),
                    ));
                }
            }
        })
        .id()
}
