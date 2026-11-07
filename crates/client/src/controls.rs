//! The card of controls: every key the game answers to, shown on a
//! player's first arrival in a world and whenever F1 is pressed.

use bevy::prelude::*;

use crate::{
    panels::OpenPanel,
    ui::{self, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

pub(crate) const CONTROLS_KEY: KeyCode = KeyCode::F1;
const KEY_COLUMN_WIDTH: f32 = 130.0;

/// What each key does, as the card lists it, a line each.
const CONTROLS: [(&str, &str); 13] = [
    (
        "Mouse",
        "Look around; click the window to take the mouse, Esc to let it go",
    ),
    ("W A S D", "Walk"),
    ("Shift", "Run"),
    ("Space", "Jump"),
    ("1 to 0, wheel", "Pick what you hold"),
    (
        "Left click",
        "Use what you hold: dig, till, water, plant, build, eat",
    ),
    (
        "Right click",
        "Use it the other way: raise the ground with the shovel",
    ),
    (
        "F",
        "Talk, harvest, pick, open what you look at, sleep in a bed",
    ),
    ("E", "Backpack, and giving money"),
    ("V", "The villagers"),
    ("F5", "First or third person"),
    ("F1", "This card"),
    ("Esc", "Close a window, or the game menu"),
];

pub(crate) struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FirstArrival>()
            .add_systems(Startup, spawn_card)
            .add_systems(Update, (toggle, open_on_arrival, show_card, press_close));
    }
}

/// Set when the server says the player is new to this world; the card
/// opens once they have made their look.
#[derive(Resource, Default)]
pub(crate) struct FirstArrival(pub bool);

#[derive(Component)]
struct ControlsCard;

#[derive(Component)]
struct CloseButton;

fn spawn_card(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Controls card"),
            ControlsCard,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen
                .spawn(ui::window(Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(20)),
                    row_gap: px(4),
                    ..default()
                }))
                .with_children(|window| {
                    window.spawn((
                        ui::label("Controls", HEADING_SIZE, TEXT_COLOR),
                        Node {
                            margin: UiRect::bottom(px(6)),
                            ..default()
                        },
                    ));
                    for (key, does) in CONTROLS {
                        window
                            .spawn(Node {
                                column_gap: px(12),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    ui::label(key, TEXT_SIZE, MUTED_TEXT_COLOR),
                                    TextLayout::no_wrap(),
                                    Node {
                                        width: px(KEY_COLUMN_WIDTH),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                                row.spawn((
                                    ui::label(does, TEXT_SIZE, TEXT_COLOR),
                                    TextLayout::no_wrap(),
                                ));
                            });
                    }
                    window
                        .spawn(Node {
                            margin: UiRect::top(px(10)),
                            ..default()
                        })
                        .with_children(|row| {
                            ui::spawn_button(row, "Close", CloseButton);
                        });
                });
        });
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<OpenPanel>) {
    if keys.just_pressed(CONTROLS_KEY) && !panel.takes_typing() {
        *panel = match *panel {
            OpenPanel::Controls => OpenPanel::None,
            OpenPanel::Menu | OpenPanel::Options => return,
            _ => OpenPanel::Controls,
        };
    }
}

/// Opens the card for a newcomer, once no other window is in the way.
fn open_on_arrival(mut arrival: ResMut<FirstArrival>, mut panel: ResMut<OpenPanel>) {
    if arrival.0 && *panel == OpenPanel::None {
        arrival.0 = false;
        *panel = OpenPanel::Controls;
    }
}

fn show_card(panel: Res<OpenPanel>, mut card: Single<&mut Visibility, With<ControlsCard>>) {
    if panel.is_changed() {
        card.set_if_neq(ui::visible_if(*panel == OpenPanel::Controls));
    }
}

fn press_close(
    buttons: Query<&Interaction, (With<CloseButton>, Changed<Interaction>)>,
    mut panel: ResMut<OpenPanel>,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        *panel = OpenPanel::None;
    }
}
