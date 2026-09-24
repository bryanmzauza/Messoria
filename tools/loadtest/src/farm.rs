//! Bot behavior: keep a row of fields. Till, plant, water every day and
//! harvest what ripens, logging each harvest against the day its crop's data
//! says it should ripen. Bots build no home, so they have no bed: they stay
//! up until the day runs out at 02:00.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{Catalog, ItemId};
use messoria_inventory::HOTBAR_SLOTS;
use messoria_shared::{
    content::Content,
    fields::{tile_at, tile_center},
    protocol::{
        ActionChannel, Asleep, Belongings, Crop, Field, HarvestRequest, ItemAction, PlayerInput,
        Position, UseItem, Watered, WorldClock,
    },
    terrain::Terrain,
    tools,
};

/// Time between two actions, a little over the server's use interval.
const PACE: Duration = tools::USE_INTERVAL.saturating_add(Duration::from_millis(50));
/// Offsets of the bot's fields from where it stands, in tiles.
const PLOT: [IVec2; 3] = [IVec2::new(-1, -2), IVec2::new(0, -2), IVec2::new(1, -2)];
const SEEDS: &str = "turnip_seeds";

pub(crate) struct FarmPlugin {
    pub name: String,
}

impl Plugin for FarmPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Farmer {
            name: self.name.clone(),
            timer: Timer::new(PACE, TimerMode::Repeating),
            plot: None,
            planted_on: HashMap::new(),
        })
        .add_systems(Update, farm);
    }
}

#[derive(Resource)]
struct Farmer {
    name: String,
    timer: Timer,
    /// The ground in the middle of each field of the plot, once chosen.
    plot: Option<Vec<Vec3>>,
    /// Day each field was planted.
    planted_on: HashMap<IVec2, u32>,
}

/// What to do next on one field.
enum Chore {
    Till,
    Plant,
    Water,
    Harvest,
}

fn farm(
    real_time: Res<Time>,
    content: Res<Content>,
    terrain: Res<Terrain>,
    clock: Query<&WorldClock>,
    mut farmer: ResMut<Farmer>,
    player: Query<(&Position, &Belongings, Has<Asleep>), With<InputMarker<PlayerInput>>>,
    fields: Query<(&Field, Has<Watered>, Option<&Crop>)>,
    mut uses: Query<&mut MessageSender<UseItem>, With<Client>>,
    mut harvests: Query<&mut MessageSender<HarvestRequest>, With<Client>>,
) {
    if !farmer.timer.tick(real_time.delta()).just_finished() {
        return;
    }
    let (Ok(clock), Ok((feet, belongings, asleep))) = (clock.single(), player.single()) else {
        return;
    };
    if asleep {
        return;
    }
    let today = clock.0.day();
    if farmer.plot.is_none() {
        farmer.plot = choose_plot(&terrain, feet.0);
    }
    let Some(plot) = farmer.plot.clone() else {
        return;
    };

    for target in plot {
        let tile = tile_at(target);
        let field = fields.iter().find(|(field, ..)| field.tile == tile);
        let chore = match field {
            None => Chore::Till,
            Some((_, _, None)) => Chore::Plant,
            Some((_, _, Some(crop))) if crop.0.is_ripe(content.crop(crop.0.crop)) => Chore::Harvest,
            Some((_, false, Some(_))) => Chore::Water,
            Some((_, true, Some(_))) => continue,
        };

        match chore {
            Chore::Harvest => {
                if let (Some((_, _, Some(crop))), Ok(mut sender)) = (field, harvests.single_mut()) {
                    report_harvest(&farmer, &content, tile, *crop, today);
                    sender.send::<ActionChannel>(HarvestRequest { target });
                    farmer.planted_on.remove(&tile);
                }
            }
            Chore::Till | Chore::Plant | Chore::Water => {
                let item = match chore {
                    Chore::Till => "hoe",
                    Chore::Plant => SEEDS,
                    Chore::Water | Chore::Harvest => "watering_can",
                };
                let (Some(slot), Ok(mut sender)) =
                    (hotbar_slot(&content, belongings, item), uses.single_mut())
                else {
                    return;
                };
                sender.send::<ActionChannel>(UseItem {
                    slot,
                    action: ItemAction::Primary,
                    target: Some(target),
                });
                if matches!(chore, Chore::Plant) {
                    farmer.planted_on.insert(tile, today);
                }
            }
        }
        return;
    }
}

/// The ground in the middle of each field of the plot, ahead of `feet`.
fn choose_plot(terrain: &Terrain, feet: Vec3) -> Option<Vec<Vec3>> {
    let home = tile_at(feet);
    PLOT.iter()
        .map(|&offset| {
            let center = tile_center(home + offset);
            let height = terrain.surface_below(Vec3::new(center.x, feet.y + 3.0, center.y), 6.0)?;
            Some(Vec3::new(center.x, height, center.y))
        })
        .collect()
}

fn hotbar_slot(content: &Catalog, belongings: &Belongings, key: &str) -> Option<u8> {
    let item: ItemId = content.id(key)?;
    (0..HOTBAR_SLOTS)
        .find(|&slot| {
            belongings
                .0
                .slot(slot)
                .is_some_and(|stack| stack.item == item)
        })
        .and_then(|slot| u8::try_from(slot).ok())
}

fn report_harvest(farmer: &Farmer, content: &Catalog, tile: IVec2, crop: Crop, today: u32) {
    let definition = content.crop(crop.0.crop);
    if let Some(planted_on) = farmer.planted_on.get(&tile) {
        info!(
            "{}: {} planted on day {planted_on} ripened by day {today}; its data says {} days",
            farmer.name,
            definition.name,
            definition.days_to_ripen(),
        );
    } else {
        info!(
            "{}: harvesting {} planted before it joined",
            farmer.name, definition.name
        );
    }
}
