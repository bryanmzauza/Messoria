use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use clap::Parser;
use messoria_calendar::{ClockTime, GAME_MINUTE, SleepRule, WorldTime};
use messoria_save::SaveDir;
use messoria_server::{ServerPlugin, WorldSetup};
use messoria_shared::{
    SharedPlugin,
    content::load_content,
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

    /// Folder the world is saved in. The world saved there is resumed, or a
    /// new one is created.
    #[arg(long, value_name = "FOLDER", default_value = "saves/world")]
    world: PathBuf,

    /// Time of day a new world starts at, between 06:00 and 01:59. A saved
    /// world resumes at the time it was saved.
    #[arg(long, value_name = "HH:MM", default_value = "06:00", value_parser = parse_start_time)]
    start_time: WorldTime,

    /// Real milliseconds one game minute lasts, to make days pass faster for
    /// testing. Defaults to normal play.
    #[arg(long, value_name = "MS")]
    minute_length: Option<u64>,
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
    let world = match WorldSetup::open(SaveDir::new(args.world), &content, args.start_time) {
        Ok(world) => world,
        Err(error) => {
            eprintln!("error: the saved world cannot be loaded: {error}");
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration())),
            LogPlugin::default(),
            bevy::app::TerminalCtrlCHandlerPlugin,
            SharedPlugin {
                role: NetworkRole::Server,
                content,
            },
            ServerPlugin {
                bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.port)),
                sleep_rule: SleepRule::Share {
                    percent: args.sleep_percent,
                },
                minute_length: args
                    .minute_length
                    .map_or(GAME_MINUTE, Duration::from_millis),
                world,
            },
        ))
        .run()
}

fn parse_start_time(text: &str) -> Result<WorldTime, String> {
    let clock: ClockTime = text.parse().map_err(|error| format!("{error}"))?;
    WorldTime::at(0, clock).ok_or_else(|| "no day runs between 02:00 and 06:00".to_owned())
}
