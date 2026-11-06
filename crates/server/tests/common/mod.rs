//! A game world hosted in-process, with the host playing in it, for tests
//! that play through the game by sending the messages a player's client
//! sends. Time runs one simulation tick per update, and nights are skipped
//! by going to sleep.
//!
//! Also a dedicated server and remote clients talking over loopback UDP in
//! real time, for tests of what crosses the network.

// Each test file uses a different part of this harness.
#![allow(dead_code)]

use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    thread,
    time::{Duration, Instant},
};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use lightyear::prelude::{input::native::InputMarker, server::Server, *};
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_content::{Catalog, ItemId, Purpose, Quality};
use messoria_farming::Planting;
use messoria_server::{ServerPlugin, WorldSetup};
use messoria_shared::{
    SharedPlugin,
    content::{Content, load_content},
    fields::{tile_at, tile_center},
    lots::{self, Lot, LotId},
    network::{self, NetworkRole},
    protocol::{
        ActionChannel, Belongings, ClaimLot, Crop, Deal, Dialogue, Field, Friendships, Happening,
        HarvestRequest, ItemAction, LotClaim, Money, Notice, PlayerInput, Position, Shopfront,
        SleepRequest, Structure, Trade, UseItem, Watered, WorldClock,
    },
    scenery::Scenery,
    terrain::Terrain,
    valley::Valley,
};
use messoria_worldgen::column_of;

/// Real time the apps talking over the network get to reach a state.
const TIMEOUT: Duration = Duration::from_secs(20);

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
    pub dialogues: Vec<Dialogue>,
    pub friendships: Vec<Friendships>,
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

    /// The village notice board.
    pub fn board(&mut self, content: &Catalog) -> Structure {
        let world = self.app.world_mut();
        *world
            .query::<&Structure>()
            .iter(world)
            .find(|structure| content.structure(structure.kind).purpose == Purpose::NoticeBoard)
            .expect("the village has a notice board")
    }

    /// Puts the host's character in front of the notice board.
    pub fn go_to_board(&mut self, content: &Catalog) {
        let board = self.board(content);
        self.teleport(board.to_world(Vec3::new(0.0, 0.0, -1.3)));
    }

    /// Every claimed lot.
    pub fn claims(&mut self) -> Vec<LotClaim> {
        let world = self.app.world_mut();
        world.query::<&LotClaim>().iter(world).cloned().collect()
    }

    /// Claims `lot` at the notice board, as the host, and waits for the
    /// server's answer: the claim, or a notice.
    pub fn claim(&mut self, content: &Catalog, lot: LotId) {
        self.go_to_board(content);
        let (claims, notices) = (self.claims().len(), self.heard().notices.len());
        self.send(ClaimLot { lot });
        self.run_until("the claim to be answered", |world| {
            world.claims().len() > claims || world.heard().notices.len() > notices
        });
        // What the claim showed follows a frame behind.
        self.run(2);
    }

    /// Claims the first free lot at the notice board and walks to its
    /// middle, where the ground can be worked.
    pub fn settle_on_a_lot(&mut self, content: &Catalog) -> Lot {
        let claimed: Vec<LotId> = self.claims().iter().map(|claim| claim.lot).collect();
        let lot = lots::lots()
            .find(|lot| !claimed.contains(&lot.id))
            .expect("a free lot");
        self.claim(content, lot.id);
        assert!(
            self.claims().iter().any(|claim| claim.lot == lot.id),
            "the lot was claimed"
        );
        self.teleport_to(lot.center());
        lot
    }

    /// Moves the host's character onto the ground at `place`.
    pub fn teleport_to(&mut self, place: Vec2) {
        let feet = self.ground_at(place);
        self.teleport(feet);
    }

    /// The ground at `place`, as the valley grew it.
    pub fn ground_at(&mut self, place: Vec2) -> Vec3 {
        let height = self.app.world().resource::<Valley>().height(place);
        Vec3::new(place.x, height, place.y)
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

/// A dedicated server listening on `bind_addr`, driven by hand.
pub fn server_app(bind_addr: SocketAddr, world: WorldSetup) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Server,
            content: load_content().expect("the shipped content is valid"),
        },
        ServerPlugin {
            bind_addr,
            sleep_rule: SleepRule::Everyone,
            minute_length: GAME_MINUTE,
            world,
        },
    ));
    ready(app)
}

/// A remote client of the server at `server_addr`, as player `client_id`,
/// driven by hand, keeping what it hears in `Heard`.
pub fn client_app(server_addr: SocketAddr, client_id: u64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Client,
            content: load_content().expect("the shipped content is valid"),
        },
    ))
    .init_resource::<Heard>()
    .add_systems(PreUpdate, listen.after(MessageSystems::Receive));
    let client = app
        .world_mut()
        .spawn(network::remote_client(server_addr, client_id, None).expect("valid connect token"))
        .id();
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.trigger(Connect { entity: client });
    });
    ready(app)
}

/// Completes plugin setup the way `App::run` would, since the tests drive
/// updates by hand.
pub fn ready(mut app: App) -> App {
    app.finish();
    app.cleanup();
    app
}

/// Asks the OS for a UDP port that is free right now.
pub fn free_udp_port() -> u16 {
    UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|socket| socket.local_addr())
        .expect("bind an ephemeral UDP port")
        .port()
}

/// Feet of the character `client` controls, once it has one.
pub fn local_character_if_any(client: &mut App) -> Option<Vec3> {
    client
        .world_mut()
        .query_filtered::<&Position, With<InputMarker<PlayerInput>>>()
        .iter(client.world())
        .next()
        .map(|position| position.0)
}

/// Updates the server and every client in lockstep, in real time, until
/// `done` holds.
pub fn run_apps_until(
    server: &mut App,
    clients: &mut [&mut App],
    waiting_for: &str,
    mut done: impl FnMut(&mut App, &mut [&mut App]) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    while !done(server, clients) {
        if Instant::now() >= deadline {
            let seen: Vec<Option<Vec3>> = clients
                .iter_mut()
                .map(|client| local_character_if_any(client))
                .collect();
            panic!(
                "timed out waiting for {waiting_for}; the clients see their characters at {seen:?}"
            );
        }
        server.update();
        for client in clients.iter_mut() {
            client.update();
        }
        thread::sleep(Duration::from_millis(2));
    }
}

/// Keeps what a client receives, which lightyear would otherwise drop at
/// the end of the frame. Each receiver appears with its first message.
fn listen(
    mut notices: Query<&mut MessageReceiver<Notice>, With<Client>>,
    mut happenings: Query<&mut MessageReceiver<Happening>, With<Client>>,
    mut dialogues: Query<&mut MessageReceiver<Dialogue>, With<Client>>,
    mut friendships: Query<&mut MessageReceiver<Friendships>, With<Client>>,
    mut heard: ResMut<Heard>,
) {
    for mut receiver in &mut notices {
        heard.notices.extend(receiver.receive());
    }
    for mut receiver in &mut happenings {
        heard.happenings.extend(receiver.receive());
    }
    for mut receiver in &mut dialogues {
        heard.dialogues.extend(receiver.receive());
    }
    for mut receiver in &mut friendships {
        heard.friendships.extend(receiver.receive());
    }
}
