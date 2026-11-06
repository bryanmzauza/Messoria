//! Bot behavior: get a farm lot and keep a row of fields on it. The bot
//! walks to the village notice board, claims the first free lot, walks to
//! it by the road and in through its gate, then tills, plants, waters every
//! day and harvests what ripens, logging each harvest against the day its
//! crop's data says it should ripen. Bots build no home, so they have no
//! bed: they stay up until the day runs out at 02:00.

use std::{collections::HashMap, time::Duration};

use bevy::prelude::*;
use lightyear::prelude::{
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
    *,
};
use messoria_content::{Catalog, ItemId, Purpose};
use messoria_inventory::HOTBAR_SLOTS;
use messoria_shared::{
    content::Content,
    fields::{tile_at, tile_center},
    lots::{LOT_COUNT, LotId, lot, lots},
    players::player_key,
    protocol::{
        ActionChannel, Asleep, Belongings, ClaimLot, Crop, Field, HarvestRequest, ItemAction,
        LotClaim, PlayerId, PlayerInput, Position, Structure, UseItem, Watered, WorldClock,
    },
    terrain::Terrain,
    tools,
};

/// Time between two actions, a little over the server's use interval.
const PACE: Duration = tools::USE_INTERVAL.saturating_add(Duration::from_millis(50));
/// Offsets of the bot's fields from where it stands, in tiles.
const PLOT: [IVec2; 3] = [IVec2::new(-1, -2), IVec2::new(0, -2), IVec2::new(1, -2)];
const SEEDS: &str = "turnip_seeds";
/// How close to a waypoint the bot must come to pass it.
const ARRIVED: f32 = 1.0;
/// Ticks over which a walking bot must make headway, and how much, before
/// it jumps to get unstuck.
const PATIENCE_TICKS: u32 = 90;
const HEADWAY: f32 = 1.0;
/// Where to stand at the board, in its frame, and how far out on the road
/// to walk before turning in at a gate.
const AT_BOARD: Vec3 = Vec3::new(0.0, 0.0, -1.3);
const ROAD_MIDDLE: f32 = 3.5;
/// The way from the notice board out of the village onto each road, from
/// the middle of the square: around the board, then along the streets,
/// skirting the fountain, the well, the benches, the lanterns and the
/// buildings as village.ron lays them out. North, east, south and west, in
/// the lots' order.
const SQUARE_EXITS: [&[Vec2]; 4] = [
    &[
        Vec2::new(5.6, 8.9),
        Vec2::new(5.6, 6.0),
        Vec2::new(3.5, -4.0),
        Vec2::new(1.0, -6.5),
        Vec2::new(0.0, -18.0),
        Vec2::new(0.0, -46.0),
    ],
    &[
        Vec2::new(5.6, 8.9),
        Vec2::new(8.0, 3.0),
        Vec2::new(12.0, 0.5),
        Vec2::new(20.0, 0.0),
        Vec2::new(46.0, 0.0),
    ],
    &[Vec2::new(0.0, 12.0), Vec2::new(0.0, 46.0)],
    &[
        Vec2::new(5.6, 8.9),
        Vec2::new(2.0, 10.0),
        Vec2::new(-2.0, 10.0),
        Vec2::new(-6.5, 9.0),
        Vec2::new(-6.5, 2.5),
        Vec2::new(-10.0, 0.5),
        Vec2::new(-46.0, 0.0),
    ],
];

pub(crate) struct FarmPlugin {
    pub name: String,
}

impl Plugin for FarmPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Farmer {
            name: self.name.clone(),
            timer: Timer::new(PACE, TimerMode::Repeating),
            stage: Stage::ToBoard,
            plot: None,
            planted_on: HashMap::new(),
        })
        .insert_resource(Walk::default())
        .add_systems(Update, farm)
        .add_systems(FixedPreUpdate, walk.in_set(InputSystems::WriteClientInputs));
    }
}

#[derive(Resource)]
struct Farmer {
    name: String,
    timer: Timer,
    stage: Stage,
    /// The ground in the middle of each field of the plot, once chosen.
    plot: Option<Vec<Vec3>>,
    /// Day each field was planted.
    planted_on: HashMap<IVec2, u32>,
}

/// What the bot is up to, from arriving to farming.
enum Stage {
    /// Walking to the notice board.
    ToBoard,
    /// Standing at the board, asking for a lot until one is granted.
    Claiming,
    /// Walking out by the road and in through the gate of its lot.
    ToLot,
    /// On its lot, farming.
    Farming,
}

/// Where the bot walks: the waypoints left to reach.
#[derive(Resource, Default)]
struct Walk {
    waypoints: Vec<Vec2>,
    /// Where the bot was when it last checked its headway.
    checkpoint: Option<Vec2>,
    ticks: u32,
}

/// What to do next on one field.
enum Chore {
    Till,
    Plant,
    Water,
    Harvest,
}

