//! The title screen: the valley behind a short column of choices. Continue
//! the last world, start a new one, host a world for friends, join one,
//! the options, and leaving.
//!
//! Choosing a world or a server starts the game again with the arguments
//! for it and ends this instance; the game comes back the same way when
//! the player leaves, with a word on why if it was sent away. Worlds are
//! listed from the saves folder, latest played first, and can be deleted
//! after a second thought.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use bevy::prelude::*;
use messoria_save::{SaveDir, WorldSummary, is_valid_world_name};
use messoria_shared::network::{self, DEFAULT_PORT};

use crate::{
    launch,
    panels::OpenPanel,
    settings::{Preferences, SettingsFile},
    ui::{self, ACCENT_COLOR, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, TextField},
};

/// The painting behind the title: the valley as the game draws it.
const BACKDROP: &str = "textures/title.png";
const TITLE_SIZE: f32 = 72.0;
const COLUMN_WIDTH: f32 = 420.0;
const FIELD_WIDTH: f32 = 300.0;
const WORLD_NAME_LENGTH: usize = 32;
const SEED_LENGTH: usize = 20;
const ADDRESS_LENGTH: usize = 64;
/// Share of players asleep that ends the night in a world hosted from
/// here, unless the host says otherwise.
const DEFAULT_SLEEP_PERCENT: u8 = 50;
const DIM_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.42);

pub(crate) struct TitlePlugin {
    pub saves: PathBuf,
    pub notice: Option<String>,
}

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Title {
            saves: self.saves.clone(),
            screen: Screen::Main,
            worlds: SaveDir::worlds(&self.saves),
            notice: self.notice.clone(),
        })
        .init_resource::<OpenPanel>()
        .add_systems(Startup, spawn_title)
        .add_systems(
            Update,
            (leave_options, press_buttons, submit_fields, show_title).chain(),
        );
    }
}

/// What the title shows: which screen, the worlds, and a word to the
/// player, such as why they are back here.
#[derive(Resource)]
struct Title {
    saves: PathBuf,
    screen: Screen,
    worlds: Vec<WorldSummary>,
    notice: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum Screen {
    Main,
    /// The worlds, to play one or to host one.
    Worlds {
        to_host: bool,
    },
    NewWorld,
    Join,
    /// A second thought before deleting this world.
    Delete(String),
}

#[derive(Component)]
struct TitleListing;

#[derive(Component)]
struct NoticeText;

#[derive(Component, Clone, PartialEq, Eq)]
enum TitleButton {
    Continue,
    NewWorld,
    Host,
    Join,
    Options,
    Quit,
    Back,
    Worlds,
    Play(String),
    HostWorld(String),
    AskDelete(String),
    Delete(String),
    Create,
    Connect,
    ConnectTo(String),
}

/// The fields of the new world, host and join screens.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Field {
    WorldName,
    Seed,
    Port,
    SleepPercent,
    Address,
}

fn spawn_title(assets: Res<AssetServer>, mut commands: Commands) {
    // Nothing else draws here, so the title brings its own camera.
    commands.spawn((Name::new("Title camera"), Camera2d));
    commands
        .spawn((
            Name::new("Title screen"),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(assets.load(BACKDROP)),
        ))
        .with_children(|screen| {
            screen
                .spawn((
                    Node {
                        width: percent(100),
                        height: percent(100),
                        padding: UiRect::all(px(56)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                    BackgroundColor(DIM_COLOR),
                ))
                .with_children(|page| {
                    page.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        ..default()
                    })
                    .with_children(|heading| {
                        heading.spawn((
                            ui::label("Messoria", TITLE_SIZE, TEXT_COLOR),
                            TextShadow::default(),
                        ));
                        heading.spawn(ui::label(
                            "A valley to farm with friends",
                            HEADING_SIZE,
                            MUTED_TEXT_COLOR,
                        ));
                    });
                    page.spawn((
                        TitleListing,
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(8),
                            width: px(COLUMN_WIDTH),
                            ..default()
                        },
                    ));
                    page.spawn((
                        NoticeText,
                        ui::label("", TEXT_SIZE, ACCENT_COLOR),
                        TextShadow::default(),
                    ));
                });
        });
}

