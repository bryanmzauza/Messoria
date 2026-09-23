//! End-to-end check of the networked skeleton: a dedicated server and a
//! headless client in one process, talking over loopback UDP.

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
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    network::{self, NetworkRole},
    protocol::{PlayerId, PlayerInput, Position},
};

const TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn client_controls_its_own_predicted_character() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = server_app(server_addr);
    let mut client = client_app(server_addr);

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

    run_until(
        &mut server,
        &mut client,
        "server to move the character forward",
        |server, _| {
            server
                .world_mut()
                .query::<&Position>()
                .iter(server.world())
                .any(|position| position.0.z < -1.0)
        },
    );
}

fn server_app(bind_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Server,
        },
        ServerPlugin { bind_addr },
    ));
    ready(app)
}

fn client_app(server_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SharedPlugin {
            role: NetworkRole::Client,
        },
    ))
    .add_systems(
        FixedPreUpdate,
        walk_forward.in_set(InputSystems::WriteClientInputs),
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

/// Completes plugin setup the way `App::run` would, since the test drives
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
