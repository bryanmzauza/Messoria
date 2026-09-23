use std::net::{Ipv4Addr, SocketAddr};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use clap::Parser;
use messoria_calendar::{ClockTime, SleepRule, WorldTime};
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

    /// Percentage of the players online that must sleep to end the day.
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u8).range(1..=100))]
    sleep_percent: u8,

    /// Time of day the world starts at, between 06:00 and 01:59.
    #[arg(long, value_name = "HH:MM", default_value = "06:00", value_parser = parse_start_time)]
    start_time: WorldTime,
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
                sleep_rule: SleepRule::Share {
                    percent: args.sleep_percent,
                },
                start_time: args.start_time,
            },
        ))
        .run()
}

fn parse_start_time(text: &str) -> Result<WorldTime, String> {
    let clock: ClockTime = text.parse().map_err(|error| format!("{error}"))?;
    WorldTime::at(0, clock).ok_or_else(|| "no day runs between 02:00 and 06:00".to_owned())
}
