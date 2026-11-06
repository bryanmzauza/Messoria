//! Who owns the farm lots: claiming one at the village notice board, and
//! inviting other players to work it.
//!
//! A player's first lot is free; each one after costs the village's price.
//! Claims are made standing at the notice board, as trades are made at a
//! stall. Each claimed lot is a replicated `LotClaim` entity, which the
//! save keeps. What a claim allows is checked by the systems that work the
//! ground (`messoria_shared::lots::Claims`).

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::Purpose;
use messoria_shared::{
    content::Content,
    lots::{Claims, LOT_COUNT, LotId},
    movement::EYE_HEIGHT,
    players::player_key,
    protocol::{
        ClaimLot, Happened, InviteToLot, LotClaim, Money, Notice, PlayerId, Position, Uninvite,
    },
    tools,
};

use crate::{
    Beginning, WorldStart,
    building::Built,
    feedback::{Show, Tell},
    players::ControlledCharacter,
};

pub(crate) struct LotsPlugin;

impl Plugin for LotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, restore_claims).add_systems(
            PreUpdate,
            (claim_lots, invite_to_lots, uninvite).after(MessageSystems::Receive),
        );
    }
}

fn restore_claims(beginning: Res<Beginning>, mut commands: Commands) {
    let WorldStart::Resume(saved) = &beginning.0 else {
        return;
    };
    for lot in &saved.world.lots {
        claim(
            &mut commands,
            LotClaim {
                lot: lot.lot,
                owner: lot.owner.clone(),
                invited: lot.invited.clone(),
            },
        );
    }
}

fn claim(commands: &mut Commands, claim: LotClaim) {
    commands.spawn((
        Name::new(format!("Lot {}", claim.lot.number())),
        claim,
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Whether the character at `feet` stands at a notice board.
fn at_a_board(built: &Built, feet: Vec3) -> bool {
    built
        .nearest(Purpose::NoticeBoard, feet, tools::REACH)
        .is_some_and(|(_, board)| tools::in_reach(feet + Vec3::Y * EYE_HEIGHT, board.position))
}

fn claim_lots(
    content: Res<Content>,
    built: Built,
    claims: Claims,
    mut clients: Query<(&mut MessageReceiver<ClaimLot>, &ControlledCharacter)>,
    mut characters: Query<(&Position, &PlayerId, &mut Money)>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut commands: Commands,
) {
    // Claims made this frame are not entities yet, so two players asking
    // for one lot at once would both get it without this.
    let mut claimed_now: Vec<LotId> = Vec::new();
    for (mut requests, character) in &mut clients {
        // One claim per frame per player: the next sees this one.
        let Some(ClaimLot { lot }) = requests.receive().next() else {
            continue;
        };
        let Ok((feet, player, mut money)) = characters.get_mut(character.0) else {
            continue;
        };
        let Some(key) = player_key(player.0) else {
            continue;
        };
        if lot.0 >= LOT_COUNT || !at_a_board(&built, feet.0) {
            debug!("rejected a claim on lot {} from {key}", lot.0);
            continue;
        }
        let refuse = |notice| Tell {
            character: character.0,
            notice,
        };
        if claims.on(lot).is_some() || claimed_now.contains(&lot) {
            tell.write(refuse(Notice::LotTaken));
            continue;
        }
        let price = if claims.owned_by(&key) == 0 {
            0
        } else {
            content.village().lots.price
        };
        if money.0.pay(price).is_err() {
            tell.write(refuse(Notice::LotCosts(price)));
            continue;
        }
        claimed_now.push(lot);
        claim(
            &mut commands,
            LotClaim {
                lot,
                owner: key,
                invited: Vec::new(),
            },
        );
        show.write(Show::at(Happened::ClaimedLot, feet.0, character.0));
    }
}

/// The claim on `lot` owned by the player controlling `character`, if any,
/// with the owner's key.
fn own_claim<'a>(
    lot: LotId,
    character: Entity,
    characters: &Query<&PlayerId>,
    claims: &'a mut Query<&mut LotClaim>,
) -> Option<(Mut<'a, LotClaim>, String)> {
    let key = player_key(characters.get(character).ok()?.0)?;
    let claim = claims
        .iter_mut()
        .find(|claim| claim.lot == lot && claim.owner == key)?;
    Some((claim, key))
}

fn invite_to_lots(
    mut clients: Query<(&mut MessageReceiver<InviteToLot>, &ControlledCharacter)>,
    characters: Query<&PlayerId>,
    mut claims: Query<&mut LotClaim>,
    mut tell: MessageWriter<Tell>,
) {
    for (mut requests, character) in &mut clients {
        for InviteToLot { lot, player } in requests.receive() {
            let Some((mut claim, _)) = own_claim(lot, character.0, &characters, &mut claims) else {
                continue;
            };
            // Only players in the world are invited, so the invitation
            // cannot go to a key nobody has.
            let Some(guest) = characters
                .iter()
                .find(|player_id| player_id.0 == player)
                .and_then(|player_id| player_key(player_id.0))
            else {
                continue;
            };
            if claim.allows(&guest) {
                tell.write(Tell {
                    character: character.0,
                    notice: Notice::AlreadyInvited,
                });
                continue;
            }
            claim.invited.push(guest);
        }
    }
}

fn uninvite(
    mut clients: Query<(&mut MessageReceiver<Uninvite>, &ControlledCharacter)>,
    characters: Query<&PlayerId>,
    mut claims: Query<&mut LotClaim>,
) {
    for (mut requests, character) in &mut clients {
        for Uninvite { lot, player } in requests.receive() {
            let Some((mut claim, _)) = own_claim(lot, character.0, &characters, &mut claims) else {
                continue;
            };
            if claim.invited.contains(&player) {
                claim.invited.retain(|invited| *invited != player);
            }
        }
    }
}
