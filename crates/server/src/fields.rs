//! Farming: tilling fields, planting, watering and fertilizing them, growing
//! crops overnight, and harvesting them.
//!
//! Each field is an entity replicated to every client. Growth happens in one
//! batch at dawn rather than continuously, so thousands of fields cost almost
//! nothing during the day.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_farming::{Overnight, Planting, harvest_quality};
use messoria_shared::{
    content::Content,
    energy::Energy,
    fields::{tile_at, tile_center, tillable_ground},
    movement::EYE_HEIGHT,
    protocol::{
        Asleep, Belongings, Crop, CurrentWeather, Fertilized, Field, Happened, HarvestRequest,
        Notice, Position, Watered, WorldClock,
    },
    terrain::Terrain,
    tools, village,
};
use messoria_voxel::Brush;

use crate::{
    Beginning, WorldStart,
    building::Built,
    day_cycle::{ClockSystems, DayStarted},
    feedback::{Show, Tell},
    inventory::{FieldTask, FieldWork, ItemUseSystems},
    players::ControlledCharacter,
    scenery::Scenery,
    terrain::GroundReshaped,
};

/// How far the ground under a field may move, in meters, before the field
/// is lost.
const GROUND_TOLERANCE: f32 = 0.05;
/// Offsets of the samples at a tile's corners from the tile.
const CORNERS: [IVec2; 4] = [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE];

pub(crate) struct FieldsPlugin;

impl Plugin for FieldsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FieldIndex>()
            .add_systems(Startup, restore_fields)
            .add_systems(
                PreUpdate,
                (
                    work_fields.after(ItemUseSystems),
                    harvest_crops.after(MessageSystems::Receive),
                ),
            )
            .add_systems(FixedUpdate, grow_crops_at_dawn.after(ClockSystems))
            .add_systems(Update, ruin_disturbed_fields);
    }
}

/// The field entity on each tilled tile.
#[derive(Resource, Default)]
struct FieldIndex(HashMap<IVec2, Entity>);

/// Brings back the fields of a saved world, as they were.
fn restore_fields(
    beginning: Res<Beginning>,
    mut index: ResMut<FieldIndex>,
    mut commands: Commands,
) {
    let WorldStart::Resume(saved) = &beginning.0 else {
        return;
    };
    for field in &saved.world.fields {
        let mut entity = commands.spawn(field_bundle(field.tile, field.height));
        if field.watered {
            entity.insert(Watered);
        }
        if field.fertilized {
            entity.insert(Fertilized);
        }
        if let Some(planting) = field.crop {
            entity.insert(Crop(planting));
        }
        index.0.insert(field.tile, entity.id());
    }
}

fn field_bundle(tile: IVec2, height: f32) -> impl Bundle {
    (
        Name::new(format!("Field {tile}")),
        Field { tile, height },
        Replicate::to_clients(NetworkTarget::All),
    )
}

