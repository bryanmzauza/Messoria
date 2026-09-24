//! The game menu Escape opens, and the options window it leads to.
//!
//! The world does not pause behind the menu: it is shared with other
//! players, and hosting it means serving them too.

use bevy::prelude::*;
use messoria_save::{Graphics, Settings};

use crate::{
    camera::LookSystems,
    connection::Session,
    panels::OpenPanel,
    settings::Preferences,
    ui::{self, BACKDROP_COLOR, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, TITLE_SIZE},
};

const VOLUME_STEP: f32 = 0.1;
const SENSITIVITY_STEP: f32 = 0.1;
const FIELD_OF_VIEW_STEP: f32 = 5.0;
const SETTING_NAME_WIDTH: f32 = 170.0;
const SETTING_VALUE_WIDTH: f32 = 70.0;
/// Above everything else on screen.
const MENU_LAYER: i32 = 10;

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_game_menu, spawn_options))
            .add_systems(
                Update,
                (
                    // Before looking around, so that the click that closes
                    // the menu also captures the cursor.
                    press_buttons.before(LookSystems),
                    show_menus,
                    show_settings,
                ),
            );
    }
}

#[derive(Component)]
struct GameMenu;

#[derive(Component)]
struct OptionsWindow;

#[derive(Component, Clone, Copy)]
enum MenuButton {
    BackToGame,
    Options,
    Quit,
    Done,
    /// Steps a setting down (-1) or up (+1).
    Change(Setting, i8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Volume,
    MouseSensitivity,
    FieldOfView,
    Graphics,
}

impl Setting {
    const ALL: [Self; 4] = [
        Self::Volume,
        Self::MouseSensitivity,
        Self::FieldOfView,
        Self::Graphics,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::MouseSensitivity => "Mouse sensitivity",
            Self::FieldOfView => "Field of view",
            Self::Graphics => "Graphics",
        }
    }

    fn shown(self, settings: &Settings) -> String {
        match self {
            Self::Volume => format!("{:.0}%", settings.volume * 100.0),
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
        *settings = settings.clamped();
    }
}

/// Shows the current value of a setting.
#[derive(Component)]
struct SettingValue(Setting);

fn spawn_game_menu(session: Res<Session>, mut commands: Commands) {
    let quit = match *session {
        Session::Host => "Save and Quit",
        Session::Join { .. } => "Quit",
    };
    commands
        .spawn((
            Name::new("Game menu"),
            GameMenu,
            ui::screen(),
            BackgroundColor(BACKDROP_COLOR),
            GlobalZIndex(MENU_LAYER),
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(10),
                    ..default()
                })
                .with_children(|column| {
                    column.spawn((
                        ui::label("Game Menu", TITLE_SIZE, TEXT_COLOR),
                        Node {
                            margin: UiRect::bottom(px(14)),
                            ..default()
                        },
                    ));
                    ui::spawn_wide_button(column, "Back to Game", MenuButton::BackToGame);
                    ui::spawn_wide_button(column, "Options...", MenuButton::Options);
                    ui::spawn_wide_button(column, quit, MenuButton::Quit);
                    column.spawn((
                        ui::label(
                            "The world goes on while this menu is open.",
                            TEXT_SIZE,
                            MUTED_TEXT_COLOR,
                        ),
                        Node {
                            margin: UiRect::top(px(14)),
                            ..default()
                        },
                    ));
                });
        });
}

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
                            ui::spawn_wide_button(row, "Done", MenuButton::Done);
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
            ui::spawn_button(row, "-", MenuButton::Change(setting, -1));
            row.spawn((
                SettingValue(setting),
                ui::label("", TEXT_SIZE, TEXT_COLOR),
                TextLayout::justify(Justify::Center),
                Node {
                    width: px(SETTING_VALUE_WIDTH),
                    ..default()
                },
            ));
            ui::spawn_button(row, "+", MenuButton::Change(setting, 1));
        });
}

fn press_buttons(
    buttons: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    mut panel: ResMut<OpenPanel>,
    mut preferences: ResMut<Preferences>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            MenuButton::BackToGame => *panel = OpenPanel::None,
            MenuButton::Options => *panel = OpenPanel::Options,
            MenuButton::Done => *panel = OpenPanel::Menu,
            // A hosted world saves itself as the app exits.
            MenuButton::Quit => {
                exit.write(AppExit::Success);
            }
            MenuButton::Change(setting, direction) => {
                setting.step(&mut preferences.0, direction);
            }
        }
    }
}

fn show_menus(
    panel: Res<OpenPanel>,
    mut menu: Single<&mut Visibility, (With<GameMenu>, Without<OptionsWindow>)>,
    mut options: Single<&mut Visibility, With<OptionsWindow>>,
) {
    if panel.is_changed() {
        menu.set_if_neq(ui::visible_if(*panel == OpenPanel::Menu));
        options.set_if_neq(ui::visible_if(*panel == OpenPanel::Options));
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
    }
}
