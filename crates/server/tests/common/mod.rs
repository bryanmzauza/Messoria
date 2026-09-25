//! A game world hosted in-process, with the host playing in it, for tests
//! that play through the game by sending the messages a player's client
//! sends. Time runs one simulation tick per update, and nights are skipped
//! by going to sleep.

// Each test file uses a different part of this harness.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use lightyear::prelude::{input::native::InputMarker, server::Server, *};
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_content::{Catalog, ItemId, Purpose, Quality};
use messoria_farming::Planting;
use messoria_server::{ServerPlugin, WorldSetup};
use messoria_shared::{
    SharedPlugin,
    content::Content,
    fields::{tile_at, tile_center},
    network::{self, NetworkRole},
    protocol::{
        ActionChannel, Belongings, Crop, Deal, Field, Happening, HarvestRequest, ItemAction, Money,
        Notice, PlayerInput, Position, Shopfront, SleepRequest, Structure, Trade, UseItem, Watered,
        WorldClock,
    },
    scenery::Scenery,
    terrain::Terrain,
};
use messoria_worldgen::column_of;

/// Updates to run while waiting for something; far more than it takes.
const PATIENCE: u32 = 2_000;
/// Updates between two item uses: more than the server's minimum interval
/// between uses at one tick per update.
pub const PAUSE_BETWEEN_USES: u32 = 10;

/// What the host's client was told since the world began.
#[derive(Resource, Default)]
pub struct Heard {
    pub notices: Vec<Notice>,
    pub happenings: Vec<Happening>,
}

/// A world hosted in-process, with the host playing in it.
pub struct HostedWorld {
    app: App,
    /// The world's time once it was set up, before any time passed.
    pub started_at: WorldTime,
}

impl HostedWorld {
    pub fn new(content: Catalog) -> Self {
        Self::with_world(content, WorldSetup::fresh(1, WorldTime::FIRST_DAWN))
    }

