//! On-screen overlay: crosshair, clock and date, energy, money, and sleep
//! status.

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;
use messoria_calendar::{Weather, WorldTime};
use messoria_shared::{
    energy::Energy,
    protocol::{Asleep, CurrentWeather, Money, PlayerInput, SleepTally},
};

use crate::{
    actions::INTERACT_KEY_NAME, camera::View, clock::LocalClock, inventory::HOTBAR_BOTTOM,
    ui::visible_if,
};

const CROSSHAIR_SIZE: f32 = 4.0;
const OVERLAY_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.8);
const ENERGY_BAR_WIDTH: f32 = 160.0;
const ENERGY_COLOR: Color = Color::srgb(0.95, 0.8, 0.3);
const PANEL_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
/// Darkens the screen while the local character sleeps.
const SLEEP_VEIL_COLOR: Color = Color::srgba(0.0, 0.0, 0.05, 0.55);
const MARGIN: f32 = 16.0;

pub(crate) struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(
            Update,
            (
                show_crosshair,
                show_clock,
                show_energy,
                show_money,
                show_sleep_status,
            ),
        );
    }
}

#[derive(Component)]
struct Crosshair;

#[derive(Component)]
struct ClockText;

#[derive(Component)]
struct EnergyFill;

#[derive(Component)]
struct MoneyText;

#[derive(Component)]
struct SleepVeil;

#[derive(Component)]
struct SleepText;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Name::new("Crosshair"),
        Crosshair,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(CROSSHAIR_SIZE),
            height: px(CROSSHAIR_SIZE),
            margin: UiRect {
                left: px(-CROSSHAIR_SIZE / 2.0),
                top: px(-CROSSHAIR_SIZE / 2.0),
                ..default()
            },
            ..default()
        },
        BackgroundColor(OVERLAY_COLOR),
        Visibility::Hidden,
    ));

    commands
        .spawn((
            Name::new("Status panel"),
            Node {
                position_type: PositionType::Absolute,
                top: px(MARGIN),
                right: px(MARGIN),
                padding: UiRect::all(px(10)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(PANEL_COLOR),
        ))
        .with_children(|panel| {
            panel.spawn((ClockText, Text::new(""), TextFont::from_font_size(18.0)));
            panel
                .spawn((
                    Node {
                        width: px(ENERGY_BAR_WIDTH),
                        height: px(8),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                ))
                .with_child((
                    EnergyFill,
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(ENERGY_COLOR),
                ));
            panel.spawn((MoneyText, Text::new(""), TextFont::from_font_size(16.0)));
        });

    commands.spawn((
        Name::new("Sleep veil"),
        SleepVeil,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(SLEEP_VEIL_COLOR),
        Visibility::Hidden,
    ));

    commands.spawn((
        Name::new("Sleep status"),
        SleepText,
        Node {
            position_type: PositionType::Absolute,
            // Above the hotbar.
            bottom: px(HOTBAR_BOTTOM + 96.0),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(""),
        TextFont::from_font_size(20.0),
        TextLayout::justify(Justify::Center),
    ));
}

fn show_crosshair(view: Res<View>, mut crosshair: Single<&mut Visibility, With<Crosshair>>) {
    crosshair.set_if_neq(visible_if(view.captured));
}

fn show_clock(
    clock: Res<LocalClock>,
    weather: Query<&CurrentWeather>,
    mut text: Single<&mut Text, With<ClockText>>,
) {
    let Some(time) = clock.time() else {
        return;
    };
    let weather = match weather.single().map(|weather| weather.0) {
        Ok(Weather::Rain) => ", rain",
        Ok(Weather::Snow) => ", snow",
        Ok(Weather::Clear) | Err(_) => "",
    };
    let date = time.date();
    let shown = format!(
        "{}, {date}\n{}{weather}",
        date.weekday.short_name(),
        time.clock()
    );
    if text.0 != shown {
        text.0 = shown;
    }
}

fn show_energy(
    player: Query<&Energy, With<InputMarker<PlayerInput>>>,
    mut fill: Single<&mut Node, With<EnergyFill>>,
) {
    if let Ok(energy) = player.single() {
        let width = percent(energy.fraction() * 100.0);
        if fill.width != width {
            fill.width = width;
        }
    }
}

fn show_money(
    player: Query<&Money, (With<InputMarker<PlayerInput>>, Changed<Money>)>,
    mut text: Single<&mut Text, With<MoneyText>>,
) {
    if let Ok(money) = player.single() {
        text.0 = format!("{} coins", money.0.coins());
    }
}

fn show_sleep_status(
    clock: Res<LocalClock>,
    tally: Query<&SleepTally>,
    player: Query<Has<Asleep>, With<InputMarker<PlayerInput>>>,
    mut veil: Single<&mut Visibility, (With<SleepVeil>, Without<SleepText>)>,
    mut text: Single<&mut Text, With<SleepText>>,
) {
    let asleep = player.single().unwrap_or(false);
    let bedtime = clock.time().is_some_and(WorldTime::is_bedtime);
    veil.set_if_neq(visible_if(asleep));

    let shown = match (asleep, tally.single()) {
        (true, Ok(tally)) => format!(
            "Sleeping, {} of {} players needed. Press {INTERACT_KEY_NAME} to get up.",
            tally.asleep, tally.required
        ),
        (true, Err(_)) => "Sleeping".to_owned(),
        (false, _) if bedtime => format!("Bedtime: press {INTERACT_KEY_NAME} at a bed to sleep"),
        (false, _) => String::new(),
    };
    if text.0 != shown {
        text.0 = shown;
    }
}
