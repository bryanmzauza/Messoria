//! End-to-end checks: a dedicated server and a headless client in one
//! process, talking over loopback UDP.

use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    thread,
    time::{Duration, Instant},
};

use bevy::prelude::*;
use lightyear::prelude::{
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
    *,
};
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_content::Purpose;
use messoria_server::{ServerPlugin, WorldSetup};
use messoria_shared::{
    SharedPlugin,
    content::{Content, load_content},
    energy::Energy,
    movement::EYE_HEIGHT,
    network::{self, NetworkRole},
    protocol::{
        ActionChannel, Asleep, Belongings, Deal, Field, ItemAction, Money, PlayerId, PlayerInput,
        Position, Shopfront, SleepRequest, Structure, Trade, UseItem, WorldClock,
    },
    scenery::Scenery,
    terrain::Terrain,
    tools,
    valley::Valley,
};
use messoria_worldgen::column_of;

const TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn client_controls_its_own_predicted_character() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::Walk);

    run_until(
        &mut server,
        &mut client,
        "client to control a predicted character",
        |_, client| {
            client
                .world_mut()
                .query_filtered::<(), (
                    With<PlayerId>,
                    With<Predicted>,
                    With<InputMarker<PlayerInput>>,
                )>()
                .iter(client.world())
                .count()
                == 1
        },
    );

    let start = local_character(&mut client);
    run_until(
        &mut server,
        &mut client,
        "server to move the character forward",
        |server, _| {
            server
                .world_mut()
                .query::<&Position>()
                .iter(server.world())
                .any(|position| position.0.z < start.z - 1.0)
        },
    );
}

/// Once a character stops, its owner must see it exactly where the server
/// has it, even if the server did not act on every input the client
/// predicted with.
#[test]
fn prediction_settles_where_the_server_has_the_character() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::Walk);

    run_until(
        &mut server,
        &mut client,
        "client to control a character",
        |_, client| local_character_if_any(client).is_some(),
    );
    let start = local_character(&mut client);
    run_until(
        &mut server,
        &mut client,
        "client to walk a few meters",
        |_, client| local_character(client).distance(start) > 3.0,
    );
    client.world_mut().resource_mut::<Walking>().0 = false;

    run_until(
        &mut server,
        &mut client,
        "the predicted character to settle where the server has it",
        |server, client| {
            let authoritative = server
                .world_mut()
                .query_filtered::<&Position, With<PlayerId>>()
                .single(server.world())
                .expect("one character")
                .0;
            local_character(client).distance(authoritative) < 0.05
        },
    );
}

#[test]
fn shovel_edits_reach_the_client_identically() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::StandStill);

    let mut target = None;
    run_until(
        &mut server,
        &mut client,
        "client to receive the ground in front of its character",
        |_, client| {
            target = aim_ahead(client);
            target.is_some()
        },
    );
    let target = target.expect("found above");

    let before = server.world().resource::<Terrain>().0.clone();
    // Players start with the shovel in their first hotbar slot.
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<UseItem>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(UseItem {
            slot: 0,
            action: ItemAction::Primary,
            target: Some(target),
        });

    run_until(
        &mut server,
        &mut client,
        "the dug terrain to reach the client",
        |server, client| {
            let server_terrain = server.world().resource::<Terrain>();
            let client_terrain = client.world().resource::<Terrain>();
            let dug = before
                .positions()
                .any(|chunk| before.get(chunk) != server_terrain.get(chunk));
            dug && client_terrain
                .positions()
                .all(|chunk| client_terrain.get(chunk) == server_terrain.get(chunk))
        },
    );

    let soil = load_content()
        .expect("the shipped content is valid")
        .id("soil")
        .expect("soil exists");
    run_until(
        &mut server,
        &mut client,
        "the dug soil to show up in the client's inventory",
        |_, client| {
            let world = client.world_mut();
            world
                .query_filtered::<&Belongings, With<InputMarker<PlayerInput>>>()
                .iter(world)
                .next()
                .is_some_and(|belongings| belongings.0.count(soil) == 1)
        },
    );
}

