//! The character creator: on a player's first arrival, the window where
//! they say their name and make themselves a look, seen in the looking
//! glass as it is made.
//!
//! The server tells a client its player is new to the world. A player who
//! has made a look before (it is kept in their profile) is introduced with
//! it at once; anyone else gets the window, which stays until they begin.
//! The look is one of each the wardrobe offers, picked with arrows; the
//! name is typed. What is chosen is sent to the server, which dresses the
//! character for everyone, and kept in the profile for other worlds.

use std::path::PathBuf;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{Characters, Look};
use messoria_save::Profile;
use messoria_shared::{
    content::Content,
    players::{NAME_LENGTH, proper_name},
    protocol::{ActionChannel, Appearance, Introduce, Newcomer, PlayerInput},
};

use crate::{
    controls::FirstArrival,
    mirror::{MIRROR_SIZE, Mirror},
    panels::OpenPanel,
    ui::{self, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, TITLE_SIZE, TextField},
};

/// Width of the column of choices, and of each choice's value.
const CHOICES_WIDTH: f32 = 330.0;
const VALUE_WIDTH: f32 = 170.0;
/// How a color is shown: a square of it.
const SWATCH_SIZE: f32 = 18.0;
const NAME_FIELD_WIDTH: f32 = 230.0;

pub(crate) struct CreatorPlugin {
    pub profile: Profile,
    /// Where the profile is kept, if it is kept at all.
    pub file: Option<PathBuf>,
}

impl Plugin for CreatorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Identity {
            profile: self.profile.clone(),
            file: self.file.clone(),
        })
        .init_resource::<Draft>()
        .add_systems(Startup, spawn_creator_window)
        .add_systems(PreUpdate, receive.after(MessageSystems::Receive))
        .add_systems(
            Update,
            (press_buttons, show_creator_window, close_when_introduced).chain(),
        );
    }
}

/// The local player's profile, and where it is saved.
#[derive(Resource)]
struct Identity {
    profile: Profile,
    file: Option<PathBuf>,
}

/// The look being made; the name is typed into its field.
#[derive(Resource, Default)]
struct Draft {
    look: Option<Look>,
}

#[derive(Component)]
struct CreatorWindow;

#[derive(Component)]
struct CreatorListing;

#[derive(Component)]
struct NameField;

/// Why the player cannot set out yet.
#[derive(Component)]
struct CreatorHint;

/// A part of the look picked with arrows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trait {
    Skin,
    Eyes,
    Hair,
    HairColor,
    Outfit,
}

impl Trait {
    const ALL: [Self; 5] = [
        Self::Skin,
        Self::Eyes,
        Self::Hair,
        Self::HairColor,
        Self::Outfit,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Skin => "Skin",
            Self::Eyes => "Eyes",
            Self::Hair => "Hair",
            Self::HairColor => "Hair color",
            Self::Outfit => "Clothes",
        }
    }

    /// Moves the look's part `steps` along the wardrobe's choices, round
    /// and round.
    fn step(self, characters: &Characters, look: &mut Look, steps: isize) {
        let wardrobe = &characters.wardrobe;
        let next = |count: usize, at: usize| {
            let count = isize::try_from(count).unwrap_or(1);
            let at = isize::try_from(at).unwrap_or(0);
            usize::try_from((at + steps).rem_euclid(count)).unwrap_or(0)
        };
        let pick = |choices: &[String], current: &str| {
            let at = choices.iter().position(|choice| choice == current);
            choices[next(choices.len(), at.unwrap_or(0))].clone()
        };
        let pick_color = |choices: &[(f32, f32, f32)], current: (f32, f32, f32)| {
            let at = choices.iter().position(|&choice| choice == current);
            choices[next(choices.len(), at.unwrap_or(0))]
        };
        match self {
            Self::Skin => look.body = pick(&wardrobe.bodies, &look.body),
            Self::Eyes => look.eyes = pick_color(&wardrobe.eye_colors, look.eyes),
            Self::Hair => look.hair = pick(&wardrobe.hair, &look.hair),
            Self::HairColor => look.hair_color = pick_color(&wardrobe.hair_colors, look.hair_color),
            Self::Outfit => look.outfit = pick(&wardrobe.outfits, &look.outfit),
        }
    }
}