/// Walks the bot along its waypoints, jumping when stuck.
fn walk(
    mut route: ResMut<Walk>,
    mut player: Query<(&Position, &mut ActionState<PlayerInput>), With<InputMarker<PlayerInput>>>,
) {
    let Ok((feet, mut action)) = player.single_mut() else {
        return;
    };
    let here = feet.0.xz();
    while route
        .waypoints
        .first()
        .is_some_and(|next| next.distance(here) < ARRIVED)
    {
        route.waypoints.remove(0);
    }
    let Some(&next) = route.waypoints.first() else {
        action.0 = PlayerInput::default();
        return;
    };
    route.ticks += 1;
    let mut jump = false;
    if route.ticks >= PATIENCE_TICKS {
        route.ticks = 0;
        jump = route
            .checkpoint
            .is_some_and(|checkpoint| checkpoint.distance(here) < HEADWAY);
        if jump {
            info!("stuck at {here}, heading for {next}");
        }
        route.checkpoint = Some(here);
    }
    // Moving forward heads along -z turned by the yaw.
    let toward = next - here;
    action.0 = PlayerInput {
        movement: Vec2::Y,
        yaw: (-toward.x).atan2(-toward.y),
        jump,
        sprint: true,
        held: 0,
    };
}

fn farm(
    real_time: Res<Time>,
    content: Res<Content>,
    terrain: Res<Terrain>,
    clock: Query<&WorldClock>,
    mut farmer: ResMut<Farmer>,
    mut route: ResMut<Walk>,
    player: Query<(&PlayerId, &Position, &Belongings, Has<Asleep>), With<InputMarker<PlayerInput>>>,
    structures: Query<&Structure>,
    claims: Query<&LotClaim>,
    fields: Query<(&Field, Has<Watered>, Option<&Crop>)>,
    mut claiming: Query<&mut MessageSender<ClaimLot>, With<Client>>,
    mut uses: Query<&mut MessageSender<UseItem>, With<Client>>,
    mut harvests: Query<&mut MessageSender<HarvestRequest>, With<Client>>,
) {
    if !farmer.timer.tick(real_time.delta()).just_finished() {
        return;
    }
    let (Ok(clock), Ok((player, feet, belongings, asleep))) = (clock.single(), player.single())
    else {
        return;
    };
    if asleep {
        return;
    }
    let Some(key) = player_key(player.0) else {
        return;
    };
    let own = claims.iter().find(|claim| claim.owner == key);
    match &farmer.stage {
        Stage::ToBoard => {
            let Some(board) = structures.iter().find(|structure| {
                content.structure(structure.kind).purpose == Purpose::NoticeBoard
            }) else {
                return;
            };
            let front = board.to_world(AT_BOARD).xz();
            if route.waypoints.is_empty() && feet.0.xz().distance(front) >= ARRIVED {
                route.waypoints = vec![front];
            } else if route.waypoints.is_empty() {
                farmer.stage = Stage::Claiming;
            }
        }
        Stage::Claiming => {
            if let Some(claim) = own {
                let lot = lot(claim.lot);
                info!("{}: claimed lot {}", farmer.name, lot.id.number());
                let outward = (lot.gate - lot.center()).normalize();
                let road = usize::from(lot.id.0) / (usize::from(LOT_COUNT) / 4);
                route.waypoints = SQUARE_EXITS[road]
                    .iter()
                    .map(|&exit| messoria_worldgen::VILLAGE_CENTER + exit)
                    .chain([lot.gate + outward * ROAD_MIDDLE, lot.gate, lot.center()])
                    .collect();
                farmer.stage = Stage::ToLot;
                return;
            }
            let claimed: Vec<LotId> = claims.iter().map(|claim| claim.lot).collect();
            let (Some(free), Ok(mut sender)) = (
                lots().find(|lot| !claimed.contains(&lot.id)),
                claiming.single_mut(),
            ) else {
                return;
            };
            sender.send::<ActionChannel>(ClaimLot { lot: free.id });
        }
        Stage::ToLot => {
            if route.waypoints.is_empty() {
                farmer.stage = Stage::Farming;
            }
        }
        Stage::Farming => {
            tend_fields(
                &content,
                &terrain,
                *clock,
                &mut farmer,
                *feet,
                belongings,
                &fields,
                &mut uses,
                &mut harvests,
            );
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the systems' resources, handed on"
)]
fn tend_fields(
    content: &Catalog,
    terrain: &Terrain,
    clock: WorldClock,
    farmer: &mut Farmer,
    feet: Position,
    belongings: &Belongings,
    fields: &Query<(&Field, Has<Watered>, Option<&Crop>)>,
    uses: &mut Query<&mut MessageSender<UseItem>, With<Client>>,
    harvests: &mut Query<&mut MessageSender<HarvestRequest>, With<Client>>,
) {
    let today = clock.0.day();
    if farmer.plot.is_none() {
        farmer.plot = choose_plot(terrain, feet.0);
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
                    report_harvest(farmer, content, tile, *crop, today);
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
                    (hotbar_slot(content, belongings, item), uses.single_mut())
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
