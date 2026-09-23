//! Fields and crops on screen: tilled squares that darken when watered, and
//! plants built from simple shapes that grow with each stage and show their
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

/// Side of a tilled square, a little under the tile so rows read as rows.
const SOIL_SIZE: f32 = 0.92;
/// Lifts the soil above the terrain so it is not hidden inside it.
const SOIL_LIFT: f32 = 0.04;
const DRY_SOIL: Color = Color::srgb(0.42, 0.3, 0.2);
const WET_SOIL: Color = Color::srgb(0.25, 0.17, 0.11);
const LEAF_COLOR: Color = Color::srgb(0.32, 0.62, 0.25);
/// Height of a fully grown stem, in meters.
const STEM_HEIGHT: f32 = 0.55;

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
    stem: Handle<Mesh>,
    leaf: Handle<Mesh>,
    produce: Handle<Mesh>,
    leaf_material: Handle<StandardMaterial>,
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
        soil: meshes.add(Plane3d::default().mesh().size(SOIL_SIZE, SOIL_SIZE)),
        dry_soil: materials.add(matte(DRY_SOIL)),
        wet_soil: materials.add(matte(WET_SOIL)),
        stem: meshes.add(Cylinder::new(0.025, 1.0)),
        leaf: meshes.add(Sphere::new(0.5)),
        produce: meshes.add(Sphere::new(0.5)),
        leaf_material: materials.add(matte(LEAF_COLOR)),
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
            Transform::from_xyz(0.0, SOIL_LIFT, 0.0),
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
    art: Res<FieldArt>,
    crops: Query<(Entity, &Crop, Option<&PlantModel>), Changed<Crop>>,
    mut commands: Commands,
) {
    for (field, crop, previous) in &crops {
        if let Some(previous) = previous {
            commands.entity(previous.0).despawn();
        }
        let plant = spawn_plant(&mut commands, &content, &art, crop.0);
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

/// A stem that grows with each stage, a pair of leaves, and the produce once
/// the crop is ripe.
fn spawn_plant(
    commands: &mut Commands,
    content: &Content,
    art: &FieldArt,
    planting: Planting,
) -> Entity {
    let crop = content.crop(planting.crop);
    #[expect(clippy::cast_precision_loss, reason = "stage counts are tiny")]
    let growth = planting.stage(crop) as f32 / crop.stages.len() as f32;
    let height = STEM_HEIGHT * (0.2 + 0.8 * growth);
    let leaf_size = 0.08 + 0.14 * growth;

    commands
        .spawn((
            Name::new(crop.name.clone()),
            Transform::from_xyz(0.0, SOIL_LIFT, 0.0),
            Visibility::default(),
        ))
        .with_children(|plant| {
            plant.spawn((
                Mesh3d(art.stem.clone()),
                MeshMaterial3d(art.leaf_material.clone()),
                Transform::from_xyz(0.0, height / 2.0, 0.0).with_scale(Vec3::new(1.0, height, 1.0)),
            ));
            for side in [-1.0, 1.0] {
                plant.spawn((
                    Mesh3d(art.leaf.clone()),
                    MeshMaterial3d(art.leaf_material.clone()),
                    Transform::from_xyz(side * leaf_size * 0.6, height * 0.6, 0.0)
                        .with_scale(Vec3::new(leaf_size, leaf_size * 0.3, leaf_size * 0.6)),
                ));
            }
            if planting.is_ripe(crop) {
                plant.spawn((
                    Mesh3d(art.produce.clone()),
                    MeshMaterial3d(art.produce_materials[&planting.crop].clone()),
                    Transform::from_xyz(0.0, height, 0.0).with_scale(Vec3::splat(0.2)),
                ));
            }
        })
        .id()
}
