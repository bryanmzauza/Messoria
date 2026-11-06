//! Talking to villagers and giving them gifts, and the list of everyone in
//! the village and how the player stands with each.
//!
//! `F` on a villager asks the server for a word; what they say opens in a
//! window with the player's hearts of friendship, a button to give them
//! what the player holds, and goodbye. Walking away closes it. `V` lists
//! the villagers: who they are, what they keep, where they are and how
//! many hearts the player has with each. Friendship itself is the
//! server's; this only shows what it sends.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{MOST_HEARTS, Reaction, VillagerId, hearts};
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{
        ActionChannel, Belongings, Dialogue, Doing, Friendship, Friendships, GiveGift, PlayerInput,
        Position, TalkTo, Villager,
    },
    tools,
};

use crate::{
    actions::{Aim, AimSystems, INTERACT_KEY},
    camera::{LookSystems, View},
    furniture::FurnitureSystems,
    inventory::HeldSlot,
    panels::OpenPanel,
    shops::ShopSystems,
    ui::{self, ACCENT_COLOR, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

/// Opens the list of villagers.
pub(crate) const VILLAGERS_KEY: KeyCode = KeyCode::KeyV;
/// How the hearts of a friendship are drawn: small squares, full and
/// empty, in a row.
const HEART_SIZE: f32 = 10.0;
const HEART_GAP: f32 = 3.0;
const HEART_COLOR: Color = Color::srgb(0.85, 0.3, 0.35);
const EMPTY_HEART_COLOR: Color = Color::srgba(0.85, 0.3, 0.35, 0.25);

pub(crate) struct TalkPlugin;

impl Plugin for TalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hearts>()
            .init_resource::<Conversation>()
            .add_systems(Startup, (spawn_talk_window, spawn_villagers_window))
            .add_systems(PreUpdate, receive.after(MessageSystems::Receive))
            .add_systems(
                Update,
                (
                    talk.after(AimSystems)
                        .after(ShopSystems)
                        .after(FurnitureSystems)
                        .before(LookSystems),
                    toggle_villagers,
                    close_far_talk,
                    show_talk_window,
                    show_villagers_window,
                    press_buttons,
                ),
            );
    }
}

/// The player's friendships, as the server last sent them.
#[derive(Resource, Default)]
pub(crate) struct Hearts(pub HashMap<VillagerId, Friendship>);

impl Hearts {
    pub(crate) fn with(&self, villager: VillagerId) -> u16 {
        self.0
            .get(&villager)
            .map_or(0, |friendship| hearts(friendship.points))
    }
}

/// What the villager being talked to last said.
#[derive(Resource, Default)]
struct Conversation(Option<Dialogue>);

#[derive(Component)]
struct TalkWindow;

#[derive(Component)]
struct TalkListing;

#[derive(Component)]
struct VillagersWindow;

#[derive(Component)]
struct VillagersListing;

#[derive(Component, Clone, Copy)]
enum TalkButton {
    Give(VillagerId, u8),
    Goodbye,
}

fn spawn_talk_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Talk window"),
            TalkWindow,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_child((
            TalkListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(10),
                width: px(520),
                ..default()
            }),
        ));
}

fn spawn_villagers_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Villagers window"),
            VillagersWindow,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_child((
            VillagersListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(6),
                min_width: px(560),
                ..default()
            }),
        ));
}

/// Keeps what the server says about villagers: dialogue and friendships.
fn receive(
    mut dialogues: Query<&mut MessageReceiver<Dialogue>, With<Client>>,
    mut friendships: Query<&mut MessageReceiver<Friendships>, With<Client>>,
    villagers: Query<(Entity, &Villager)>,
    mut conversation: ResMut<Conversation>,
    mut hearts: ResMut<Hearts>,
    mut panel: ResMut<OpenPanel>,
) {
    for mut receiver in &mut dialogues {
        for dialogue in receiver.receive() {
            if let Some((entity, _)) = villagers
                .iter()
                .find(|(_, villager)| villager.0 == dialogue.villager)
            {
                *panel = OpenPanel::Talk(entity);
            }
            conversation.0 = Some(dialogue);
        }
    }
    for mut receiver in &mut friendships {
        for Friendships(all) in receiver.receive() {
            hearts.0 = all
                .into_iter()
                .map(|friendship| (friendship.villager, friendship))
                .collect();
        }
    }
}

/// A word with the villager keeping the shop whose window is open, asked
/// for from that window.
#[derive(Component, Clone, Copy)]
pub(crate) struct WordWithKeeper(pub VillagerId);

