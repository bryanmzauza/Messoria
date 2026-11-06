//! Where the game keeps what it writes: worlds, the profile, the settings
//! and the log, in the player's data folder unless told otherwise.

use std::{
    env,
    path::{Path, PathBuf},
};

/// Overrides the data folder for development runs; Cargo sets it to the
/// workspace's `saves` folder.
const DATA_DIR_VAR: &str = "MESSORIA_DATA_DIR";
pub const WORLDS_FOLDER: &str = "worlds";
pub const PROFILE_FILE: &str = "profile.ron";
pub const SETTINGS_FILE: &str = "settings.ron";
pub const LOG_FILE: &str = "messoria.log";

/// The data folder: the one asked for, the one the environment names, or
/// the player's own: `%APPDATA%\Messoria` on Windows, `$XDG_DATA_HOME`
/// or `~/.local/share` with `messoria` elsewhere.
pub fn data_dir(asked: Option<PathBuf>) -> PathBuf {
    asked
        .or_else(|| env::var_os(DATA_DIR_VAR).map(PathBuf::from))
        .unwrap_or_else(own_data_dir)
}

fn own_data_dir() -> PathBuf {
    if cfg!(windows) {
        env::var_os("APPDATA").map_or_else(
            || PathBuf::from("Messoria"),
            |appdata| Path::new(&appdata).join("Messoria"),
        )
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
            .map_or_else(|| PathBuf::from("messoria"), |share| share.join("messoria"))
    }
}