fn work_fields(
    content: Res<Content>,
    terrain: Res<Terrain>,
    scenery: Res<Scenery>,
    built: Built,
    clock: Single<&WorldClock>,
    mut work: MessageReader<FieldWork>,
    characters: Query<&Position>,
    mut workers: Query<(&mut Energy, &mut Belongings), Without<Asleep>>,
    fields: Query<(&Field, Has<Watered>, Has<Fertilized>, Has<Crop>)>,
    mut index: ResMut<FieldIndex>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut commands: Commands,
) {
    for job in work.read() {
        let (Ok(feet), Ok((mut energy, mut belongings))) = (
            characters.get(job.character),
            workers.get_mut(job.character),
        ) else {
            continue;
        };
        if !job.target.is_finite() || !tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, job.target) {
            continue;
        }
        let tile = tile_at(job.target);
        let middle = tile_center(tile);
        let field = index.0.get(&tile).copied();
        let state = field.and_then(|field| fields.get(field).ok());
        let refuse = |notice| Tell {
            character: job.character,
            notice,
        };
        let at = |height: f32| Vec3::new(middle.x, height, middle.y);

        match (job.task, field, state) {
            (FieldTask::Till, None, _) => {
                // A tile reaches about 0.7 m from its middle to its corners.
                if village::reaches(job.target, 0.0) {
                    tell.write(refuse(Notice::ProtectedGround));
                    continue;
                }
                let ground = at(job.target.y);
                if scenery.blocks(ground, 0.71) || built.covers(ground, 0.71) {
                    tell.write(refuse(Notice::SceneryInTheWay));
                    continue;
                }
                let Some(height) = tillable_ground(&terrain, tile, job.target.y) else {
                    tell.write(refuse(Notice::NotTillable));
                    continue;
                };
                if !energy.try_spend(tools::HOE_ENERGY) {
                    tell.write(refuse(Notice::NotEnoughEnergy));
                    continue;
                }
                let field = commands.spawn(field_bundle(tile, height)).id();
                index.0.insert(tile, field);
                show.write(Show::at(Happened::Tilled, at(height), job.character));
            }
            (FieldTask::Water, Some(field), Some((land, false, _, _))) => {
                if energy.try_spend(tools::WATERING_ENERGY) {
                    commands.entity(field).insert(Watered);
                    show.write(Show::at(Happened::Watered, at(land.height), job.character));
                } else {
                    tell.write(refuse(Notice::NotEnoughEnergy));
                }
            }
            (FieldTask::Plant(crop), Some(field), Some((land, _, _, false))) => {
                if !content.crop(crop).seasons.contains(&clock.0.season()) {
                    tell.write(refuse(Notice::OutOfSeason));
                } else if belongings.0.take_one(job.slot).is_some() {
                    commands.entity(field).insert(Crop(Planting::new(crop)));
                    show.write(Show::at(Happened::Planted, at(land.height), job.character));
                }
            }
            (FieldTask::Fertilize, Some(field), Some((land, _, false, _))) => {
                if belongings.0.take_one(job.slot).is_none() {
                    continue;
                }
                commands.entity(field).insert(Fertilized);
                show.write(Show::at(
                    Happened::Fertilized,
                    at(land.height),
                    job.character,
                ));
            }
            _ => {}
        }
    }
}

fn harvest_crops(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    index: Res<FieldIndex>,
    mut clients: Query<(&mut MessageReceiver<HarvestRequest>, &ControlledCharacter)>,
    characters: Query<&Position>,
    mut workers: Query<&mut Belongings, Without<Asleep>>,
    mut crops: Query<(&Field, &mut Crop, Has<Fertilized>)>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut commands: Commands,
) {
    for (mut requests, character) in &mut clients {
        for HarvestRequest { target } in requests.receive() {
            let (Ok(feet), Ok(mut belongings)) =
                (characters.get(character.0), workers.get_mut(character.0))
            else {
                continue;
            };
            if !target.is_finite() || !tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, target) {
                continue;
            }
            let Some(&field) = index.0.get(&tile_at(target)) else {
                continue;
            };
            let Ok((land, mut crop, fertilized)) = crops.get_mut(field) else {
                continue;
            };
            let definition = content.crop(crop.0.crop);
            let quality = harvest_quality(rand::random_range(0..100), fertilized);
            let room = belongings.0.room_for(&content, definition.produce, quality);
            let refusal = if !crop.0.is_ripe(definition) {
                Some(Notice::NotRipe)
            } else if room < u32::from(definition.harvest) {
                Some(Notice::NoRoom)
            } else {
                None
            };
            if let Some(notice) = refusal {
                tell.write(Tell {
                    character: character.0,
                    notice,
                });
                continue;
            }
            let middle = tile_center(land.tile);
            show.write(Show::at(
                Happened::Harvested,
                Vec3::new(middle.x, land.height, middle.y),
                character.0,
            ));

            let mut planting = crop.0;
            let Some(harvest) = planting.harvest(definition, quality) else {
                continue;
            };
            belongings.0.add(
                &content,
                harvest.produce,
                harvest.quality,
                harvest.count,
                clock.0.day(),
            );
            if harvest.regrows {
                crop.0 = planting;
            } else {
                commands.entity(field).remove::<Crop>();
            }
            // Fertilizer feeds one harvest.
            commands.entity(field).remove::<Fertilized>();
        }
    }
}

