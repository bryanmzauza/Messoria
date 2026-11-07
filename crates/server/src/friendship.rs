//! Getting to know the villagers: talking to them, giving them gifts, and
//! the friendship each player builds with each villager.
//!
//! Friendship is counted in points, a hundred to a heart. A first talk each
//! day earns a few, a gift earns more or less by how the villager takes it,
//! and reaching new hearts brings small things back: warmer words, a
//! present from the villager, a friend's price at their counter. Each
//! player's friendships are theirs alone, kept in their file, and sent to
//! their client whenever they change.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::{Quality, Reaction, VillagerId, hearts};
use messoria_save::FriendshipState;
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{
        Asleep, Belongings, CurrentWeather, Dialogue, FeedbackChannel, Friendship, Friendships,
        GiveGift, Happened, Notice, Position, TalkTo, Villager, WorldClock,
    },
    tools,
};

use crate::{
    feedback::{Show, Tell},
    players::ControlledCharacter,
    villagers::Attending,
};

/// Friendship earned by the first talk of the day.
const TALK_POINTS: u16 = 20;
/// Friendship earned, or lost, by a gift the villager loves, likes, shrugs
/// at or dislikes.
const LOVED_POINTS: u16 = 80;
const LIKED_POINTS: u16 = 40;
const NEUTRAL_POINTS: u16 = 15;
const DISLIKED_POINTS: u16 = 30;
/// How long a villager turns to whoever talked to them, in ticks.
const ATTENTION_TICKS: u32 = 150;

pub(crate) struct FriendshipPlugin;

impl Plugin for FriendshipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (talk, give_gifts, send_friendships)
                .chain()
                .after(MessageSystems::Receive),
        );
    }
}

/// A character's friendships, by villager.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Bonds(pub HashMap<VillagerId, Bond>);

/// How a player stands with one villager.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Bond {
    pub points: u16,
    pub talked_on: Option<u32>,
    pub gave_on: Option<u32>,
    /// Hearts at which the villager's present was already given.
    pub received: Vec<u16>,
}

