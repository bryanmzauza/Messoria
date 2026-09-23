use std::net::{Ipv4Addr, SocketAddr};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use clap::Parser;
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    network::{DEFAULT_PORT, NetworkRole},
    tick::tick_duration,
};

/// Headless Messoria server.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// UDP port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
}

fn main() -> AppExit {
    let args = Args::parse();

    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration())),
            LogPlugin::default(),
            bevy::app::TerminalCtrlCHandlerPlugin,
            SharedPlugin {
                role: NetworkRole::Server,
            },
            ServerPlugin {
                bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.port)),
            },
        ))
        .run()
}
