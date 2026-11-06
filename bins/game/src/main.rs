use std::{
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

use bevy::prelude::*;
use clap::Parser;
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_client::{ClientPlugin, Session, TitlePlugin};
use messoria_save::{Profile, SaveDir};
use messoria_server::{ServerPlugin, WorldSetup};
use messoria_shared::{
    SharedPlugin,
    content::load_content,
    network::{self, DEFAULT_PORT, NetworkRole},
};

/// Where worlds are saved, each in a folder of its own.
const SAVES_FOLDER: &str = "saves";
/// Where the local player's identity is kept, shared by every world they join.
const PROFILE_FILE: &str = "saves/profile.ron";
/// Where the player's settings are kept.
const SETTINGS_FILE: &str = "saves/settings.ron";
/// Players a world hosted from the game takes at once.
const MAX_PLAYERS: u32 = 8;

/// Messoria. Without options, opens the title screen.
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Join the world at this address (IP or host name, optionally with a port).
    #[arg(long, value_name = "ADDRESS", value_parser = parse_server_addr, conflicts_with_all = ["host", "world"])]
    connect: Option<SocketAddr>,

    /// Open the local world to other players from the start.
    #[arg(long)]
    host: bool,

    /// Folder the local world is saved in. The world saved there is resumed,
    /// or a new one is created. Implied by --host, as `saves/local`.
    #[arg(long, value_name = "FOLDER")]
    world: Option<PathBuf>,

    /// UDP port to accept other players on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Seed of a new local world; a saved world keeps its own.
    #[arg(long, value_name = "NUMBER", conflicts_with = "connect")]
    seed: Option<u64>,

    /// Percentage of the players online that must sleep to end the day in
    /// a hosted world; everyone, if omitted.
    #[arg(long, value_name = "PERCENT", value_parser = clap::value_parser!(u8).range(1..=100))]
    sleep_percent: Option<u8>,

    /// Delay packets from the server by this many milliseconds.
    #[arg(long, value_name = "MS", requires = "connect")]
    simulate_latency: Option<u64>,

    /// Started by the title screen, which the game returns to on leaving.
    #[arg(long, hide = true)]
    from_title: bool,

    /// A word for the title screen to show.
    #[arg(long, hide = true, value_name = "TEXT")]
    notice: Option<String>,
}

fn main() -> AppExit {
    let args = Args::parse();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Messoria".into(),
            ..default()
        }),
        ..default()
    }));

    let world = args
        .world
        .clone()
        .or_else(|| args.host.then(|| PathBuf::from(SAVES_FOLDER).join("local")));
    if args.connect.is_none() && world.is_none() {
        app.add_plugins(TitlePlugin {
            saves: SAVES_FOLDER.into(),
            settings_file: SETTINGS_FILE.into(),
            notice: args.notice,
        });
        return app.run();
    }

    let content = match load_content() {
        Ok(content) => content,
        Err(error) => {
            eprintln!("error: {error}");
            return AppExit::error();
        }
    };
    let profile = match Profile::load_or_create(Path::new(PROFILE_FILE)) {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("error: the player profile cannot be loaded: {error}");
            return AppExit::error();
        }
    };

    if let Some(server_addr) = args.connect {
        app.add_plugins((
            SharedPlugin {
                role: NetworkRole::Client,
                content,
            },
            ClientPlugin {
                session: Session::Join {
                    server_addr,
                    player_id: profile.player_id,
                    simulated_latency: args.simulate_latency.map(Duration::from_millis),
                },
                settings_file: SETTINGS_FILE.into(),
                profile,
                profile_file: Some(PROFILE_FILE.into()),
                from_title: args.from_title,
            },
        ));
    } else if let Some(world) = world {
        let world = match WorldSetup::open_with(
            SaveDir::new(world),
            &content,
            WorldTime::FIRST_DAWN,
            args.seed,
        ) {
            Ok(world) => world,
            Err(error) => {
                eprintln!("error: the saved world cannot be loaded: {error}");
                return AppExit::error();
            }
        };
        // A world played from the title screen listens for guests from the
        // start, so its host can open it from the game menu; one started
        // from a terminal only listens when told to host.
        let bind_addr = if args.host || args.from_title {
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.port))
        } else {
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
        };
        app.add_plugins((
            SharedPlugin {
                role: NetworkRole::Host,
                content,
            },
            ServerPlugin {
                bind_addr,
                open_to_guests: args.host,
                max_players: MAX_PLAYERS,
                sleep_rule: args
                    .sleep_percent
                    .map_or(SleepRule::Everyone, |percent| SleepRule::Share { percent }),
                minute_length: GAME_MINUTE,
                world,
            },
            ClientPlugin {
                session: Session::Host,
                settings_file: SETTINGS_FILE.into(),
                profile,
                profile_file: Some(PROFILE_FILE.into()),
                from_title: args.from_title,
            },
        ));
    }

    app.run()
}

fn parse_server_addr(input: &str) -> Result<SocketAddr, String> {
    network::resolve_server_addr(input).map_err(|error| error.to_string())
}