impl Bonds {
    pub(crate) fn from_saved(saved: &[FriendshipState]) -> Self {
        Self(
            saved
                .iter()
                .map(|state| {
                    (
                        state.villager,
                        Bond {
                            points: state.points,
                            talked_on: state.talked_on,
                            gave_on: state.gave_on,
                            received: state.received.clone(),
                        },
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn to_saved(&self) -> Vec<FriendshipState> {
        let mut states: Vec<FriendshipState> = self
            .0
            .iter()
            .map(|(&villager, bond)| FriendshipState {
                villager,
                points: bond.points,
                talked_on: bond.talked_on,
                gave_on: bond.gave_on,
                received: bond.received.clone(),
            })
            .collect();
        states.sort_by_key(|state| state.villager);
        states
    }

    /// Hearts of friendship with `villager`.
    pub(crate) fn hearts(&self, villager: VillagerId) -> u16 {
        self.0.get(&villager).map_or(0, |bond| hearts(bond.points))
    }

    fn as_sent(&self, today: u32) -> Friendships {
        let mut all: Vec<Friendship> = self
            .0
            .iter()
            .map(|(&villager, bond)| Friendship {
                villager,
                points: bond.points,
                talked_today: bond.talked_on == Some(today),
                gave_today: bond.gave_on == Some(today),
            })
            .collect();
        all.sort_by_key(|friendship| friendship.villager);
        Friendships(all)
    }
}

/// The villager `id` names, where it stands, if the character at `feet`
/// is near enough to talk to it.
fn villager_near(
    villagers: &Query<(Entity, &Villager, &Position)>,
    id: VillagerId,
    feet: Vec3,
) -> Option<(Entity, Vec3)> {
    villagers
        .iter()
        .find(|(_, villager, _)| villager.0 == id)
        .filter(|(_, _, at)| tools::in_reach(feet + Vec3::Y * EYE_HEIGHT, at.0 + Vec3::Y))
        .map(|(entity, _, at)| (entity, at.0))
}

/// The present a villager gives for newly reached hearts, if any, put in
/// the character's hands.
fn present(
    content: &Content,
    villager: VillagerId,
    bond: &mut Bond,
    belongings: &mut Belongings,
    today: u32,
) -> Option<(messoria_content::ItemId, u16)> {
    let reached = hearts(bond.points);
    let definition = content.villager(villager);
    let gift = definition
        .gifts
        .iter()
        .find(|gift| gift.at <= reached && !bond.received.contains(&gift.at))?;
    let fits = belongings.0.room_for(content, gift.item, Quality::Normal) >= u32::from(gift.count);
    if !fits {
        return None;
    }
    belongings
        .0
        .add(content, gift.item, Quality::Normal, gift.count, today);
    bond.received.push(gift.at);
    Some((gift.item, gift.count))
}

fn talk(
    content: Res<Content>,
    clock: Single<(&WorldClock, &CurrentWeather)>,
    mut clients: Query<(
        &mut MessageReceiver<TalkTo>,
        &mut MessageSender<Dialogue>,
        &ControlledCharacter,
    )>,
    mut characters: Query<(&Position, &mut Bonds, &mut Belongings), Without<Asleep>>,
    villagers: Query<(Entity, &Villager, &Position)>,
    mut commands: Commands,
) {
    let (clock, weather) = *clock;
    let today = clock.0.day();
    for (mut requests, mut dialogues, character) in &mut clients {
        for TalkTo { villager } in requests.receive() {
            let Ok((feet, mut bonds, mut belongings)) = characters.get_mut(character.0) else {
                continue;
            };
            let Some((entity, at)) = villager_near(&villagers, villager, feet.0) else {
                continue;
            };
            let bond = bonds.0.entry(villager).or_default();
            if bond.talked_on != Some(today) {
                bond.talked_on = Some(today);
                bond.points = bond.points.saturating_add(TALK_POINTS);
            }
            let definition = content.villager(villager);
            // Spread over the villagers, so two talked to in a day do not
            // pick alike.
            let pick = usize::try_from(today).unwrap_or(0) + usize::from(villager.index());
            let says = definition
                .line(
                    clock.0,
                    weather.0.waters_fields(),
                    hearts(bond.points),
                    pick,
                )
                .to_owned();
            let given = present(&content, villager, bond, &mut belongings, today);
            dialogues.send::<FeedbackChannel>(Dialogue {
                villager,
                says,
                points: bond.points,
                reaction: None,
                given,
            });
            let toward = feet.0.xz() - at.xz();
            commands.entity(entity).insert(Attending {
                heading: (-toward.x).atan2(-toward.y),
                ticks: ATTENTION_TICKS,
            });
        }
    }
}

fn give_gifts(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut clients: Query<(
        &mut MessageReceiver<GiveGift>,
        &mut MessageSender<Dialogue>,
        &ControlledCharacter,
    )>,
    mut characters: Query<(&Position, &mut Bonds, &mut Belongings), Without<Asleep>>,
    villagers: Query<(Entity, &Villager, &Position)>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut commands: Commands,
) {
    let today = clock.0.day();
    for (mut requests, mut dialogues, character) in &mut clients {
        for GiveGift { villager, slot } in requests.receive() {
            let Ok((feet, mut bonds, mut belongings)) = characters.get_mut(character.0) else {
                continue;
            };
            let Some((entity, at)) = villager_near(&villagers, villager, feet.0) else {
                continue;
            };
            let refuse = |notice| Tell {
                character: character.0,
                notice,
            };
            let Some(stack) = belongings.0.slot(usize::from(slot)) else {
                tell.write(refuse(Notice::NothingToGive));
                continue;
            };
            let item = stack.item;
            let bond = bonds.0.entry(villager).or_default();
            if bond.gave_on == Some(today) {
                tell.write(refuse(Notice::GaveAlready));
                continue;
            }
            belongings.0.take_one(usize::from(slot));
            bond.gave_on = Some(today);
            let definition = content.villager(villager);
            let reaction = definition.reaction(item);
            bond.points = match reaction {
                Reaction::Loved => bond.points.saturating_add(LOVED_POINTS),
                Reaction::Liked => bond.points.saturating_add(LIKED_POINTS),
                Reaction::Neutral => bond.points.saturating_add(NEUTRAL_POINTS),
                Reaction::Disliked => bond.points.saturating_sub(DISLIKED_POINTS),
            };
            let says = definition.gift_line(reaction).to_owned();
            let given = present(&content, villager, bond, &mut belongings, today);
            dialogues.send::<FeedbackChannel>(Dialogue {
                villager,
                says,
                points: bond.points,
                reaction: Some(reaction),
                given,
            });
            show.write(Show::at(Happened::Gave, at, character.0));
            let toward = feet.0.xz() - at.xz();
            commands.entity(entity).insert(Attending {
                heading: (-toward.x).atan2(-toward.y),
                ticks: ATTENTION_TICKS,
            });
        }
    }
}

/// Sends each player their friendships when they arrive and whenever they
/// change.
fn send_friendships(
    clock: Single<&WorldClock>,
    characters: Query<(&Bonds, &ControlledBy), Changed<Bonds>>,
    mut senders: Query<&mut MessageSender<Friendships>>,
) {
    let today = clock.0.day();
    for (bonds, owner) in &characters {
        if let Ok(mut sender) = senders.get_mut(owner.owner) {
            sender.send::<FeedbackChannel>(bonds.as_sent(today));
        }
    }
}