/// The options window's Done leads to the menu it came from, which here is
/// the title itself.
fn leave_options(mut panel: ResMut<OpenPanel>) {
    if *panel == OpenPanel::Menu {
        *panel = OpenPanel::None;
    }
}

fn press_buttons(
    buttons: Query<(&Interaction, &TitleButton), Changed<Interaction>>,
    fields: Query<(&Field, &TextField)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut title: ResMut<Title>,
    mut preferences: ResMut<Preferences>,
    settings_file: Res<SettingsFile>,
    mut panel: ResMut<OpenPanel>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut pressed: Vec<TitleButton> = buttons
        .iter()
        .filter(|(interaction, _)| **interaction == Interaction::Pressed)
        .map(|(_, button)| button.clone())
        .collect();
    if keys.just_pressed(KeyCode::Escape) && *panel == OpenPanel::None {
        pressed.push(TitleButton::Back);
    }
    for button in pressed {
        if button == TitleButton::Options {
            *panel = OpenPanel::Options;
        } else {
            let mut desk = Desk {
                title: &mut title,
                preferences: &mut preferences,
                settings_file: &settings_file,
                typed: fields
                    .iter()
                    .map(|(field, text)| (*field, text.text.trim().to_owned()))
                    .collect(),
                exit: &mut exit,
            };
            desk.act(button);
        }
    }
}

/// Everything a press on the title needs at hand.
struct Desk<'a, 'e> {
    title: &'a mut Title,
    preferences: &'a mut Preferences,
    settings_file: &'a SettingsFile,
    /// What is typed in each field, trimmed.
    typed: Vec<(Field, String)>,
    exit: &'a mut MessageWriter<'e, AppExit>,
}

impl Desk<'_, '_> {
    fn typed(&self, wanted: Field) -> String {
        self.typed
            .iter()
            .find(|(field, _)| *field == wanted)
            .map(|(_, text)| text.clone())
            .unwrap_or_default()
    }

    fn act(&mut self, button: TitleButton) {
        match button {
            TitleButton::Continue => {
                let last = self.preferences.0.last_world.clone();
                let known =
                    |name: &String| self.title.worlds.iter().any(|world| world.name == *name);
                match last.filter(known) {
                    Some(name) => self.play(&name, None, None),
                    None => self.title.screen = Screen::NewWorld,
                }
            }
            TitleButton::NewWorld => self.title.screen = Screen::NewWorld,
            TitleButton::Worlds => self.title.screen = Screen::Worlds { to_host: false },
            TitleButton::Host => self.title.screen = Screen::Worlds { to_host: true },
            TitleButton::Join => self.title.screen = Screen::Join,
            TitleButton::Quit => {
                self.exit.write(AppExit::Success);
            }
            TitleButton::Back => self.title.screen = Screen::Main,
            TitleButton::Options => {}
            TitleButton::Play(name) => self.play(&name, None, None),
            TitleButton::HostWorld(name) => {
                match hosting_terms(&self.typed(Field::Port), &self.typed(Field::SleepPercent)) {
                    Ok(terms) => self.play(&name, None, Some(terms)),
                    Err(notice) => self.title.notice = Some(notice),
                }
            }
            TitleButton::AskDelete(name) => self.title.screen = Screen::Delete(name),
            TitleButton::Delete(name) => self.delete(&name),
            TitleButton::Create => {
                let (name, seed) = (self.typed(Field::WorldName), self.typed(Field::Seed));
                self.create(&name, &seed);
            }
            TitleButton::Connect => {
                let address = self.typed(Field::Address);
                self.join(&address);
            }
            TitleButton::ConnectTo(address) => self.join(&address),
        }
    }

    fn delete(&mut self, name: &str) {
        let folder = self.title.saves.join(name);
        self.title.notice = Some(match SaveDir::new(folder).delete() {
            Ok(()) => format!("{name} is gone."),
            Err(error) => format!("{name} could not be deleted: {error}"),
        });
        self.title.worlds = SaveDir::worlds(&self.title.saves);
        self.title.screen = Screen::Worlds { to_host: false };
    }
}