#[derive(Component, Clone, Copy)]
enum CreatorButton {
    Previous(Trait),
    Next(Trait),
    Begin,
}

fn spawn_creator_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Character creator"),
            CreatorWindow,
            ui::screen(),
            BackgroundColor(ui::BACKDROP_COLOR),
            Visibility::Hidden,
        ))
        .with_child((
            CreatorListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(20)),
                row_gap: px(12),
                ..default()
            }),
        ));
}

/// Answers the server's word that the player is new here: with the look
/// from the profile, or by opening the window.
fn receive(
    content: Res<Content>,
    identity: Res<Identity>,
    mut newcomers: Query<&mut MessageReceiver<Newcomer>, With<Client>>,
    mut senders: Query<&mut MessageSender<Introduce>, With<Client>>,
    mut draft: ResMut<Draft>,
    mut panel: ResMut<OpenPanel>,
    mut arrival: ResMut<FirstArrival>,
) {
    for mut receiver in &mut newcomers {
        for Newcomer in receiver.receive() {
            arrival.0 = true;
            let characters = content.characters();
            let kept = identity
                .profile
                .look
                .clone()
                .filter(|look| characters.offers(look))
                .zip(identity.profile.name.clone());
            if let Some((look, name)) = kept
                && let Ok(mut sender) = senders.single_mut()
            {
                sender.send::<ActionChannel>(Introduce { name, look });
                continue;
            }
            draft.look = Some(characters.look_for(identity.profile.player_id));
            *panel = OpenPanel::Creator;
        }
    }
}

fn press_buttons(
    content: Res<Content>,
    panel: Res<OpenPanel>,
    buttons: Query<(&Interaction, &CreatorButton), Changed<Interaction>>,
    name: Query<&TextField, With<NameField>>,
    mut hint: Query<&mut Text, With<CreatorHint>>,
    mut draft: ResMut<Draft>,
    mut identity: ResMut<Identity>,
    mut senders: Query<&mut MessageSender<Introduce>, With<Client>>,
) {
    if *panel != OpenPanel::Creator {
        return;
    }
    let typed = name.single().ok();
    let mut begin = typed.is_some_and(|field| field.submitted);
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            CreatorButton::Previous(part) => {
                if let Some(look) = &mut draft.look {
                    part.step(content.characters(), look, -1);
                }
            }
            CreatorButton::Next(part) => {
                if let Some(look) = &mut draft.look {
                    part.step(content.characters(), look, 1);
                }
            }
            CreatorButton::Begin => begin = true,
        }
    }
    if !begin {
        return;
    }
    let name = typed.and_then(|field| proper_name(&field.text));
    let (Some(name), Some(look)) = (name, draft.look.clone()) else {
        for mut hint in &mut hint {
            "Give yourself a name first.".clone_into(&mut hint.0);
        }
        return;
    };
    if let Ok(mut sender) = senders.single_mut() {
        sender.send::<ActionChannel>(Introduce {
            name: name.clone(),
            look: look.clone(),
        });
    }
    identity.profile.name = Some(name);
    identity.profile.look = Some(look);
    if let Some(file) = &identity.file
        && let Err(error) = identity.profile.save(file)
    {
        warn!("the profile could not be saved: {error}");
    }
}

/// The window closes once the server has dressed the character as asked.
fn close_when_introduced(
    mut panel: ResMut<OpenPanel>,
    player: Query<Ref<Appearance>, With<InputMarker<PlayerInput>>>,
) {
    if *panel == OpenPanel::Creator
        && player
            .single()
            .is_ok_and(|appearance| appearance.is_changed() && !appearance.is_added())
    {
        *panel = OpenPanel::None;
    }
}

