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
use messoria_calendar::{SleepRule, WorldTime};
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    energy::Energy,
    movement::EYE_HEIGHT,
    network::{self, NetworkRole},
    protocol::{
        ActionChannel, Asleep, PlayerId, PlayerInput, Position, ShovelAction, ShovelRequest,
        SleepRequest, WorldClock,
    },
    shovel,
    terrain::Terrain,
};

const TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn client_controls_its_own_predicted_character() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr, Behavior::WalkForward);

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
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<ShovelRequest>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(ShovelRequest {
            target,
            action: ShovelAction::Dig,
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

enum Behavior {
    StandStill,
    WalkForward,
}

fn server_app(bind_addr: SocketAddr) -> App {
    server_app_starting_at(bind_addr, WorldTime::FIRST_DAWN)
}

fn server_app_starting_at(bind_addr: SocketAddr, start_time: WorldTime) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Server,
        },
        ServerPlugin {
            bind_addr,
            sleep_rule: SleepRule::Everyone,
            start_time,
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
        },
    ));
    if let Behavior::WalkForward = behavior {
        app.add_systems(
            FixedPreUpdate,
            walk_forward.in_set(InputSystems::WriteClientInputs),
        );
    }

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

fn walk_forward(mut inputs: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>) {
    for mut input in &mut inputs {
        input.0 = PlayerInput {
            movement: Vec2::Y,
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
        .raycast(eyes, direction, shovel::REACH)?;
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
