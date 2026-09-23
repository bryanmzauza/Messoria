//! A crop planted on the first day ripens on the day its data file predicts
//! and can then be harvested.
//!
//! This runs the whole game in one app, as a player hosting their own world
//! does, and plays through the days by sending the same messages a player's
//! client sends. Time runs one simulation tick per update, and nights are
//! skipped by going to sleep.

use std::net::{Ipv4Addr, SocketAddr};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use lightyear::prelude::{input::native::InputMarker, server::Server, *};
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_content::{Catalog, ItemId};
use messoria_farming::Planting;
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    content::load_content,
    fields::{tile_at, tile_center},
    network::{self, NetworkRole},
    protocol::{
        ActionChannel, Belongings, Crop, Field, HarvestRequest, ItemAction, PlayerInput, Position,
        SleepRequest, UseItem, Watered, WorldClock,
    },
    terrain::Terrain,
};

/// Updates to run while waiting for something; far more than it takes.
const PATIENCE: u32 = 2_000;
/// Updates between two item uses: more than the server's minimum interval
/// between uses at one tick per update.
const PAUSE_BETWEEN_USES: u32 = 10;

#[test]
fn a_crop_ripens_on_the_day_its_data_predicts() {
    let content = load_content().expect("the shipped content is valid");
    let turnip = content.crop_id("turnip").expect("turnips exist");
    let days_to_ripen = u32::from(content.crop(turnip).days_to_ripen());
    let mut world = HostedWorld::new(content.clone());

    let target = world.ground_ahead();
    let tile = tile_at(target);
    let hoe = world.slot_holding(&content, "hoe");
    world.use_item(hoe, target);
    assert!(world.field(tile).is_some(), "the hoe tills the ground");

    let seeds = world.slot_holding(&content, "turnip_seeds");
    world.use_item(seeds, target);
    let planted_on = world.clock().day();
    assert_eq!(world.crop(tile).map(|crop| crop.crop), Some(turnip));

    let watering_can = world.slot_holding(&content, "watering_can");
    let mut ripe_on = None;
    for _ in 0..=days_to_ripen + 2 {
        world.use_item(watering_can, target);
        assert!(world.is_watered(tile), "the watering can waters the field");
        world.sleep_through_the_night();

        let crop = world.crop(tile).expect("the crop keeps growing");
        if crop.is_ripe(content.crop(turnip)) {
            ripe_on = Some(world.clock().day());
            break;
        }
    }
    assert_eq!(ripe_on, Some(planted_on + days_to_ripen));

    let produce = content.crop(turnip).produce;
    let before = world.carried(produce);
    world.harvest(target);
    assert_eq!(world.carried(produce), before + 1);
    assert_eq!(world.crop(tile), None, "turnips do not regrow");
}

/// A world hosted in-process, with the host playing in it.
struct HostedWorld {
    app: App,
}

impl HostedWorld {
    fn new(content: Catalog) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            SharedPlugin {
                role: NetworkRole::Host,
                content,
            },
            ServerPlugin {
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                sleep_rule: SleepRule::Everyone,
                start_time: WorldTime::FIRST_DAWN,
                minute_length: GAME_MINUTE,
            },
        ))
        .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        app.finish();
        app.cleanup();
        app.update();

        let server = app
            .world_mut()
            .query_filtered::<Entity, With<Server>>()
            .single(app.world())
            .expect("the server is listening");
        let host = app.world_mut().spawn(network::host_client(server)).id();
        app.world_mut().trigger(Connect { entity: host });

        let mut world = Self { app };
        world.run_until("the host to control a character", |world| {
            world.character().is_some()
        });
        world
    }

    fn character(&mut self) -> Option<(Vec3, Belongings)> {
        let world = self.app.world_mut();
        world
            .query_filtered::<(&Position, &Belongings), With<InputMarker<PlayerInput>>>()
            .iter(world)
            .next()
            .map(|(position, belongings)| (position.0, belongings.clone()))
    }

    fn clock(&mut self) -> WorldTime {
        let world = self.app.world_mut();
        world
            .query::<&WorldClock>()
            .single(world)
            .expect("the world has a clock")
            .0
    }

    /// The middle of the field square two meters ahead of the character, on
    /// the ground.
    fn ground_ahead(&mut self) -> Vec3 {
        let (feet, _) = self.character().expect("the host has a character");
        let center = tile_center(tile_at(feet + Vec3::new(0.0, 0.0, -2.0)));
        let height = self
            .app
            .world()
            .resource::<Terrain>()
            .surface_below(Vec3::new(center.x, feet.y + 3.0, center.y), 6.0)
            .expect("ground ahead");
        Vec3::new(center.x, height, center.y)
    }

    fn slot_holding(&mut self, content: &Catalog, key: &str) -> u8 {
        let item = content.id(key).expect("the item exists");
        let (_, belongings) = self.character().expect("the host has a character");
        let slot = (0..10)
            .find(|&slot| {
                belongings
                    .0
                    .slot(slot)
                    .is_some_and(|stack| stack.item == item)
            })
            .unwrap_or_else(|| panic!("{key} is in the hotbar"));
        u8::try_from(slot).expect("hotbar slots fit in u8")
    }

    fn carried(&mut self, item: ItemId) -> u32 {
        self.character()
            .expect("the host has a character")
            .1
            .0
            .count(item)
    }

    fn field(&mut self, tile: IVec2) -> Option<Entity> {
        let world = self.app.world_mut();
        world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| field.tile == tile)
            .map(|(entity, _)| entity)
    }

    fn crop(&mut self, tile: IVec2) -> Option<Planting> {
        let field = self.field(tile)?;
        self.app.world().get::<Crop>(field).map(|crop| crop.0)
    }

    fn is_watered(&mut self, tile: IVec2) -> bool {
        self.field(tile)
            .is_some_and(|field| self.app.world().get::<Watered>(field).is_some())
    }

    fn use_item(&mut self, slot: u8, target: Vec3) {
        self.send(UseItem {
            slot,
            action: ItemAction::Primary,
            target: Some(target),
        });
        self.run(PAUSE_BETWEEN_USES);
    }

    fn harvest(&mut self, target: Vec3) {
        self.send(HarvestRequest { target });
        self.run(PAUSE_BETWEEN_USES);
    }

    /// Jumps to 21:00, goes to bed and waits for the next dawn.
    fn sleep_through_the_night(&mut self) {
        let today = self.clock().day();
        let bedtime =
            WorldTime::at(today, "21:00".parse().expect("valid time")).expect("within the day");
        let world = self.app.world_mut();
        world
            .query::<&mut WorldClock>()
            .single_mut(world)
            .expect("the world has a clock")
            .0 = bedtime;
        self.send(SleepRequest::Sleep);
        self.run_until("the next day to begin", |world| {
            world.clock().day() == today + 1
        });
    }

    fn send<M: lightyear::prelude::Message>(&mut self, message: M) {
        let world = self.app.world_mut();
        world
            .query_filtered::<&mut MessageSender<M>, With<Client>>()
            .single_mut(world)
            .expect("the host's client connection")
            .send::<ActionChannel>(message);
    }

    fn run(&mut self, updates: u32) {
        for _ in 0..updates {
            self.app.update();
        }
    }

    fn run_until(&mut self, waiting_for: &str, mut done: impl FnMut(&mut Self) -> bool) {
        for _ in 0..PATIENCE {
            if done(self) {
                return;
            }
            self.app.update();
        }
        panic!("timed out waiting for {waiting_for}");
    }
}
