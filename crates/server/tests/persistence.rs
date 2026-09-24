//! Stopping a world in the middle of a day and starting it again from its
//! save resumes it exactly as it was: its terrain, clock, weather, market,
//! fields and players.

mod common;

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use common::HostedWorld;
use lightyear::prelude::input::native::InputMarker;
use messoria_calendar::WorldTime;
use messoria_content::Catalog;
use messoria_economy::{Market, SalesLedger, Wallet};
use messoria_farming::Planting;
use messoria_inventory::Inventory;
use messoria_save::SaveDir;
use messoria_server::{WorldSetup, WorldStart};
use messoria_shared::{
    content::load_content,
    energy::Energy,
    protocol::{
        Belongings, Crop, CurrentWeather, Deal, Fertilized, Field, Heading, MarketState, Money,
        PlayerInput, Position, SoldToday, Trade, Watered,
    },
    terrain::Terrain,
};

#[test]
fn a_restarted_world_resumes_exactly_where_it_stopped() {
    let content = load_content().expect("the shipped content is valid");
    let save_dir = scratch_folder("restart");
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    assert!(matches!(setup.start, WorldStart::New { .. }));
    let mut world = HostedWorld::with_world(content.clone(), setup);

    play_a_morning(&mut world, &content);
    world.stop();
    let before = Snapshot::of(&mut world);
    drop(world);

    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    assert!(matches!(setup.start, WorldStart::Resume(_)));
    let mut resumed = HostedWorld::with_world(content, setup);
    assert_eq!(resumed.started_at, before.clock);
    let after = Snapshot::of(&mut resumed);

    assert_eq!(before.terrain.len(), after.terrain.len());
    for chunk in before.terrain.positions() {
        assert!(
            before.terrain.get(chunk) == after.terrain.get(chunk),
            "chunk {chunk:?} differs after the restart"
        );
    }
    assert_eq!(before.weather, after.weather);
    assert_eq!(before.market, after.market);
    assert_eq!(before.fields, after.fields);
    assert_eq!(before.player, after.player);
    assert!(
        before.position.distance(after.position) < 1e-3,
        "the character moved from {} to {}",
        before.position,
        after.position
    );
}

/// Digs, farms and trades, leaving a mark on every part of the world a save
/// keeps.
fn play_a_morning(world: &mut HostedWorld, content: &Catalog) {
    let (feet, _) = world.character().expect("the host has a character");
    let dig_at = ground(world, feet + Vec3::new(-3.0, 0.0, 0.0));
    let shovel = world.slot_holding(content, "shovel");
    world.use_item(shovel, dig_at);

    let field = world.ground_ahead();
    for tool in ["hoe", "turnip_seeds", "watering_can"] {
        let slot = world.slot_holding(content, tool);
        world.use_item(slot, field);
    }
    assert!(
        world
            .crop(messoria_shared::fields::tile_at(field))
            .is_some()
    );

    world.set_time("10:00");
    let stall = world.go_to_stall(content, "grocer");
    let berries = content.id("wild_berries").expect("wild berries exist");
    let slot = world.slot_of(berries).expect("players start with berries");
    for deal in [
        Deal::Sell { slot, count: 5 },
        Deal::Buy {
            item: content.id("potato_seeds").expect("potato seeds exist"),
            count: 2,
        },
    ] {
        world.send(Trade {
            shop: stall.shop,
            deal,
        });
        world.run(common::PAUSE_BETWEEN_USES);
    }
    assert_ne!(world.coins(), content.starting_money());
}

/// Everything a save keeps, as the running world has it.
struct Snapshot {
    clock: WorldTime,
    terrain: messoria_voxel::ChunkMap,
    weather: CurrentWeather,
    market: Market,
    fields: Vec<(IVec2, u32, bool, bool, Option<Planting>)>,
    player: (u32, Energy, Inventory, Wallet, SalesLedger),
    position: Vec3,
}

impl Snapshot {
    fn of(hosted: &mut HostedWorld) -> Self {
        let clock = hosted.clock();
        let world = hosted.world();
        let weather = *world
            .query::<&CurrentWeather>()
            .single(world)
            .expect("the world has weather");
        let market = world
            .query::<&MarketState>()
            .single(world)
            .expect("the world has a market")
            .0
            .clone();
        let mut fields: Vec<_> = world
            .query::<(&Field, Has<Watered>, Has<Fertilized>, Option<&Crop>)>()
            .iter(world)
            .map(|(field, watered, fertilized, crop)| {
                (
                    field.tile,
                    field.height.to_bits(),
                    watered,
                    fertilized,
                    crop.map(|crop| crop.0),
                )
            })
            .collect();
        fields.sort_by_key(|field| (field.0.x, field.0.y));
        let (position, heading, energy, belongings, money, sold) = world
            .query_filtered::<(
                &Position,
                &Heading,
                &Energy,
                &Belongings,
                &Money,
                &SoldToday,
            ), With<InputMarker<PlayerInput>>>()
            .single(world)
            .expect("the host has a character");
        let player = (
            heading.0.to_bits(),
            *energy,
            belongings.0.clone(),
            money.0,
            sold.0.clone(),
        );
        let position = position.0;
        Self {
            clock,
            terrain: world.resource::<Terrain>().0.clone(),
            weather,
            market,
            fields,
            player,
            position,
        }
    }
}

fn ground(world: &mut HostedWorld, above: Vec3) -> Vec3 {
    let height = world
        .world()
        .resource::<Terrain>()
        .surface_below(above + Vec3::Y * 3.0, 6.0)
        .expect("ground below");
    above.with_y(height)
}

/// An empty folder of the test's own.
fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
