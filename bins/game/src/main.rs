// A release build shows no console on Windows; its log goes to a file.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod paths;

use std::{
    fs::{File, OpenOptions},
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use bevy::{
    asset::RenderAssetUsages,
    ecs::system::NonSendMarker,
    image::ImageLoaderSettings,
    log::{BoxedFmtLayer, LogPlugin, tracing_subscriber},
    prelude::*,
    winit::WINIT_WINDOWS,
};
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

/// Players a world hosted from the game takes at once.
const MAX_PLAYERS: u32 = 8;
/// The icon on the window, within the assets.
const ICON: &str = "textures/icon.png";

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
    /// or a new one is created. Implied by --host, as the world called
    /// `local` in the data folder.
    #[arg(long, value_name = "FOLDER")]
    world: Option<PathBuf>,

    /// Folder for worlds, the profile, the settings and the log. The
    /// player's own data folder unless given.
    #[arg(long, value_name = "FOLDER")]
    data_dir: Option<PathBuf>,

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

/// Where the log goes when there is no console to print to.
static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

fn main() -> AppExit {
    let args = Args::parse();
    let data_dir = paths::data_dir(args.data_dir.clone());
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        eprintln!(
            "error: the data folder {} cannot be made: {error}",
            data_dir.display()
        );
        return AppExit::error();
    }
    let log_file = data_dir.join(paths::LOG_FILE);
    if !cfg!(debug_assertions) {
        LOG_FILE.set(log_file).ok();
        std::panic::set_hook(Box::new(|panic| {
            if let Some(path) = LOG_FILE.get()
                && let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path)
            {
                let _ = writeln!(file, "{panic}");
            }
        }));
    }
    let profile_file = data_dir.join(paths::PROFILE_FILE);
    let settings_file = data_dir.join(paths::SETTINGS_FILE);
    let worlds = data_dir.join(paths::WORLDS_FOLDER);

    let mut app = App::new();
    let mut plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Messoria".into(),
            ..default()
        }),
        ..default()
    });
    if !cfg!(debug_assertions) {
        plugins = plugins.set(LogPlugin {
            fmt_layer: log_to_file,
            ..default()
        });
    }
    app.add_plugins(plugins)
        .add_systems(Startup, load_icon)
        .add_systems(Update, set_window_icon);

    let world = args
        .world
        .clone()
        .or_else(|| args.host.then(|| worlds.join("local")));
    if args.connect.is_none() && world.is_none() {
        app.add_plugins(TitlePlugin {
            saves: worlds,
            settings_file,
            notice: args.notice,
        });
        return app.run();
    }
    if let Err(exit) = add_the_game(&mut app, &args, world, profile_file, settings_file) {
        return exit;
    }
    app.run()
}

/// Adds the world played, joined or hosted, and the client for it.
fn add_the_game(
    app: &mut App,
    args: &Args,
    world: Option<PathBuf>,
    profile_file: PathBuf,
    settings_file: PathBuf,
) -> Result<(), AppExit> {
    let content = match load_content() {
        Ok(content) => content,
        Err(error) => return Err(fail(&format!("the content cannot be loaded: {error}"))),
    };
    let profile = match Profile::load_or_create(&profile_file) {
        Ok(profile) => profile,
        Err(error) => {
            return Err(fail(&format!(
                "the player profile cannot be loaded: {error}"
            )));
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
                settings_file,
                profile,
                profile_file: Some(profile_file),
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
                return Err(fail(&format!("the saved world cannot be loaded: {error}")));
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
                settings_file,
                profile,
                profile_file: Some(profile_file),
                from_title: args.from_title,
            },
        ));
    }
    Ok(())
}

/// Reports why the game cannot start, on the console or in the log, and
/// ends with an error.
fn fail(reason: &str) -> AppExit {
    eprintln!("error: {reason}");
    if let Some(path) = LOG_FILE.get()
        && let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path)
    {
        let _ = writeln!(file, "error: {reason}");
    }
    AppExit::error()
}

/// The log layer of a release build: the log file in the data folder,
/// started afresh each run.
fn log_to_file(_: &mut App) -> Option<BoxedFmtLayer> {
    let path = LOG_FILE.get()?;
    let file = File::create(path).ok()?;
    Some(Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(file)),
    ))
}

/// The icon, kept in memory so the window can be given it once loaded.
#[derive(Resource)]
struct Icon(Handle<Image>);

fn load_icon(assets: Res<AssetServer>, mut commands: Commands) {
    let icon = assets
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.asset_usage = RenderAssetUsages::MAIN_WORLD;
        })
        .load(ICON);
    commands.insert_resource(Icon(icon));
}

/// Runs on the main thread, where the windows live.
fn set_window_icon(
    icon: Res<Icon>,
    images: Res<Assets<Image>>,
    _main_thread: NonSendMarker,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Some(image) = images.get(&icon.0) else {
        return;
    };
    let Some(data) = image.data.as_ref() else {
        return;
    };
    let size = image.size();
    let Ok(icon) = winit::window::Icon::from_rgba(data.clone(), size.x, size.y) else {
        *done = true;
        return;
    };
    // The window may not exist yet in the first frames.
    *done = WINIT_WINDOWS.with_borrow(|windows| {
        for window in windows.windows.values() {
            window.set_window_icon(Some(icon.clone()));
        }
        !windows.windows.is_empty()
    });
}

fn parse_server_addr(input: &str) -> Result<SocketAddr, String> {
    network::resolve_server_addr(input).map_err(|error| error.to_string())
}
