//! The local player's settings: how the game sounds, looks and handles.

use std::{ops::RangeInclusive, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Problem, SaveError},
    files::{read_if_present, to_ron, unreadable, version_of, write_atomically},
};

/// Version of the format this game writes.
const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Loudness of every sound, from silent to as recorded.
    pub volume: f32,
    /// Scales how far the view turns for a given mouse movement.
    pub mouse_sensitivity: f32,
    /// Vertical field of view, in degrees.
    pub field_of_view: f32,
}

impl Settings {
    pub const VOLUME: RangeInclusive<f32> = 0.0..=1.0;
    pub const MOUSE_SENSITIVITY: RangeInclusive<f32> = 0.2..=3.0;
    pub const FIELD_OF_VIEW: RangeInclusive<f32> = 40.0..=100.0;

    /// Reads the settings saved at `path`, if any were. Values outside
    /// their ranges, as a hand-edited file may hold, are brought into them.
    ///
    /// # Errors
    ///
    /// If the file exists but cannot be read.
    pub fn load(path: &Path) -> Result<Option<Self>, SaveError> {
        let in_file = |problem| SaveError {
            file: path.to_path_buf(),
            problem,
        };
        let Some(bytes) = read_if_present(path).map_err(|error| in_file(error.into()))? else {
            return Ok(None);
        };
        Self::from_ron(&String::from_utf8_lossy(&bytes))
            .map(Some)
            .map_err(in_file)
    }

    /// Saves the settings at `path`.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        let in_file = |problem| SaveError {
            file: path.to_path_buf(),
            problem,
        };
        let text = to_ron(&SettingsFile {
            version: VERSION,
            volume: self.volume,
            mouse_sensitivity: self.mouse_sensitivity,
            field_of_view: self.field_of_view,
        })
        .map_err(in_file)?;
        write_atomically(path, text.as_bytes()).map_err(|error| in_file(error.into()))
    }

    /// These settings with every value brought into its range.
    #[must_use]
    pub fn clamped(self) -> Self {
        let clamp = |value: f32, range: RangeInclusive<f32>| {
            if value.is_finite() {
                value.clamp(*range.start(), *range.end())
            } else {
                *range.start()
            }
        };
        Self {
            volume: clamp(self.volume, Self::VOLUME),
            mouse_sensitivity: clamp(self.mouse_sensitivity, Self::MOUSE_SENSITIVITY),
            field_of_view: clamp(self.field_of_view, Self::FIELD_OF_VIEW),
        }
    }

    fn from_ron(text: &str) -> Result<Self, Problem> {
        let file: SettingsFile = match version_of(text)? {
            VERSION => ron::from_str(text)?,
            found => return Err(unreadable(found, VERSION)),
        };
        Ok(Self {
            volume: file.volume,
            mouse_sensitivity: file.mouse_sensitivity,
            field_of_view: file.field_of_view,
        }
        .clamped())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            mouse_sensitivity: 1.0,
            field_of_view: 60.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    volume: f32,
    mouse_sensitivity: f32,
    field_of_view: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_load_back() {
        let path = std::env::temp_dir()
            .join(format!("messoria-settings-test-{}", std::process::id()))
            .join("settings.ron");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Settings::load(&path).expect("no file is fine"), None);

        let settings = Settings {
            volume: 0.3,
            mouse_sensitivity: 1.5,
            field_of_view: 75.0,
        };
        settings.save(&path).expect("settings are saved");
        assert_eq!(Settings::load(&path).expect("readable"), Some(settings));
    }

    #[test]
    fn values_out_of_range_are_brought_into_it() {
        let text = "(version: 1, volume: 3.0, mouse_sensitivity: 0.0, field_of_view: 70.0)";
        let settings = Settings::from_ron(text).expect("readable");
        assert!((settings.volume - 1.0).abs() < f32::EPSILON);
        assert!((settings.mouse_sensitivity - 0.2).abs() < f32::EPSILON);
        assert!((settings.field_of_view - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_from_a_newer_game_are_refused() {
        let text = "(version: 2, volume: 1.0, mouse_sensitivity: 1.0, field_of_view: 70.0)";
        assert!(matches!(
            Settings::from_ron(text),
            Err(Problem::Newer { found: 2, .. })
        ));
    }
}
