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
    fields::{tile_at, tillable_ground},
    movement::EYE_HEIGHT,
    protocol::{
        Asleep, Belongings, Crop, CurrentWeather, Fertilized, Field, HarvestRequest, Position,
        Watered, WorldClock,
    },
    terrain::Terrain,
    tools,
};
use messoria_voxel::Brush;

use crate::{
    day_cycle::{ClockSystems, DayStarted},
    inventory::{FieldTask, FieldWork, ItemUseSystems},
    players::ControlledCharacter,
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

fn work_fields(
    content: Res<Content>,
    terrain: Res<Terrain>,
    clock: Single<&WorldClock>,
    mut work: MessageReader<FieldWork>,
    characters: Query<&Position>,
    mut workers: Query<(&mut Energy, &mut Belongings), Without<Asleep>>,
    fields: Query<(Has<Watered>, Has<Fertilized>, Has<Crop>), With<Field>>,
    mut index: ResMut<FieldIndex>,
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
        let field = index.0.get(&tile).copied();
        let state = field.and_then(|field| fields.get(field).ok());

        match (job.task, field, state) {
            (FieldTask::Till, None, _) => {
                let Some(height) = tillable_ground(&terrain, tile, job.target.y) else {
                    continue;
                };
                if !energy.try_spend(tools::HOE_ENERGY) {
                    continue;
                }
                let field = commands
                    .spawn((
                        Name::new(format!("Field {tile}")),
                        Field { tile, height },
                        Replicate::to_clients(NetworkTarget::All),
                    ))
                    .id();
                index.0.insert(tile, field);
            }
            (FieldTask::Water, Some(field), Some((false, _, _))) => {
                if energy.try_spend(tools::WATERING_ENERGY) {
                    commands.entity(field).insert(Watered);
                }
            }
            (FieldTask::Plant(crop), Some(field), Some((_, _, false))) => {
                let in_season = content.crop(crop).seasons.contains(&clock.0.season());
                if in_season && belongings.0.take_one(job.slot).is_some() {
                    commands.entity(field).insert(Crop(Planting::new(crop)));
                }
            }
            (FieldTask::Fertilize, Some(field), Some((_, false, _))) => {
                let spread = belongings.0.take_one(job.slot).is_some();
                if spread {
                    commands.entity(field).insert(Fertilized);
                }
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
    mut crops: Query<(&mut Crop, Has<Fertilized>)>,
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
            let Ok((mut crop, fertilized)) = crops.get_mut(field) else {
                continue;
            };
            let definition = content.crop(crop.0.crop);
            let quality = harvest_quality(rand::random_range(0..100), fertilized);
            let room = belongings.0.room_for(&content, definition.produce, quality);
            if !crop.0.is_ripe(definition) || room < u32::from(definition.harvest) {
                continue;
            }

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
