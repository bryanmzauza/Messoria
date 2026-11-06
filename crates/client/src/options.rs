//! The options window: the settings, stepped up and down, in the game and
//! on the title screen alike.

use bevy::prelude::*;
use messoria_save::{Graphics, Settings};

use crate::{
    panels::OpenPanel,
    settings::Preferences,
    ui::{self, BACKDROP_COLOR, TEXT_COLOR, TEXT_SIZE, TITLE_SIZE},
};

const VOLUME_STEP: f32 = 0.1;
const SENSITIVITY_STEP: f32 = 0.1;
const FIELD_OF_VIEW_STEP: f32 = 5.0;
const SETTING_NAME_WIDTH: f32 = 170.0;
const SETTING_VALUE_WIDTH: f32 = 70.0;
/// Above everything else on screen.
pub(crate) const MENU_LAYER: i32 = 10;

pub(crate) struct OptionsPlugin;

impl Plugin for OptionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_options)
            .add_systems(Update, (press_buttons, show_options, show_settings));
    }
}

#[derive(Component)]
struct OptionsWindow;

#[derive(Component, Clone, Copy)]
enum OptionsButton {
    Done,
    /// Steps a setting down (-1) or up (+1).
    Change(Setting, i8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Volume,
    Music,
    MouseSensitivity,
    FieldOfView,
    Graphics,
}

impl Setting {
    const ALL: [Self; 5] = [
        Self::Volume,
        Self::Music,
        Self::MouseSensitivity,
        Self::FieldOfView,
        Self::Graphics,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Music => "Music",
            Self::MouseSensitivity => "Mouse sensitivity",
            Self::FieldOfView => "Field of view",
            Self::Graphics => "Graphics",
        }
    }

    fn shown(self, settings: &Settings) -> String {
        match self {
            Self::Volume => format!("{:.0}%", settings.volume * 100.0),
            Self::Music => format!("{:.0}%", settings.music_volume * 100.0),
            Self::MouseSensitivity => format!("{:.1}x", settings.mouse_sensitivity),
            Self::FieldOfView => format!("{:.0}", settings.field_of_view),
            Self::Graphics => match settings.graphics {
                Graphics::Low => "Low",
                Graphics::Medium => "Medium",
                Graphics::High => "High",
            }
            .to_owned(),
        }
    }

    fn step(self, settings: &mut Settings, direction: i8) {
        let (value, step) = match self {
            Self::Volume => (&mut settings.volume, VOLUME_STEP),
            Self::Music => (&mut settings.music_volume, VOLUME_STEP),
            Self::MouseSensitivity => (&mut settings.mouse_sensitivity, SENSITIVITY_STEP),
            Self::FieldOfView => (&mut settings.field_of_view, FIELD_OF_VIEW_STEP),
            Self::Graphics => {
                settings.graphics = settings.graphics.step(direction);
                return;
            }
        };
        let direction = f32::from(direction);
        // Rounded to the step, so repeated steps land on round values.
        *value = ((*value + direction * step) / step).round() * step;
        *settings = settings.clone().clamped();
    }
}

/// Shows the current value of a setting.
#[derive(Component)]
struct SettingValue(Setting);

fn spawn_options(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Options"),
            OptionsWindow,
            ui::screen(),
            BackgroundColor(BACKDROP_COLOR),
            GlobalZIndex(MENU_LAYER),
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen
                .spawn(ui::window(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(24)),
                    row_gap: px(12),
                    ..default()
                }))
                .with_children(|window| {
                    window.spawn((
                        ui::label("Options", TITLE_SIZE, TEXT_COLOR),
                        Node {
                            margin: UiRect::bottom(px(8)),
                            ..default()
                        },
                    ));
                    for setting in Setting::ALL {
                        spawn_setting_row(window, setting);
                    }
                    window
                        .spawn(Node {
                            margin: UiRect::top(px(12)),
                            ..default()
                        })
                        .with_children(|row| {
                            ui::spawn_wide_button(row, "Done", OptionsButton::Done);
                        });
                });
        });
}

fn spawn_setting_row(parent: &mut ChildSpawnerCommands, setting: Setting) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::label(setting.name(), TEXT_SIZE, TEXT_COLOR),
                TextLayout::no_wrap(),
                Node {
                    width: px(SETTING_NAME_WIDTH),
                    ..default()
                },
            ));
            ui::spawn_button(row, "-", OptionsButton::Change(setting, -1));
            row.spawn((
                SettingValue(setting),
                ui::label("", TEXT_SIZE, TEXT_COLOR),
                TextLayout::justify(Justify::Center),
                Node {
                    width: px(SETTING_VALUE_WIDTH),
                    ..default()
                },
            ));
            ui::spawn_button(row, "+", OptionsButton::Change(setting, 1));
        });
}

/// Done leads back to the menu the options were opened from.
fn press_buttons(
    buttons: Query<(&Interaction, &OptionsButton), Changed<Interaction>>,
    mut panel: ResMut<OpenPanel>,
    mut preferences: ResMut<Preferences>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            OptionsButton::Done => *panel = OpenPanel::Menu,
            OptionsButton::Change(setting, direction) => {
                setting.step(&mut preferences.0, direction);
            }
        }
    }
}

fn show_options(panel: Res<OpenPanel>, mut window: Single<&mut Visibility, With<OptionsWindow>>) {
    if panel.is_changed() {
        window.set_if_neq(ui::visible_if(*panel == OpenPanel::Options));
    }
}

fn show_settings(preferences: Res<Preferences>, mut values: Query<(&SettingValue, &mut Text)>) {
    if !preferences.is_changed() {
        return;
    }
    for (value, mut text) in &mut values {
        text.0 = value.0.shown(&preferences.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_land_on_round_values_within_range() {
        let mut settings = Settings {
            volume: 0.83,
            ..Settings::default()
        };
        Setting::Volume.step(&mut settings, 1);
        assert!((settings.volume - 0.9).abs() < 1e-5);
        Setting::Volume.step(&mut settings, 1);
        Setting::Volume.step(&mut settings, 1);
        assert!((settings.volume - 1.0).abs() < 1e-5, "stops at the top");

        settings.field_of_view = 40.0;
        Setting::FieldOfView.step(&mut settings, -1);
        assert!(
            (settings.field_of_view - 40.0).abs() < 1e-5,
            "stops at the bottom"
        );
        Setting::Graphics.step(&mut settings, -1);
        assert_eq!(settings.graphics, Graphics::Medium);
    }
}
