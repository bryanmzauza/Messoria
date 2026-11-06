//! The local player's profile, kept on their own machine.

use std::path::Path;

use messoria_content::Look;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Problem, SaveError},
    files::{read_if_present, to_ron, unreadable, version_of, write_atomically},
};

/// Version of the format this game writes. Version 1 profiles had no name
/// or look; their players are asked for them on their next arrival.
const VERSION: u32 = 2;
const WITHOUT_A_LOOK: u32 = 1;

/// Who the local player is, the same in every world they join. Servers
/// recognize returning players by their id; the name and the look are what
/// they introduce themselves with to a world new to them.
///
/// Nothing proves the id belongs to the player yet; issuing connections
/// through the rendezvous service will.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub player_id: u64,
    /// `None` until the player has made themselves a look.
    pub name: Option<String>,
    pub look: Option<Look>,
}

#[derive(Serialize, Deserialize)]
struct ProfileFile {
    version: u32,
    player_id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    look: Option<Look>,
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
            name: None,
            look: None,
        };
        profile.save(path)?;
        Ok(profile)
    }

    /// Writes the profile to `path`.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        let in_file = |problem| SaveError {
            file: path.to_path_buf(),
            problem,
        };
        let text = to_ron(&ProfileFile {
            version: VERSION,
            player_id: self.player_id,
            name: self.name.clone(),
            look: self.look.clone(),
        })
        .map_err(in_file)?;
        write_atomically(path, text.as_bytes()).map_err(|error| in_file(error.into()))
    }

    fn from_ron(text: &str) -> Result<Self, Problem> {
        let file: ProfileFile = match version_of(text)? {
            VERSION | WITHOUT_A_LOOK => ron::from_str(text)?,
            found => return Err(unreadable(found, VERSION)),
        };
        Ok(Self {
            player_id: file.player_id,
            name: file.name,
            look: file.look,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "messoria-profile-tests-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder.join("profile.ron")
    }

    #[test]
    fn a_new_profile_is_kept_and_a_look_saved_into_it_comes_back() {
        let path = scratch("look");
        let mut profile = Profile::load_or_create(&path).unwrap();
        assert_eq!(profile.name, None);
        assert_eq!(Profile::load_or_create(&path).unwrap(), profile);

        profile.name = Some("Wren".to_owned());
        profile.look = Some(Look {
            body: "body-fair.png".to_owned(),
            eyes: (0.3, 0.2, 0.1),
            outfit: "outfit-tunic.png".to_owned(),
            hair: "hair-braid.png".to_owned(),
            hair_color: (0.5, 0.3, 0.1),
            hat: None,
        });
        profile.save(&path).unwrap();
        assert_eq!(Profile::load_or_create(&path).unwrap(), profile);
    }

    #[test]
    fn profiles_without_a_look_read_as_having_none() {
        let path = scratch("old");
        fs::write(&path, "(version: 1, player_id: 7)").unwrap();
        let profile = Profile::load_or_create(&path).unwrap();
        assert_eq!(profile.player_id, 7);
        assert_eq!(profile.look, None);
    }
}
