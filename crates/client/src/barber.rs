//! The barber's counter: hair and its color picked with arrows, seen in
//! the looking glass, and paid for at a word.
//!
//! The barber keeps a shop like any other, so its window is the shop
//! window; this is what that window shows at the barber's instead of
//! wares. The server cuts the hair and takes the coins.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::Look;
use messoria_shared::{
    content::Content,
    protocol::{ActionChannel, Appearance, PlayerInput, Restyle},
};

use crate::{
    creator::{skin_name, spawn_swatch},
    mirror::MIRROR_SIZE,
    panels::OpenPanel,
    ui::{self, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

const VALUE_WIDTH: f32 = 150.0;

pub(crate) struct BarberPlugin;

impl Plugin for BarberPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Styling>()
            .add_systems(Update, press_buttons);
    }
}

/// The hair being considered at the barber's: none until an arrow is
/// pressed, which starts from the player's own.
#[derive(Resource, Default)]
pub(crate) struct Styling {
    hair: Option<(String, (f32, f32, f32))>,
}

/// `look` with the hair being considered.
pub(crate) fn styled(styling: &Styling, look: &Look) -> Look {
    let mut styled = look.clone();
    if let Some((hair, color)) = &styling.hair {
        styled.hair.clone_from(hair);
        styled.hair_color = *color;
    }
    styled
}

#[derive(Component, Clone, Copy)]
enum StyleButton {
    Hair(isize),
    Color(isize),
    Pay,
}

/// Lays out the barber's counter under `listing`: the look being
/// considered, `styled`, against the player's own, and what it costs.
pub(crate) fn show(
    listing: &mut ChildSpawnerCommands,
    content: &Content,
    styled: &Look,
    look: &Look,
    image: Handle<Image>,
    coins: u32,
) {
    let (hair, color) = (&styled.hair, styled.hair_color);
    let price = content.village().haircut.price;
    let same = *hair == look.hair && color == look.hair_color;
    listing
        .spawn(Node {
            column_gap: px(20),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|column| {
                spawn_row(
                    column,
                    "Hair",
                    StyleButton::Hair(-1),
                    StyleButton::Hair(1),
                    |value| {
                        value.spawn(ui::label(skin_name(hair), TEXT_SIZE, TEXT_COLOR));
                    },
                );
                spawn_row(
                    column,
                    "Color",
                    StyleButton::Color(-1),
                    StyleButton::Color(1),
                    |value| {
                        spawn_swatch(value, color);
                    },
                );
                if same {
                    column.spawn(ui::label(
                        format!("That is how you look now. A new look costs {price} coins."),
                        TEXT_SIZE,
                        MUTED_TEXT_COLOR,
                    ));
                } else if coins < price {
                    column.spawn(ui::label(
                        format!("A new look costs {price} coins, more than you have."),
                        TEXT_SIZE,
                        MUTED_TEXT_COLOR,
                    ));
                } else {
                    column.spawn(Node::default()).with_children(|row| {
                        ui::spawn_button(
                            row,
                            format!("New look for {price} coins"),
                            StyleButton::Pay,
                        );
                    });
                }
            });
            row.spawn((
                ImageNode::new(image),
                Node {
                    width: px(MIRROR_SIZE.0),
                    height: px(MIRROR_SIZE.1),
                    ..default()
                },
            ));
        });
}

fn spawn_row(
    column: &mut ChildSpawnerCommands,
    name: &str,
    previous: StyleButton,
    next: StyleButton,
    value: impl FnOnce(&mut ChildSpawnerCommands),
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
                    width: px(70),
                    ..default()
                },
            ));
            ui::spawn_button(row, "<", previous);
            row.spawn((
                Node {
                    width: px(VALUE_WIDTH),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Visibility::default(),
            ))
            .with_children(value);
            ui::spawn_button(row, ">", next);
        });
}

/// Steps `index` along `count` choices, round and round.
fn stepped(count: usize, at: Option<usize>, steps: isize) -> usize {
    let count = isize::try_from(count).unwrap_or(1);
    let at = isize::try_from(at.unwrap_or(0)).unwrap_or(0);
    usize::try_from((at + steps).rem_euclid(count)).unwrap_or(0)
}

fn press_buttons(
    content: Res<Content>,
    panel: Res<OpenPanel>,
    buttons: Query<(&Interaction, &StyleButton), Changed<Interaction>>,
    player: Query<&Appearance, With<InputMarker<PlayerInput>>>,
    mut styling: ResMut<Styling>,
    mut sender: Query<&mut MessageSender<Restyle>, With<Client>>,
) {
    // The considered hair starts over with each visit.
    if panel.is_changed() && !matches!(*panel, OpenPanel::Shop(_)) && styling.hair.is_some() {
        styling.hair = None;
    }
    let wardrobe = &content.characters().wardrobe;
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok(own) = player.single() else {
            continue;
        };
        let (hair, color) = styling
            .hair
            .get_or_insert_with(|| (own.0.hair.clone(), own.0.hair_color));
        match *button {
            StyleButton::Hair(steps) => {
                let at = wardrobe.hair.iter().position(|style| style == hair);
                hair.clone_from(&wardrobe.hair[stepped(wardrobe.hair.len(), at, steps)]);
            }
            StyleButton::Color(steps) => {
                let at = wardrobe
                    .hair_colors
                    .iter()
                    .position(|choice| choice == color);
                *color = wardrobe.hair_colors[stepped(wardrobe.hair_colors.len(), at, steps)];
            }
            StyleButton::Pay => {
                if let Ok(mut sender) = sender.single_mut() {
                    sender.send::<ActionChannel>(Restyle {
                        hair: hair.clone(),
                        hair_color: *color,
                    });
                }
            }
        }
    }
}
