//! The local player's profile, kept on their own machine.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    error::{Problem, SaveError},
    files::{read_if_present, to_ron, unreadable, version_of, write_atomically},
};

/// Version of the format this game writes.
const VERSION: u32 = 1;

/// Who the local player is, the same in every world they join. Servers
/// recognize returning players by their id.
///
/// Nothing proves the id belongs to the player yet; issuing connections
/// through the rendezvous service will.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub player_id: u64,
}

#[derive(Serialize, Deserialize)]
struct ProfileFile {
    version: u32,
    player_id: u64,
}

impl Profile {
    /// Reads the profile at `path`, or creates and saves a new one if there
    /// is none.
    ///
    /// # Errors
    ///
    /// If the profile exists but cannot be read, or a new one cannot be
    /// saved.
    pub fn load_or_create(path: &Path) -> Result<Self, SaveError> {
        let in_file = |problem| SaveError {
            file: path.to_path_buf(),
            problem,
        };
        if let Some(bytes) = read_if_present(path).map_err(|error| in_file(error.into()))? {
            return Self::from_ron(&String::from_utf8_lossy(&bytes)).map_err(in_file);
        }
        let profile = Self {
            player_id: rand::random(),
        };
        let text = to_ron(&ProfileFile {
            version: VERSION,
            player_id: profile.player_id,
        })
        .map_err(in_file)?;
        write_atomically(path, text.as_bytes()).map_err(|error| in_file(error.into()))?;
        Ok(profile)
    }

    fn from_ron(text: &str) -> Result<Self, Problem> {
        let file: ProfileFile = match version_of(text)? {
            VERSION => ron::from_str(text)?,
            found => return Err(unreadable(found, VERSION)),
        };
        Ok(Self {
            player_id: file.player_id,
        })
    }
}