#[test]
fn a_purchase_made_over_the_network_reaches_the_buyer() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::StandStill);
    run_until(
        &mut server,
        &mut client,
        "client to control a character",
        |_, client| local_character_if_any(client).is_some(),
    );

    // Open the grocer and put the character at its stall.
    let world = server.world_mut();
    world
        .query::<&mut WorldClock>()
        .single_mut(world)
        .expect("the world has a clock")
        .0 = WorldTime::at(0, "10:00".parse().expect("valid time")).expect("within the day");
    let content = load_content().expect("the shipped content is valid");
    let grocer = content.shop_id("grocer").expect("the grocer exists");
    let stall = *world
        .query::<&Shopfront>()
        .iter(world)
        .find(|stall| stall.shop == grocer)
        .expect("the grocer has a stall");
    world
        .query_filtered::<&mut Position, With<PlayerId>>()
        .single_mut(world)
        .expect("one character")
        .0 = stall.position + Quat::from_rotation_y(stall.facing) * Vec3::new(0.0, 0.0, -1.5);

    let seeds = content.id("turnip_seeds").expect("turnip seeds exist");
    let price = content
        .shop(stall.shop)
        .listing(seeds)
        .expect("the grocer sells turnip seeds")
        .price;
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<Trade>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(Trade {
            shop: stall.shop,
            deal: Deal::Buy {
                item: seeds,
                count: 2,
            },
        });

    run_until(
        &mut server,
        &mut client,
        "the client to see its money spent and its seeds",
        |_, client| {
            let world = client.world_mut();
            world
                .query_filtered::<(&Money, &Belongings), With<InputMarker<PlayerInput>>>()
                .iter(world)
                .next()
                .is_some_and(|(money, belongings)| {
                    money.0.coins() == content.starting_money() - 2 * price
                        && belongings.0.count(seeds) >= 2
                })
        },
    );
}

#[test]
fn sleeping_through_the_night_starts_a_new_day() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let bedtime = WorldTime::at(0, "21:00".parse().expect("valid time")).expect("within the day");
    let mut server = server_app_starting_at(server_addr, bedtime);
    let mut client = client_app(server_addr, Behavior::StandStill);

    run_until(
        &mut server,
        &mut client,
        "client to control a character",
        |_, client| local_character_if_any(client).is_some(),
    );
    // Characters sleep in beds; one stands beside this one.
    let world = server.world_mut();
    let feet = world
        .query_filtered::<&Position, With<PlayerId>>()
        .single(world)
        .expect("one character")
        .0;
    let bed = world
        .resource::<Content>()
        .structures()
        .find(|(_, structure)| structure.purpose == Purpose::Bed)
        .map(|(id, _)| id)
        .expect("the content has a bed");
    world.spawn(Structure {
        kind: bed,
        position: feet + Vec3::new(0.0, 0.0, 1.6),
        facing: 0.0,
    });
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<SleepRequest>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(SleepRequest::Sleep);

    run_until(
        &mut server,
        &mut client,
        "the client to wake up rested on the next day",
        |_, client| {
            let world = client.world_mut();
            let day = world
                .query::<&WorldClock>()
                .iter(world)
                .next()
                .map(|clock| clock.0.day());
            let rested = world
                .query_filtered::<(&Energy, Has<Asleep>), With<InputMarker<PlayerInput>>>()
                .iter(world)
                .next()
                .is_some_and(|(energy, asleep)| *energy == Energy::FULL && !asleep);
            day == Some(1) && rested
        },
    );
}