/// `F` on a villager asks them for a word, or ends it if the window is
/// open. Consumes the key press, so it does not also harvest.
fn talk(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    view: Res<View>,
    aim: Res<Aim>,
    villagers: Query<&Villager>,
    mut panel: ResMut<OpenPanel>,
    mut sender: Query<&mut MessageSender<TalkTo>, With<Client>>,
) {
    if !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    if let OpenPanel::Talk(_) = *panel {
        *panel = OpenPanel::None;
        keys.clear_just_pressed(INTERACT_KEY);
        return;
    }
    if !view.captured {
        return;
    }
    let Some(villager) = aim.villager.and_then(|entity| villagers.get(entity).ok()) else {
        return;
    };
    if let Ok(mut sender) = sender.single_mut() {
        sender.send::<ActionChannel>(TalkTo {
            villager: villager.0,
        });
    }
    keys.clear_just_pressed(INTERACT_KEY);
}

fn toggle_villagers(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<OpenPanel>) {
    if keys.just_pressed(VILLAGERS_KEY) {
        *panel = match *panel {
            OpenPanel::Villagers => OpenPanel::None,
            OpenPanel::Menu | OpenPanel::Options => return,
            _ => OpenPanel::Villagers,
        };
    }
}

fn close_far_talk(
    mut panel: ResMut<OpenPanel>,
    villagers: Query<&Position, With<Villager>>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
) {
    let OpenPanel::Talk(villager) = *panel else {
        return;
    };
    let near = player
        .single()
        .ok()
        .zip(villagers.get(villager).ok())
        .is_some_and(|(feet, at)| tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, at.0 + Vec3::Y));
    if !near {
        *panel = OpenPanel::None;
    }
}

fn show_talk_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    conversation: Res<Conversation>,
    hearts: Res<Hearts>,
    held: Res<HeldSlot>,
    player: Query<Ref<Belongings>, With<InputMarker<PlayerInput>>>,
    villagers: Query<&Villager>,
    mut window: Single<&mut Visibility, With<TalkWindow>>,
    listing: Single<Entity, With<TalkListing>>,
    mut commands: Commands,
) {
    let open = matches!(*panel, OpenPanel::Talk(_));
    window.set_if_neq(ui::visible_if(open));
    let (OpenPanel::Talk(entity), Some(dialogue), Ok(belongings)) =
        (*panel, &conversation.0, player.single())
    else {
        return;
    };
    let changed = panel.is_changed()
        || conversation.is_changed()
        || hearts.is_changed()
        || held.is_changed()
        || belongings.is_changed();
    if !changed {
        return;
    }
    let Ok(villager) = villagers.get(entity) else {
        return;
    };
    let definition = content.villager(villager.0);
    let friendship = hearts.0.get(&villager.0).copied();
    let gave_today = friendship.is_some_and(|friendship| friendship.gave_today);
    let held_item = belongings.0.slot(held.0).map(|stack| stack.item);
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| {
            listing
                .spawn(Node {
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(ui::label(definition.name.clone(), HEADING_SIZE, TEXT_COLOR));
                    spawn_hearts(row, hearts.with(villager.0));
                });
            listing.spawn(ui::label(dialogue.says.clone(), TEXT_SIZE, TEXT_COLOR));
            if let Some(reaction) = dialogue.reaction {
                listing.spawn(ui::label(
                    reaction_note(reaction),
                    TEXT_SIZE,
                    MUTED_TEXT_COLOR,
                ));
            }
            if let Some((item, count)) = dialogue.given {
                listing.spawn(ui::label(
                    format!(
                        "{} gives you {count} {}.",
                        definition
                            .name
                            .split(' ')
                            .next()
                            .unwrap_or(&definition.name),
                        content.item(item).name.to_lowercase()
                    ),
                    TEXT_SIZE,
                    ACCENT_COLOR,
                ));
            }
            listing
                .spawn(Node {
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|row| {
                    match held_item {
                        Some(item) if !gave_today => {
                            let slot = u8::try_from(held.0).expect("hotbar slots fit in u8");
                            ui::spawn_button(
                                row,
                                format!("Give {}", content.item(item).name.to_lowercase()),
                                TalkButton::Give(villager.0, slot),
                            );
                        }
                        Some(_) => {
                            row.spawn(ui::label(
                                "A gift a day is plenty.",
                                TEXT_SIZE,
                                MUTED_TEXT_COLOR,
                            ));
                        }
                        None => {
                            row.spawn(ui::label(
                                "Hold something to give it.",
                                TEXT_SIZE,
                                MUTED_TEXT_COLOR,
                            ));
                        }
                    }
                    ui::spawn_button(row, "Goodbye", TalkButton::Goodbye);
                });
        });
}