/// Grows every crop by the night that just passed, then sets which fields
/// start the new day watered: all of them if it rains, none otherwise.
fn grow_crops_at_dawn(
    content: Res<Content>,
    mut dawns: MessageReader<DayStarted>,
    clock: Single<(&WorldClock, &CurrentWeather)>,
    mut fields: Query<(Entity, Option<&mut Crop>, Has<Watered>), With<Field>>,
    mut commands: Commands,
) {
    if dawns.read().last().is_none() {
        return;
    }
    let (clock, weather) = *clock;
    let season = clock.0.season();
    let rain = weather.0.waters_fields();

    for (field, crop, watered) in &mut fields {
        if let Some(mut crop) = crop {
            let mut planting = crop.0;
            match planting.grow_overnight(content.crop(planting.crop), watered, season) {
                Overnight::Withered => {
                    commands.entity(field).remove::<Crop>();
                }
                Overnight::Grew | Overnight::Dry | Overnight::Waiting => {
                    crop.set_if_neq(Crop(planting));
                }
            }
        }
        match (watered, rain) {
            (true, false) => {
                commands.entity(field).remove::<Watered>();
            }
            (false, true) => {
                commands.entity(field).insert(Watered);
            }
            _ => {}
        }
    }
}

/// Digging or raising the ground under a field destroys it, crop and all.
fn ruin_disturbed_fields(
    mut reshaped: MessageReader<GroundReshaped>,
    fields: Query<&Field>,
    mut index: ResMut<FieldIndex>,
    mut commands: Commands,
) {
    for GroundReshaped(brush) in reshaped.read() {
        let reach = Vec3::splat(brush.radius + 1.0);
        let min = tile_at(brush.center - reach);
        let max = tile_at(brush.center + reach);
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                let tile = IVec2::new(x, z);
                let Some(&entity) = index.0.get(&tile) else {
                    continue;
                };
                if fields
                    .get(entity)
                    .is_ok_and(|field| ground_moves_under(brush, field))
                {
                    index.0.remove(&tile);
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

/// Whether `brush` moves the ground a field rests on: the samples at its
/// tile's corners, which lie at about the field's height. Ground moving
/// further out only tilts the terrain near the field's edges a little, which
/// the soil drawn for it is deep enough to hide.
fn ground_moves_under(brush: &Brush, field: &Field) -> bool {
    CORNERS.iter().any(|&corner| {
        let column = (field.tile + corner).as_vec2();
        brush.shift(column, field.height).abs() > GROUND_TOLERANCE
    })
}

#[cfg(test)]
mod tests {
    use messoria_shared::tools::ShovelAction;

    use super::*;

    const FIELD: Field = Field {
        tile: IVec2::ZERO,
        height: 10.0,
    };

    fn dig_at(x: f32) -> Brush {
        tools::shovel_brush(Vec3::new(x, 10.0, 0.5), ShovelAction::Dig)
    }

    #[test]
    fn digging_in_a_field_or_next_to_it_ruins_it() {
        assert!(ground_moves_under(&dig_at(0.5), &FIELD));
        assert!(ground_moves_under(&dig_at(1.5), &FIELD));
    }

    #[test]
    fn digging_a_tile_away_leaves_it() {
        assert!(!ground_moves_under(&dig_at(2.5), &FIELD));
        assert!(!ground_moves_under(&dig_at(-1.5), &FIELD));
    }

    #[test]
    fn digging_on_higher_ground_next_to_a_field_leaves_it() {
        let above = tools::shovel_brush(Vec3::new(1.5, 11.0, 0.5), ShovelAction::Dig);
        assert!(!ground_moves_under(&above, &FIELD));
    }
}
