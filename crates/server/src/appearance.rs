//! How players look: the look each chooses on arrival, the clothes they
//! change into at the wardrobe in their cabin and the hair the barber
//! gives them.
//!
//! A newcomer is told so and asked to introduce themselves; until they do,
//! everyone sees them in a look picked from their id, and the save keeps
//! no look for them, so they are asked again next time. Clothes are
//! garments, items the tailor sells, worn at a wardrobe on a lot the
//! player may work; what comes off goes back into the backpack. The barber
//! changes hair and its color for the village's price, at the counter and
//! in opening hours.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::{ItemKind, Look, Purpose, Quality, Wear};
use messoria_economy::Refusal;
use messoria_inventory::Inventory;
use messoria_shared::{
    content::Content,
    lots::Claims,
    movement::EYE_HEIGHT,
    players::{player_key, proper_name},
    protocol::{
        Appearance, Asleep, Belongings, ChangeClothes, FeedbackChannel, Happened, Introduce, Money,
        Newcomer, Notice, PlayerId, PlayerName, Position, Restyle, Shopfront, WorldClock,
    },
    shops, tools,
};

use crate::{
    building::Built,
    feedback::{Show, Tell},
    players::ControlledCharacter,
};

pub(crate) struct AppearancePlugin;

impl Plugin for AppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (greet_newcomers, introduce, change_clothes, restyle).after(MessageSystems::Receive),
        );
    }
}

/// A character whose player has not introduced themselves yet.
#[derive(Component)]
pub(crate) struct Unintroduced;

/// A newcomer already asked to introduce themselves.
#[derive(Component)]
struct Asked;

/// Whether `look` can be worn here: every skin of it is in the content, as
/// a wardrobe choice or as a garment.
pub(crate) fn wearable(content: &Content, look: &Look) -> bool {
    let characters = content.characters();
    let wardrobe = &characters.wardrobe;
    let garment = |skin: &str, wear: Wear| {
        content.items().any(|(_, item)| {
            matches!(&item.kind, ItemKind::Garment { wear: worn, skin: own } if *worn == wear && own == skin)
        })
    };
    wardrobe.bodies.contains(&look.body)
        && wardrobe.eye_colors.contains(&look.eyes)
        && characters.styles(&look.hair, look.hair_color)
        && (wardrobe.outfits.contains(&look.outfit) || garment(&look.outfit, Wear::Outfit))
        && look
            .hat
            .as_deref()
            .is_none_or(|hat| garment(hat, Wear::Hat))
}

/// Tells each newcomer's client so, once its connection can be written to.
fn greet_newcomers(
    newcomers: Query<(Entity, &ControlledBy), (With<Unintroduced>, Without<Asked>)>,
    mut senders: Query<&mut MessageSender<Newcomer>>,
    mut commands: Commands,
) {
    for (character, owner) in &newcomers {
        if let Ok(mut sender) = senders.get_mut(owner.owner) {
            sender.send::<FeedbackChannel>(Newcomer);
            commands.entity(character).insert(Asked);
        }
    }
}

fn introduce(
    content: Res<Content>,
    mut clients: Query<(&mut MessageReceiver<Introduce>, &ControlledCharacter)>,
    mut characters: Query<(&mut Appearance, &mut PlayerName), With<Unintroduced>>,
    mut commands: Commands,
) {
    for (mut requests, character) in &mut clients {
        for Introduce { name, look } in requests.receive() {
            let Ok((mut appearance, mut player_name)) = characters.get_mut(character.0) else {
                continue;
            };
            let Some(name) = proper_name(&name) else {
                debug!("rejected an introduction: the name {name:?} will not do");
                continue;
            };
            if !content.characters().offers(&look) {
                debug!("rejected an introduction: the wardrobe does not offer {look:?}");
                continue;
            }
            appearance.0 = look;
            player_name.0 = name;
            commands
                .entity(character.0)
                .remove::<(Unintroduced, Asked)>();
        }
    }
}