/// Enter in a field does what its screen's button does.
fn submit_fields(
    fields: Query<(&Field, &TextField)>,
    mut title: ResMut<Title>,
    mut preferences: ResMut<Preferences>,
    settings_file: Res<SettingsFile>,
    mut exit: MessageWriter<AppExit>,
) {
    let submitted = fields.iter().any(|(_, field)| field.submitted);
    if !submitted {
        return;
    }
    let button = match title.screen {
        Screen::NewWorld => TitleButton::Create,
        Screen::Join => TitleButton::Connect,
        Screen::Main | Screen::Worlds { .. } | Screen::Delete(_) => return,
    };
    let mut desk = Desk {
        title: &mut title,
        preferences: &mut preferences,
        settings_file: &settings_file,
        typed: fields
            .iter()
            .map(|(field, text)| (*field, text.text.trim().to_owned()))
            .collect(),
        exit: &mut exit,
    };
    desk.act(button);
}

/// The port and the share of sleepers a hosted world is started with.
struct HostingTerms {
    port: u16,
    sleep_percent: u8,
}

fn hosting_terms(port: &str, sleep: &str) -> Result<HostingTerms, String> {
    let port = if port.is_empty() {
        DEFAULT_PORT
    } else {
        port.parse()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| "The port must be a number from 1 to 65535.".to_owned())?
    };
    let sleep_percent = if sleep.is_empty() {
        DEFAULT_SLEEP_PERCENT
    } else {
        sleep
            .parse()
            .ok()
            .filter(|percent| (1..=100).contains(percent))
            .ok_or_else(|| "The share of sleepers must be from 1 to 100.".to_owned())?
    };
    Ok(HostingTerms {
        port,
        sleep_percent,
    })
}

impl Desk<'_, '_> {
    /// Starts the game in the world `name`, hosted for friends on
    /// `hosting`'s terms or kept to oneself, and remembers it as the last
    /// one played. A world played from here listens on the default port
    /// either way, so its host can open it from the game menu.
    fn play(&mut self, name: &str, seed: Option<u64>, hosting: Option<HostingTerms>) {
        let port = hosting.as_ref().map_or(DEFAULT_PORT, |terms| terms.port);
        let mut args = vec![
            "--from-title".to_owned(),
            "--world".to_owned(),
            self.title.saves.join(name).to_string_lossy().into_owned(),
            "--port".to_owned(),
            port.to_string(),
        ];
        if let Some(seed) = seed {
            args.push("--seed".to_owned());
            args.push(seed.to_string());
        }
        if let Some(terms) = hosting {
            args.push("--host".to_owned());
            args.push("--sleep-percent".to_owned());
            args.push(terms.sleep_percent.to_string());
        }
        self.preferences.0.last_world = Some(name.to_owned());
        self.start(args);
    }

    fn create(&mut self, name: &str, seed: &str) {
        if !is_valid_world_name(name) {
            self.title.notice =
                Some("A world's name is letters, digits and spaces, up to 32.".to_owned());
            return;
        }
        if self.title.worlds.iter().any(|world| world.name == name) {
            self.title.notice = Some(format!("There is a world called {name} already."));
            return;
        }
        let seed = if seed.is_empty() {
            None
        } else {
            let Ok(seed) = seed.parse::<u64>() else {
                self.title.notice = Some("A seed is a whole number.".to_owned());
                return;
            };
            Some(seed)
        };
        self.play(name, seed, None);
    }

    fn join(&mut self, address: &str) {
        if address.is_empty() {
            self.title.notice = Some("Type the address of a world to join.".to_owned());
            return;
        }
        if let Err(error) = network::resolve_server_addr(address) {
            self.title.notice = Some(format!(
                "{address} is not an address that can be reached: {error}."
            ));
            return;
        }
        self.preferences.0.remember_server(address);
        let args = vec![
            "--from-title".to_owned(),
            "--connect".to_owned(),
            address.to_owned(),
        ];
        self.start(args);
    }

