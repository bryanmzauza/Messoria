//! The game's content, loaded once at startup and shared by every system.

use std::{env, path::PathBuf};

use bevy::prelude::*;
use messoria_content::{Catalog, ContentError};

/// Folder of the game's assets, relative to the asset root.
const ASSETS_FOLDER: &str = "assets";
/// Folder of the content data files, inside the assets folder.
const DATA_FOLDER: &str = "data";

/// The content catalog. Client and server must load the same content, since
/// item ids on the wire are positions in it.
#[derive(Resource, Deref)]
pub struct Content(pub Catalog);

/// Loads the content catalog from the game's data folder.
///
/// # Errors
///
/// Fails with the file and the problem if the content is missing or invalid.
pub fn load_content() -> Result<Catalog, ContentError> {
    Catalog::load(&data_dir())
}

/// Where the content data files live. Follows Bevy's rule for its asset
/// root, so content and other assets are always found side by side:
/// `BEVY_ASSET_ROOT` if set, the crate directory when run through Cargo, and
/// the executable's directory otherwise.
fn data_dir() -> PathBuf {
    let root = env::var_os("BEVY_ASSET_ROOT")
        .or_else(|| env::var_os("CARGO_MANIFEST_DIR"))
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(PathBuf::from))
        })
        .unwrap_or_default();
    root.join(ASSETS_FOLDER).join(DATA_FOLDER)
}