fn change_clothes(
    content: Res<Content>,
    built: Built,
    claims: Claims,
    mut clients: Query<(&mut MessageReceiver<ChangeClothes>, &ControlledCharacter)>,
    mut characters: Query<
        (&Position, &PlayerId, &mut Belongings, &mut Appearance),
        Without<Asleep>,
    >,
    mut tell: MessageWriter<Tell>,
) {
    for (mut requests, character) in &mut clients {
        for ChangeClothes { slot } in requests.receive() {
            let Ok((feet, player, mut belongings, mut appearance)) =
                characters.get_mut(character.0)
            else {
                continue;
            };
            let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
            let Some((_, wardrobe)) = built
                .nearest(Purpose::Wardrobe, feet.0, tools::REACH)
                .filter(|(_, wardrobe)| tools::in_reach(eyes, wardrobe.position))
            else {
                debug!("rejected a change of clothes: no wardrobe in reach");
                continue;
            };
            let allowed =
                player_key(player.0).is_some_and(|key| claims.allows(&key, wardrobe.position.xz()));
            if !allowed {
                tell.write(Tell {
                    character: character.0,
                    notice: Notice::NotYourLot,
                });
                continue;
            }
            // Copies, so that a refused change changes nothing.
            let (mut carried, mut look) = (belongings.0.clone(), appearance.0.clone());
            let changed = match slot {
                Some(slot) => put_on(&content, &mut carried, &mut look, usize::from(slot)),
                None => take_off_hat(&content, &mut carried, &mut look),
            };
            match changed {
                Ok(()) => {
                    belongings.0 = carried;
                    appearance.set_if_neq(Appearance(look));
                }
                Err(notice) => {
                    tell.write(Tell {
                        character: character.0,
                        notice,
                    });
                }
            }
        }
    }
}

/// Wears the garment in `slot`; what it replaces goes into the backpack,
/// which has the room the garment just left.
fn put_on(
    content: &Content,
    carried: &mut Inventory,
    look: &mut Look,
    slot: usize,
) -> Result<(), Notice> {
    let item = carried
        .slot(slot)
        .map(|stack| stack.item)
        .ok_or(Notice::NothingToWear)?;
    let ItemKind::Garment { wear, skin } = &content.item(item).kind else {
        return Err(Notice::NothingToWear);
    };
    carried.take_one(slot);
    let taken_off = match wear {
        Wear::Outfit => Some(std::mem::replace(&mut look.outfit, skin.clone())),
        Wear::Hat => look.hat.replace(skin.clone()),
    };
    if let Some(garment) = taken_off.and_then(|skin| content.garment_with(&skin)) {
        carried.add(content, garment, Quality::Normal, 1, 0);
    }
    Ok(())
}

fn take_off_hat(content: &Content, carried: &mut Inventory, look: &mut Look) -> Result<(), Notice> {
    let hat = look.hat.take().ok_or(Notice::NothingToWear)?;
    if let Some(garment) = content.garment_with(&hat) {
        if carried.room_for(content, garment, Quality::Normal) == 0 {
            look.hat = Some(hat);
            return Err(Notice::NoRoom);
        }
        carried.add(content, garment, Quality::Normal, 1, 0);
    }
    Ok(())
}

fn restyle(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    stalls: Query<&Shopfront>,
    mut clients: Query<(&mut MessageReceiver<Restyle>, &ControlledCharacter)>,
    mut characters: Query<(&Position, &mut Money, &mut Appearance), Without<Asleep>>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
) {
    let haircut = content.village().haircut;
    for (mut requests, character) in &mut clients {
        for Restyle { hair, hair_color } in requests.receive() {
            let Ok((feet, mut money, mut appearance)) = characters.get_mut(character.0) else {
                continue;
            };
            let Some(stall) = stalls
                .iter()
                .find(|stall| stall.shop == haircut.shop && shops::can_reach(feet.0, stall))
            else {
                debug!("rejected a new hairstyle: not at the barber's");
                continue;
            };
            let refuse = |notice| Tell {
                character: character.0,
                notice,
            };
            if !content.shop(haircut.shop).is_open(clock.0) {
                tell.write(refuse(Notice::Trade(Refusal::Closed)));
                continue;
            }
            if !content.characters().styles(&hair, hair_color) {
                debug!("rejected a new hairstyle: the barber does not do {hair:?}");
                continue;
            }
            if appearance.0.hair == hair && appearance.0.hair_color == hair_color {
                continue;
            }
            if money.0.pay(haircut.price).is_err() {
                tell.write(refuse(Notice::HaircutCosts(haircut.price)));
                continue;
            }
            appearance.0.hair = hair;
            appearance.0.hair_color = hair_color;
            show.write(Show::at(Happened::Traded, stall.position, character.0));
        }
    }
}