    pub fn with_world(content: Catalog, world: WorldSetup) -> Self {
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
                minute_length: GAME_MINUTE,
                world,
            },
        ))
        .insert_resource(TimeUpdateStrategy::FixedTimesteps(1))
        .init_resource::<Heard>()
        .add_systems(PreUpdate, listen.after(MessageSystems::Receive));
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

        let mut world = Self {
            app,
            started_at: WorldTime::FIRST_DAWN,
        };
        world.started_at = world.clock();
        world.run_until("the host to control a character", |world| {
            world.character().is_some()
        });
        world.run_until("the ground around the host to load", Self::settled);
        world
    }

    /// Whether the terrain and the scenery around the host's character are
    /// loaded, which the server does over a few updates after it moves far.
    fn settled(&mut self) -> bool {
        let (feet, _) = self.character().expect("the host has a character");
        let world = self.app.world();
        let column = column_of(feet.xz());
        let around = (-1..=1).flat_map(|z| (-1..=1).map(move |x| column + IVec2::new(x, z)));
        world.resource::<Terrain>().distance(feet).is_some()
            && around
                .into_iter()
                .all(|column| world.resource::<Scenery>().is_loaded(column))
    }

    pub fn character(&mut self) -> Option<(Vec3, Belongings)> {
        let world = self.app.world_mut();
        world
            .query_filtered::<(&Position, &Belongings), With<InputMarker<PlayerInput>>>()
            .iter(world)
            .next()
            .map(|(position, belongings)| (position.0, belongings.clone()))
    }

    pub fn clock(&mut self) -> WorldTime {
        let world = self.app.world_mut();
        world
            .query::<&WorldClock>()
            .single(world)
            .expect("the world has a clock")
            .0
    }

    /// The middle of the field square two meters ahead of the character, on
    /// the ground.
    pub fn ground_ahead(&mut self) -> Vec3 {
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

    pub fn slot_holding(&mut self, content: &Catalog, key: &str) -> u8 {
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

    pub fn carried(&mut self, item: ItemId) -> u32 {
        self.character()
            .expect("the host has a character")
            .1
            .0
            .count(item)
    }

    pub fn field(&mut self, tile: IVec2) -> Option<Entity> {
        let world = self.app.world_mut();
        world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| field.tile == tile)
            .map(|(entity, _)| entity)
    }

    pub fn crop(&mut self, tile: IVec2) -> Option<Planting> {
        let field = self.field(tile)?;
        self.app.world().get::<Crop>(field).map(|crop| crop.0)
    }

    pub fn is_watered(&mut self, tile: IVec2) -> bool {
        self.field(tile)
            .is_some_and(|field| self.app.world().get::<Watered>(field).is_some())
    }

    pub fn use_item(&mut self, slot: u8, target: Vec3) {
        self.send(UseItem {
            slot,
            action: ItemAction::Primary,
            target: Some(target),
        });
        self.run(PAUSE_BETWEEN_USES);
    }

    pub fn harvest(&mut self, target: Vec3) {
        self.send(HarvestRequest { target });
        self.run(PAUSE_BETWEEN_USES);
    }

    /// Jumps to 21:00, goes to bed and waits for the next dawn. A bed is
    /// put beside the character if none is near.
    pub fn sleep_through_the_night(&mut self) {
        self.bed_nearby();
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

    /// Puts a bed behind the host's character, out of the way of the ground
    /// ahead, unless one stands near.
    pub fn bed_nearby(&mut self) {
        let (feet, _) = self.character().expect("the host has a character");
        let world = self.app.world_mut();
        let bed = world
            .resource::<Content>()
            .structures()
            .find(|(_, structure)| structure.purpose == Purpose::Bed)
            .map(|(id, _)| id)
            .expect("the content has a bed");
        let near = world
            .query::<&Structure>()
            .iter(world)
            .any(|structure| structure.kind == bed && structure.position.distance(feet) < 1.8);
        if !near {
            world.spawn(Structure {
                kind: bed,
                position: feet + Vec3::new(0.0, 0.0, 1.6),
                facing: 0.0,
            });
        }
    }

    /// Sets the world's clock to `clock` on the current day.
    pub fn set_time(&mut self, clock: &str) {
        let today = self.clock().day();
        let time =
            WorldTime::at(today, clock.parse().expect("valid time")).expect("within the day");
        let world = self.app.world_mut();
        world
            .query::<&mut WorldClock>()
            .single_mut(world)
            .expect("the world has a clock")
            .0 = time;
    }

    /// Moves the host's character to `feet`.
    pub fn teleport(&mut self, feet: Vec3) {
        let world = self.app.world_mut();
        world
            .query_filtered::<&mut Position, With<InputMarker<PlayerInput>>>()
            .single_mut(world)
            .expect("the host has a character")
            .0 = feet;
        self.run_until("the ground around the host to load", Self::settled);
        self.run(PAUSE_BETWEEN_USES);
    }

    /// Puts `count` of `item` in the host's inventory, as if gathered earlier.
    pub fn give(&mut self, content: &Catalog, key: &str, count: u16) {
        let item = content.id(key).expect("the item exists");
        let day = self.clock().day();
        let world = self.app.world_mut();
        let left = world
            .query_filtered::<&mut Belongings, With<InputMarker<PlayerInput>>>()
            .single_mut(world)
            .expect("the host has a character")
            .0
            .add(content, item, Quality::Normal, count, day);
        assert_eq!(left, 0, "{key} fits in the inventory");
    }

    pub fn coins(&mut self) -> u32 {
        let world = self.app.world_mut();
        world
            .query_filtered::<&Money, With<InputMarker<PlayerInput>>>()
            .single(world)
            .expect("the host has a character")
            .0
            .coins()
    }

    /// The first inventory slot holding `item`.
    pub fn slot_of(&mut self, item: ItemId) -> Option<u8> {
        let (_, belongings) = self.character()?;
        (0..messoria_inventory::SLOTS)
            .find(|&slot| {
                belongings
                    .0
                    .slot(slot)
                    .is_some_and(|stack| stack.item == item)
            })
            .and_then(|slot| u8::try_from(slot).ok())
    }

    /// The stall of the shop with id `shop`.
    pub fn stall(&mut self, content: &Catalog, shop: &str) -> Shopfront {
        let shop = content.shop_id(shop).expect("the shop exists");
        let world = self.app.world_mut();
        *world
            .query::<&Shopfront>()
            .iter(world)
            .find(|stall| stall.shop == shop)
            .expect("the shop has a stall")
    }

    /// Puts the host's character in front of the counter of `shop`'s stall.
    pub fn go_to_stall(&mut self, content: &Catalog, shop: &str) -> Shopfront {
        let stall = self.stall(content, shop);
        let front = Quat::from_rotation_y(stall.facing) * Vec3::new(0.0, 0.0, -1.5);
        self.teleport(stall.position + front);
        stall
    }

    pub fn trade(&mut self, content: &Catalog, shop: &str, deal: Deal) {
        let shop = content.shop_id(shop).expect("the shop exists");
        self.send(Trade { shop, deal });
        self.run(PAUSE_BETWEEN_USES);
    }

    pub fn heard(&self) -> &Heard {
        self.app.world().resource::<Heard>()
    }

    pub fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Asks the app to exit, as closing the game or stopping the server
    /// does, and runs the frame in which it would.
    pub fn stop(&mut self) {
        self.app.world_mut().write_message(AppExit::Success);
        self.app.update();
    }

    pub fn send<M: lightyear::prelude::Message>(&mut self, message: M) {
        let world = self.app.world_mut();
        world
            .query_filtered::<&mut MessageSender<M>, With<Client>>()
            .single_mut(world)
            .expect("the host's client connection")
            .send::<ActionChannel>(message);
    }

    pub fn run(&mut self, updates: u32) {
        for _ in 0..updates {
            self.app.update();
        }
    }

    pub fn run_until(&mut self, waiting_for: &str, mut done: impl FnMut(&mut Self) -> bool) {
        for _ in 0..PATIENCE {
            if done(self) {
                return;
            }
            self.app.update();
        }
        panic!("timed out waiting for {waiting_for}");
    }
}

/// Keeps what the host's client receives, which lightyear would otherwise
/// drop at the end of the frame. Each receiver appears with its first message.
fn listen(
    mut notices: Query<&mut MessageReceiver<Notice>, With<Client>>,
    mut happenings: Query<&mut MessageReceiver<Happening>, With<Client>>,
    mut heard: ResMut<Heard>,
) {
    for mut receiver in &mut notices {
        heard.notices.extend(receiver.receive());
    }
    for mut receiver in &mut happenings {
        heard.happenings.extend(receiver.receive());
    }
}
