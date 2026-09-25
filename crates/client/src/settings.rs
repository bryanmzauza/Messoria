//! The player's settings: loading them, applying them, and saving them when
//! the options window closes.

use std::path::PathBuf;

use bevy::{audio::Volume, prelude::*};
use messoria_save::Settings;

use crate::{camera::WorldCamera, panels::OpenPanel};

pub(crate) struct SettingsPlugin {
    /// Where the settings are kept.
    pub file: PathBuf,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let settings = match Settings::load(&self.file) {
            Ok(saved) => saved.unwrap_or_default(),
            Err(error) => {
                warn!("using the default settings: {error}");
                Settings::default()
            }
        };
        app.insert_resource(Preferences(settings))
            .insert_resource(SettingsFile(self.file.clone()))
            .add_systems(Update, (apply_volume, apply_field_of_view, save_on_close));
    }
}

/// The settings in effect.
#[derive(Resource, Debug)]
pub(crate) struct Preferences(pub Settings);

#[derive(Resource)]
struct SettingsFile(PathBuf);

fn apply_volume(preferences: Res<Preferences>, mut volume: ResMut<GlobalVolume>) {
    if preferences.is_changed() {
        volume.volume = Volume::Linear(preferences.0.volume);
    }
}

fn apply_field_of_view(
    preferences: Res<Preferences>,
    mut cameras: Query<(&mut Projection, Ref<WorldCamera>)>,
) {
    for (mut projection, camera) in &mut cameras {
        if (preferences.is_changed() || camera.is_added())
            && let Projection::Perspective(perspective) = &mut *projection
        {
            perspective.fov = preferences.0.field_of_view.to_radians();
        }
    }
}

/// Saves the settings when the options window closes, if they changed while
/// it was open.
fn save_on_close(
    panel: Res<OpenPanel>,
    preferences: Res<Preferences>,
    file: Res<SettingsFile>,
    mut shown: Local<Option<Settings>>,
) {
    if !panel.is_changed() {
        return;
    }
    if *panel == OpenPanel::Options {
        *shown = Some(preferences.0);
    } else if let Some(before) = shown.take()
        && before != preferences.0
        && let Err(error) = preferences.0.save(&file.0)
    {
        warn!("the settings could not be saved: {error}");
    }
}