/// A client learns the valley from the seed and grows the scenery of the
/// terrain it is sent, and is sent only what stands near its character.
#[test]
fn a_client_is_sent_the_valley_and_what_stands_near_it() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::StandStill);
    run_until(
        &mut server,
        &mut client,
        "the client to grow the scenery around its character",
        |_, client| {
            local_character_if_any(client).is_some_and(|feet| {
                client
                    .world()
                    .resource::<Scenery>()
                    .is_loaded(column_of(feet.xz()))
            })
        },
    );
    let seed = |app: &App| app.world().resource::<Valley>().seed();
    assert_eq!(seed(&client), seed(&server));

    let feet = local_character(&mut client);
    let field = |at: Vec3| Field {
        tile: at.xz().floor().as_ivec2(),
        height: at.y,
    };
    let (near, far) = (
        feet + Vec3::new(3.0, 0.0, 3.0),
        feet + Vec3::new(600.0, 0.0, 0.0),
    );
    for at in [near, far] {
        server
            .world_mut()
            .spawn((field(at), Replicate::to_clients(NetworkTarget::All)));
    }
    let has_field = |client: &mut App, at: Vec3| {
        let tile = field(at).tile;
        client
            .world_mut()
            .query::<&Field>()
            .iter(client.world())
            .any(|field| field.tile == tile)
    };
    run_until(
        &mut server,
        &mut client,
        "the near field to arrive",
        |_, client| has_field(client, near),
    );
    for _ in 0..60 {
        server.update();
        client.update();
    }
    assert!(!has_field(&mut client, far), "a field far away was sent");

    let world = server.world_mut();
    world
        .query_filtered::<&mut Position, With<PlayerId>>()
        .single_mut(world)
        .expect("one character")
        .0 = far + Vec3::new(2.0, 0.0, 2.0);
    run_until(
        &mut server,
        &mut client,
        "the far field to arrive and the near one to go",
        |_, client| has_field(client, far) && !has_field(client, near),
    );
}

enum Behavior {
    StandStill,
    /// Walk forward until [`Walking`] says otherwise.
    Walk,
}

/// Whether a client with [`Behavior::Walk`] walks.
#[derive(Resource)]
struct Walking(bool);

fn server_app(bind_addr: SocketAddr) -> App {
    server_app_starting_at(bind_addr, WorldTime::FIRST_DAWN)
}

fn server_app_starting_at(bind_addr: SocketAddr, start_time: WorldTime) -> App {
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
            world: WorldSetup::fresh(1, start_time),
        },
    ));
    ready(app)
}

fn client_app(server_addr: SocketAddr, behavior: Behavior) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Client,
            content: load_content().expect("the shipped content is valid"),
        },
    ));
    if let Behavior::Walk = behavior {
        app.insert_resource(Walking(true));
    }
    app.add_systems(
        FixedPreUpdate,
        walk.run_if(resource_exists::<Walking>)
            .in_set(InputSystems::WriteClientInputs),
    );

    let client = app
        .world_mut()
        .spawn(network::remote_client(server_addr, 1, None).expect("valid connect token"))
        .id();
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.trigger(Connect { entity: client });
    });
    ready(app)
}

/// Completes plugin setup the way `App::run` would, since the tests drive
/// updates by hand.
fn ready(mut app: App) -> App {
    app.finish();
    app.cleanup();
    app
}

fn walk(
    walking: Res<Walking>,
    mut inputs: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    for mut input in &mut inputs {
        input.0 = PlayerInput {
            movement: if walking.0 { Vec2::Y } else { Vec2::ZERO },
            ..default()
        };
    }
}

/// Feet of the character the client controls, once it has one.
fn local_character_if_any(client: &mut App) -> Option<Vec3> {
    client
        .world_mut()
        .query_filtered::<&Position, With<InputMarker<PlayerInput>>>()
        .iter(client.world())
        .next()
        .map(|position| position.0)
}

fn local_character(client: &mut App) -> Vec3 {
    local_character_if_any(client).expect("the client controls a character")
}

/// Where the client's shovel would hit the ground just ahead of its character,
/// once that terrain has arrived.
fn aim_ahead(client: &mut App) -> Option<Vec3> {
    let eyes = local_character_if_any(client)? + Vec3::Y * EYE_HEIGHT;
    let direction = Vec3::new(0.0, -1.0, -1.0).normalize();
    let hit = client
        .world()
        .resource::<Terrain>()
        .raycast(eyes, direction, tools::REACH)?;
    Some(hit.point)
}

/// Updates both apps in lockstep, in real time, until `done` holds.
fn run_until(
    server: &mut App,
    client: &mut App,
    waiting_for: &str,
    mut done: impl FnMut(&mut App, &mut App) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    while !done(server, client) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {waiting_for}"
        );
        server.update();
        client.update();
        thread::sleep(Duration::from_millis(2));
    }
}

/// Asks the OS for a UDP port that is free right now.
fn free_udp_port() -> u16 {
    UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|socket| socket.local_addr())
        .expect("bind an ephemeral UDP port")
        .port()
}
