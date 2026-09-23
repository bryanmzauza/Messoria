use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use bevy::prelude::*;
use clap::Parser;
use messoria_calendar::{SleepRule, WorldTime};
use messoria_client::{ClientPlugin, Session};
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    content::load_content,
    network::{self, DEFAULT_PORT, NetworkRole},
};

/// Messoria. Without options, starts a private local world.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Join the world at this address (IP or host name, optionally with a port).
    #[arg(long, value_name = "ADDRESS", value_parser = parse_server_addr, conflicts_with = "host")]
    connect: Option<SocketAddr>,

    /// Open the local world to other players.
    #[arg(long)]
    host: bool,

    /// UDP port to accept other players on.
    #[arg(long, default_value_t = DEFAULT_PORT, requires = "host")]
    port: u16,

    /// Delay packets from the server by this many milliseconds.
    #[arg(long, value_name = "MS", requires = "connect")]
    simulate_latency: Option<u64>,
}

fn main() -> AppExit {
    let args = Args::parse();
    let content = match load_content() {
        Ok(content) => content,
        Err(error) => {
            eprintln!("error: {error}");
            return AppExit::error();
        }
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Messoria".into(),
            ..default()
        }),
        ..default()
    }));

    if let Some(server_addr) = args.connect {
        app.add_plugins((
            SharedPlugin {
                role: NetworkRole::Client,
                content,
            },
            ClientPlugin {
                session: Session::Join {
                    server_addr,
                    simulated_latency: args.simulate_latency.map(Duration::from_millis),
                },
            },
        ));
    } else {
        let bind_addr = if args.host {
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.port))
        } else {
            // A private world only listens on loopback, on whatever port is free.
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
        };
        app.add_plugins((
            SharedPlugin {
                role: NetworkRole::Host,
                content,
            },
            // A world hosted for friends waits until everyone is asleep.
            ServerPlugin {
                bind_addr,
                sleep_rule: SleepRule::Everyone,
                start_time: WorldTime::FIRST_DAWN,
            },
            ClientPlugin {
                session: Session::Host,
            },
        ));
    }

    app.run()
}

fn parse_server_addr(input: &str) -> Result<SocketAddr, String> {
    network::resolve_server_addr(input).map_err(|error| error.to_string())
}