fn show_creator_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    draft: Res<Draft>,
    mut mirror: ResMut<Mirror>,
    mut window: Single<&mut Visibility, With<CreatorWindow>>,
    listing: Single<Entity, With<CreatorListing>>,
    mut commands: Commands,
) {
    let open = *panel == OpenPanel::Creator;
    window.set_if_neq(ui::visible_if(open));
    let Some(look) = draft.look.as_ref().filter(|_| open) else {
        return;
    };
    mirror.show(look);
    if !panel.is_changed() && !draft.is_changed() {
        return;
    }
    let image = mirror.image.clone();
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| {
            listing.spawn(ui::label(
                "Who comes to the valley?",
                TITLE_SIZE,
                TEXT_COLOR,
            ));
            listing
                .spawn(Node {
                    column_gap: px(24),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        width: px(CHOICES_WIDTH),
                        ..default()
                    })
                    .with_children(|column| {
                        spawn_name_row(column);
                        for part in Trait::ALL {
                            spawn_choice_row(column, &content, look, part);
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
            listing
                .spawn(Node {
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    ui::spawn_button(row, "Set out", CreatorButton::Begin);
                    row.spawn((CreatorHint, ui::label("", TEXT_SIZE, MUTED_TEXT_COLOR)));
                });
        });
}

fn spawn_name_row(column: &mut ChildSpawnerCommands) {
    column
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::label("Name", TEXT_SIZE, MUTED_TEXT_COLOR),
                Node {
                    width: px(90),
                    ..default()
                },
            ));
            ui::spawn_text_field(
                row,
                "",
                "What are you called?",
                NAME_LENGTH,
                NAME_FIELD_WIDTH,
                true,
                NameField,
            );
        });
}

fn spawn_choice_row(
    column: &mut ChildSpawnerCommands,
    content: &Content,
    look: &Look,
    part: Trait,
) {
    column
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                ui::label(part.name(), TEXT_SIZE, MUTED_TEXT_COLOR),
                Node {
                    width: px(90),
                    ..default()
                },
            ));
            ui::spawn_button(row, "<", CreatorButton::Previous(part));
            row.spawn((
                Node {
                    width: px(VALUE_WIDTH),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Visibility::default(),
            ))
            .with_children(|value| match part {
                Trait::Eyes => spawn_swatch(value, look.eyes),
                Trait::HairColor => spawn_swatch(value, look.hair_color),
                Trait::Skin => {
                    value.spawn(ui::label(skin_name(&look.body), TEXT_SIZE, TEXT_COLOR));
                }
                Trait::Hair => {
                    value.spawn(ui::label(skin_name(&look.hair), TEXT_SIZE, TEXT_COLOR));
                }
                Trait::Outfit => {
                    value.spawn(ui::label(
                        garment_name(content, &look.outfit),
                        TEXT_SIZE,
                        TEXT_COLOR,
                    ));
                }
            });
            ui::spawn_button(row, ">", CreatorButton::Next(part));
        });
}

/// A square of a color.
pub(crate) fn spawn_swatch(parent: &mut ChildSpawnerCommands, (red, green, blue): (f32, f32, f32)) {
    parent.spawn((
        Node {
            width: px(SWATCH_SIZE),
            height: px(SWATCH_SIZE),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(Color::srgb(red, green, blue)),
        BorderColor::all(TEXT_COLOR),
    ));
}

/// What a skin file is called on screen: `hair-short.png` is "Short".
pub(crate) fn skin_name(file: &str) -> String {
    let stem = file.rsplit('/').next().unwrap_or(file);
    let stem = stem.strip_suffix(".png").unwrap_or(stem);
    let name = stem.split_once('-').map_or(stem, |(_, name)| name);
    let mut letters = name.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

/// What an outfit or hat is called: the garment's name if one paints that
/// skin, else the skin's.
pub(crate) fn garment_name(content: &Content, skin: &str) -> String {
    content
        .garment_with(skin)
        .map_or_else(|| skin_name(skin), |item| content.item(item).name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_are_named_by_what_follows_their_kind() {
        assert_eq!(skin_name("hair-short.png"), "Short");
        assert_eq!(skin_name("body-fair.png"), "Fair");
        assert_eq!(skin_name("plain.png"), "Plain");
    }
}