    /// Saves what the title remembers, starts the game with `args` and
    /// ends this instance.
    fn start(&mut self, args: Vec<String>) {
        if let Err(error) = self.preferences.0.save(&self.settings_file.0) {
            warn!("the settings could not be saved: {error}");
        }
        match launch::start(&args) {
            Ok(()) => {
                self.exit.write(AppExit::Success);
            }
            Err(error) => {
                self.title.notice = Some(format!("The game could not be started: {error}."));
            }
        }
    }
}

fn show_title(
    title: Res<Title>,
    preferences: Res<Preferences>,
    fields: Query<(&Field, &TextField)>,
    mut notice: Single<&mut Text, With<NoticeText>>,
    listing: Single<Entity, With<TitleListing>>,
    mut shown: Local<Option<Screen>>,
    mut commands: Commands,
) {
    if title.is_changed() {
        let text = title.notice.clone().unwrap_or_default();
        if notice.0 != text {
            notice.0 = text;
        }
    }
    if shown.as_ref() == Some(&title.screen) {
        return;
    }
    *shown = Some(title.screen.clone());
    // What was typed survives a change of screen and back.
    let typed = |wanted: Field| {
        fields
            .iter()
            .find(|(field, _)| **field == wanted)
            .map(|(_, field)| field.text.clone())
            .unwrap_or_default()
    };
    let kept = (
        typed(Field::Port),
        typed(Field::SleepPercent),
        typed(Field::Address),
    );
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|column| match &title.screen {
            Screen::Main => spawn_main(column, &title, &preferences),
            Screen::Worlds { to_host } => spawn_worlds(column, &title, *to_host, &kept),
            Screen::NewWorld => spawn_new_world(column),
            Screen::Join => spawn_join(column, &preferences, &kept.2),
            Screen::Delete(name) => spawn_delete(column, name),
        });
}

fn spawn_main(column: &mut ChildSpawnerCommands, title: &Title, preferences: &Preferences) {
    let last = preferences
        .0
        .last_world
        .as_ref()
        .and_then(|name| title.worlds.iter().find(|world| world.name == *name));
    if let Some(world) = last {
        ui::spawn_wide_button(
            column,
            format!("Continue {}", world.name),
            TitleButton::Continue,
        );
    }
    ui::spawn_wide_button(column, "New world", TitleButton::NewWorld);
    if !title.worlds.is_empty() {
        ui::spawn_wide_button(column, "Your worlds", TitleButton::Worlds);
        ui::spawn_wide_button(column, "Host for friends", TitleButton::Host);
    }
    ui::spawn_wide_button(column, "Join a world", TitleButton::Join);
    ui::spawn_wide_button(column, "Options", TitleButton::Options);
    ui::spawn_wide_button(column, "Quit", TitleButton::Quit);
}

fn spawn_worlds(
    column: &mut ChildSpawnerCommands,
    title: &Title,
    to_host: bool,
    kept: &(String, String, String),
) {
    column.spawn(ui::label(
        if to_host {
            "Host a world for friends"
        } else {
            "Your worlds"
        },
        HEADING_SIZE,
        TEXT_COLOR,
    ));
    if to_host {
        spawn_field_row(
            column,
            "Port",
            &kept.0,
            &DEFAULT_PORT.to_string(),
            5,
            Field::Port,
        );
        spawn_field_row(
            column,
            "Sleepers to end the night",
            &kept.1,
            &format!("{DEFAULT_SLEEP_PERCENT} percent"),
            3,
            Field::SleepPercent,
        );
        column.spawn(ui::label(
            "Friends on your network join your address; others need the port forwarded.",
            TEXT_SIZE,
            MUTED_TEXT_COLOR,
        ));
    }
    for world in &title.worlds {
        column
            .spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                let button = if to_host {
                    TitleButton::HostWorld(world.name.clone())
                } else {
                    TitleButton::Play(world.name.clone())
                };
                ui::spawn_button(row, world.name.clone(), button);
                row.spawn((
                    ui::label(describe(world), TEXT_SIZE, MUTED_TEXT_COLOR),
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                if !to_host {
                    ui::spawn_button(row, "Delete", TitleButton::AskDelete(world.name.clone()));
                }
            });
    }
    column
        .spawn(Node::default())
        .with_children(|row| ui::spawn_button(row, "Back", TitleButton::Back));
}

