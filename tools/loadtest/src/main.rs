//! Connects headless bot players to a server and lets them wander.
//!
//! Each bot is a complete client app running on its own thread, so the server
//! sees exactly the traffic real players would produce.

mod wander;

use std::{net::SocketAddr, thread, time::Duration};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use clap::Parser;
use lightyear::prelude::*;
use messoria_shared::{
    SharedPlugin,
    network::{self, NetworkRole},
    tick::tick_duration,
};

/// Simulated players for load testing a Messoria server.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Server to connect to (IP or host name, optionally with a port).
    #[arg(long, default_value = "127.0.0.1", value_parser = parse_server_addr)]
    server: SocketAddr,

    /// Number of bots to connect.
    #[arg(long, default_value_t = 1)]
    bots: u16,

    /// Delay packets from the server by this many milliseconds.
    #[arg(long, value_name = "MS")]
    simulate_latency: Option<u64>,
}

fn main() {
    let args = Args::parse();
    let simulated_latency = args.simulate_latency.map(Duration::from_millis);

    let bots: Vec<_> = (0..args.bots)
        .map(|index| {
            thread::Builder::new()
                .name(format!("bot-{index}"))
                .spawn(move || run_bot(index, args.server, simulated_latency))
                .expect("spawn bot thread")
        })
        .collect();

    for bot in bots {
        if let Err(panic) = bot.join() {
            std::panic::resume_unwind(panic);
        }
    }
}

fn run_bot(index: u16, server_addr: SocketAddr, simulated_latency: Option<Duration>) -> AppExit {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration())));
    // The log subscriber is process-wide, so only one bot installs it.
    if index == 0 {
        app.add_plugins(LogPlugin::default());
    }
    app.add_plugins((
        SharedPlugin {
            role: NetworkRole::Client,
        },
        wander::WanderPlugin {
            seed: u64::from(index),
        },
    ));

    let client_id = rand::random();
    let connection = network::remote_client(server_addr, client_id, simulated_latency)
        .expect("connect token for a resolved address");
    let client = app.world_mut().spawn(connection).id();
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.trigger(Connect { entity: client });
    });

    app.run()
}

fn parse_server_addr(input: &str) -> Result<SocketAddr, String> {
    network::resolve_server_addr(input).map_err(|error| error.to_string())
}