fn show_villagers_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    hearts: Res<Hearts>,
    villagers: Query<(&Villager, &Doing)>,
    mut window: Single<&mut Visibility, With<VillagersWindow>>,
    listing: Single<Entity, With<VillagersListing>>,
    mut shown: Local<Vec<(VillagerId, Doing)>>,
    mut commands: Commands,
) {
    let open = *panel == OpenPanel::Villagers;
    window.set_if_neq(ui::visible_if(open));
    let mut seen: Vec<(VillagerId, Doing)> = villagers
        .iter()
        .map(|(villager, doing)| (villager.0, *doing))
        .collect();
    seen.sort_by_key(|(villager, _)| *villager);
    let changed = panel.is_changed() || hearts.is_changed() || *shown != seen;
    if !open || !changed {
        return;
    }
    *shown = seen;
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| {
            listing.spawn(ui::label("The village", HEADING_SIZE, TEXT_COLOR));
            for (id, villager) in content.villagers() {
                let keeps = villager
                    .keeps
                    .map(|shop| format!("keeps the {}", content.shop(shop).name.to_lowercase()))
                    .unwrap_or_default();
                let doing = shown
                    .iter()
                    .find(|(seen, _)| *seen == id)
                    .map_or("out of sight", |(_, doing)| whereabouts(*doing));
                let talked = hearts
                    .0
                    .get(&id)
                    .is_some_and(|friendship| friendship.talked_today);
                listing
                    .spawn(Node {
                        column_gap: px(12),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            ui::label(villager.name.clone(), TEXT_SIZE, TEXT_COLOR),
                            Node {
                                width: px(150),
                                ..default()
                            },
                        ));
                        row.spawn(Node {
                            width: px(150),
                            ..default()
                        })
                        .with_children(|cell| spawn_hearts(cell, hearts.with(id)));
                        row.spawn((
                            ui::label(keeps, TEXT_SIZE, MUTED_TEXT_COLOR),
                            Node {
                                width: px(140),
                                ..default()
                            },
                        ));
                        let state = if talked {
                            format!("{doing}, talked today")
                        } else {
                            doing.to_owned()
                        };
                        row.spawn(ui::label(state, TEXT_SIZE, MUTED_TEXT_COLOR));
                    });
            }
        });
}

fn press_buttons(
    buttons: Query<(&Interaction, &TalkButton), Changed<Interaction>>,
    words: Query<(&Interaction, &WordWithKeeper), Changed<Interaction>>,
    mut panel: ResMut<OpenPanel>,
    mut gifts: Query<&mut MessageSender<GiveGift>, With<Client>>,
    mut talks: Query<&mut MessageSender<TalkTo>, With<Client>>,
) {
    for (interaction, word) in &words {
        if *interaction == Interaction::Pressed
            && let Ok(mut sender) = talks.single_mut()
        {
            sender.send::<ActionChannel>(TalkTo { villager: word.0 });
        }
    }
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            TalkButton::Give(villager, slot) => {
                if let Ok(mut sender) = gifts.single_mut() {
                    sender.send::<ActionChannel>(GiveGift { villager, slot });
                }
            }
            TalkButton::Goodbye => *panel = OpenPanel::None,
        }
    }
}

/// Hearts of friendship as a row of full and empty hearts.
fn spawn_hearts(parent: &mut ChildSpawnerCommands, full: u16) {
    parent
        .spawn(Node {
            column_gap: px(HEART_GAP),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            for heart in 0..MOST_HEARTS {
                row.spawn((
                    Node {
                        width: px(HEART_SIZE),
                        height: px(HEART_SIZE),
                        border_radius: BorderRadius::all(px(2)),
                        ..default()
                    },
                    BackgroundColor(if heart < full {
                        HEART_COLOR
                    } else {
                        EMPTY_HEART_COLOR
                    }),
                ));
            }
        });
}

fn reaction_note(reaction: Reaction) -> &'static str {
    match reaction {
        Reaction::Loved => "They love it.",
        Reaction::Liked => "They like it.",
        Reaction::Neutral => "They take it politely.",
        Reaction::Disliked => "They would rather not have it.",
    }
}

fn whereabouts(doing: Doing) -> &'static str {
    match doing {
        Doing::Standing => "about the village",
        Doing::Walking => "on the way somewhere",
        Doing::Sitting => "sitting down",
        Doing::Tending => "in the garden",
        Doing::Indoors => "at home",
    }
}
