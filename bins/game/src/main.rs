// A release build shows no console on Windows; its log goes to a file.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod paths;

use std::{
    fs::{File, OpenOptions},
    io::Write,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use bevy::{
    app::PanicHandlerPlugin,
    asset::RenderAssetUsages,
    ecs::system::NonSendMarker,
    image::ImageLoaderSettings,
    log::{BoxedFmtLayer, LogPlugin, tracing_subscriber},
    prelude::*,
    winit::WINIT_WINDOWS,
};
use clap::Parser;
use messoria_calendar::{GAME_MINUTE, SleepRule, WorldTime};
use messoria_client::{ClientPlugin, Session, TitlePlugin, back_to_title};
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
/// Ports after the one asked for that a world played from the title screen
/// may listen on instead, when another program holds that one.
const SPARE_PORTS: u16 = 15;

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
/// Whether this instance was started by another one (the title screen, or
/// a world going back to it), whose log it continues rather than erases.
static LOG_CONTINUED: AtomicBool = AtomicBool::new(false);

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
        LOG_CONTINUED.store(args.from_title, Ordering::Relaxed);
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
        // Bevy's panic hook prints the panics of failing systems on the
        // console and skips the hook above, leaving them out of the log.
        plugins = plugins
            .set(LogPlugin {
                fmt_layer: log_to_file,
                ..default()
            })
            .disable::<PanicHandlerPlugin>();
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
    if let Err(reason) = add_the_game(&mut app, &args, world, profile_file, settings_file) {
        return fail(&reason, args.from_title);
    }
    app.run()
}

/// Adds the world played, joined or hosted, and the client for it, or says
/// why it cannot be.
fn add_the_game(
    app: &mut App,
    args: &Args,
    world: Option<PathBuf>,
    profile_file: PathBuf,
    settings_file: PathBuf,
) -> Result<(), String> {
    let content =
        load_content().map_err(|error| format!("the content cannot be loaded: {error}"))?;
    let profile = Profile::load_or_create(&profile_file)
        .map_err(|error| format!("the player profile cannot be loaded: {error}"))?;

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
        let world = WorldSetup::open_with(
            SaveDir::new(world),
            &content,
            WorldTime::FIRST_DAWN,
            args.seed,
        )
        .map_err(|error| format!("the saved world cannot be loaded: {error}"))?;
        // A world played from the title screen listens for guests from the
        // start, so its host can open it from the game menu; one started
        // from a terminal only listens when told to host.
        let bind_addr = if args.host || args.from_title {
            let port = port_to_listen_on(args.port, args.host)?;
            if port != args.port {
                info!(
                    "port {} is in use by another program; listening on {port}",
                    args.port
                );
            }
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))
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

/// The port a hosted world listens on: the one `asked` for or, unless the
/// player asked to host on it, the first free one of the few after it, so
/// a server running on the same machine does not keep the player out of
/// their own world. Its host sees the port in the game menu.
fn port_to_listen_on(asked: u16, insist: bool) -> Result<u16, String> {
    let last = if insist {
        asked
    } else {
        asked.saturating_add(SPARE_PORTS)
    };
    // A socket bound and dropped at once: the server binds the port a
    // moment later.
    (asked..=last)
        .find(|&port| UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok())
        .ok_or_else(|| {
            if insist {
                format!("port {asked} is in use by another program; host on another one")
            } else {
                format!("ports {asked} to {last} are all in use by other programs")
            }
        })
}

/// Reports why the game cannot start, on the console or in the log, and
/// ends with an error. A game started from the title screen goes back to
/// it, which shows the reason.
fn fail(reason: &str, from_title: bool) -> AppExit {
    eprintln!("error: {reason}");
    if let Some(path) = LOG_FILE.get()
        && let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path)
    {
        let _ = writeln!(file, "error: {reason}");
    }
    if from_title {
        back_to_title(Some(&as_sentence(reason)));
    }
    AppExit::error()
}

/// `reason`, capitalized and ended with a full stop.
fn as_sentence(reason: &str) -> String {
    let mut chars = reason.chars();
    let mut sentence: String = chars
        .next()
        .map(char::to_uppercase)
        .into_iter()
        .flatten()
        .collect();
    sentence.push_str(chars.as_str());
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

/// The log layer of a release build: the log file in the data folder,
/// started afresh each time the game is opened and kept as it passes
/// between the title screen and the worlds.
fn log_to_file(_: &mut App) -> Option<BoxedFmtLayer> {
    let path = LOG_FILE.get()?;
    let file = if LOG_CONTINUED.load(Ordering::Relaxed) {
        OpenOptions::new().append(true).create(true).open(path)
    } else {
        File::create(path)
    }
    .ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_in_use_is_passed_over_unless_insisted_on() {
        let taken = (0..10)
            .filter_map(|_| UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok())
            .find(|socket| {
                socket
                    .local_addr()
                    .is_ok_and(|addr| addr.port() < u16::MAX - SPARE_PORTS)
            })
            .expect("a free port low enough to have spares after it");
        let port = taken.local_addr().unwrap().port();

        let error = port_to_listen_on(port, true).unwrap_err();
        assert!(error.contains(&port.to_string()), "{error}");
        let other = port_to_listen_on(port, false).unwrap();
        assert!(other > port && other <= port + SPARE_PORTS, "{other}");
    }

    #[test]
    fn reasons_read_as_sentences_on_the_title_screen() {
        assert_eq!(
            as_sentence("the saved world cannot be loaded: world.ron is truncated"),
            "The saved world cannot be loaded: world.ron is truncated."
        );
        assert_eq!(as_sentence("Already one."), "Already one.");
    }
}