fn spawn_new_world(column: &mut ChildSpawnerCommands) {
    column.spawn(ui::label("A new world", HEADING_SIZE, TEXT_COLOR));
    spawn_field_row(
        column,
        "Name",
        "",
        "Green Valley",
        WORLD_NAME_LENGTH,
        Field::WorldName,
    );
    spawn_field_row(
        column,
        "Seed",
        "",
        "any number, or none",
        SEED_LENGTH,
        Field::Seed,
    );
    column
        .spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            ui::spawn_button(row, "Set out", TitleButton::Create);
            ui::spawn_button(row, "Back", TitleButton::Back);
        });
}

fn spawn_join(column: &mut ChildSpawnerCommands, preferences: &Preferences, address: &str) {
    column.spawn(ui::label("Join a world", HEADING_SIZE, TEXT_COLOR));
    spawn_field_row(
        column,
        "Address",
        address,
        "192.168.0.10 or farm.example.net:5717",
        ADDRESS_LENGTH,
        Field::Address,
    );
    column
        .spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            ui::spawn_button(row, "Join", TitleButton::Connect);
            ui::spawn_button(row, "Back", TitleButton::Back);
        });
    if !preferences.0.recent_servers.is_empty() {
        column.spawn(ui::label("Lately", TEXT_SIZE, MUTED_TEXT_COLOR));
        for address in &preferences.0.recent_servers {
            ui::spawn_button(
                column,
                address.clone(),
                TitleButton::ConnectTo(address.clone()),
            );
        }
    }
}

fn spawn_delete(column: &mut ChildSpawnerCommands, name: &str) {
    column.spawn(ui::label(
        format!("Delete {name} for good?"),
        HEADING_SIZE,
        TEXT_COLOR,
    ));
    column.spawn(ui::label(
        "Its fields, cabins and everyone's things there go with it.",
        TEXT_SIZE,
        MUTED_TEXT_COLOR,
    ));
    column
        .spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            ui::spawn_button(row, "Delete it", TitleButton::Delete(name.to_owned()));
            ui::spawn_button(row, "Keep it", TitleButton::Back);
        });
}

fn spawn_field_row(
    column: &mut ChildSpawnerCommands,
    name: &str,
    text: &str,
    placeholder: &str,
    limit: usize,
    field: Field,
) {
    column
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::label(name, TEXT_SIZE, MUTED_TEXT_COLOR),
                Node {
                    width: px(COLUMN_WIDTH - FIELD_WIDTH - 10.0),
                    ..default()
                },
            ));
            let first = matches!(field, Field::WorldName | Field::Address);
            ui::spawn_text_field(row, text, placeholder, limit, FIELD_WIDTH, first, field);
        });
}

/// When a world's clock stood and when it was last played.
fn describe(world: &WorldSummary) -> String {
    let clock = world.clock.map_or_else(
        || "a world yet to begin".to_owned(),
        |clock| clock.date().to_string(),
    );
    format!("{clock}, played {}", ago(world.saved))
}

fn ago(when: SystemTime) -> String {
    let passed = SystemTime::now()
        .duration_since(when)
        .unwrap_or(Duration::ZERO);
    let minutes = passed.as_secs() / 60;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => format!("{} days ago", minutes / 1440),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosting_terms_default_and_are_checked() {
        let terms = hosting_terms("", "").expect("defaults");
        assert_eq!(
            (terms.port, terms.sleep_percent),
            (DEFAULT_PORT, DEFAULT_SLEEP_PERCENT)
        );
        let terms = hosting_terms("6000", "75").expect("given");
        assert_eq!((terms.port, terms.sleep_percent), (6000, 75));
        assert!(hosting_terms("0", "").is_err());
        assert!(hosting_terms("", "101").is_err());
    }

    #[test]
    fn how_long_ago_reads_naturally() {
        let now = SystemTime::now();
        assert_eq!(ago(now), "just now");
        assert_eq!(ago(now - Duration::from_mins(5)), "5 min ago");
        assert_eq!(ago(now - Duration::from_hours(3)), "3 h ago");
        assert_eq!(ago(now - Duration::from_hours(49)), "2 days ago");
    }
}
