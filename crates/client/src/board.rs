//! The village notice board: the window for picking a farm lot and for
//! letting other players work one's own.
//!
//! `F` at the board opens it; `F` again, Escape or walking away closes it.
//! The window lists every lot, free or whose it is, with a button to claim
//! a free one (the first for nothing, the next for the village's price),
//! and, for each lot of the player's own, who may work it and who is
//! around to invite. The server decides every claim and invitation.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_shared::{
    content::Content,
    lots::{Claims, LOT_COUNT, LotId},
    movement::EYE_HEIGHT,
    players::{player_key, player_name},
    protocol::{
        ActionChannel, ClaimLot, InviteToLot, LotClaim, Money, PlayerId, PlayerInput, Position,
        Structure, Uninvite,
    },
    tools,
};

use crate::{
    actions::INTERACT_KEY,
    names::Names,
    panels::OpenPanel,
    ui::{self, ACCENT_COLOR, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

/// Lots listed in each column of the window.
const LOTS_PER_COLUMN: u16 = LOT_COUNT.div_ceil(2);
const NUMBER_WIDTH: f32 = 60.0;
const STATUS_WIDTH: f32 = 130.0;

pub(crate) struct BoardPlugin;

impl Plugin for BoardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_board_window)
            .add_systems(Update, (close_far_board, show_board_window, press_buttons));
    }
}

#[derive(Component)]
struct BoardWindow;

/// The part of the window rebuilt whenever what it shows changes.
#[derive(Component)]
struct BoardListing;

#[derive(Component, Clone)]
enum BoardButton {
    Claim(LotId),
    Invite(LotId, PeerId),
    Uninvite(LotId, String),
}

fn spawn_board_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Notice board window"),
            BoardWindow,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_child((
            BoardListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(8),
                min_width: px(620),
                ..default()
            }),
        ));
}

/// Walking away from the board, or pressing the interact key again,
/// closes its window.
fn close_far_board(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut panel: ResMut<OpenPanel>,
    boards: Query<&Structure>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
) {
    let OpenPanel::Board(board) = *panel else {
        return;
    };
    let within_reach = player
        .single()
        .ok()
        .zip(boards.get(board).ok())
        .is_some_and(|(feet, board)| {
            tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, board.position)
        });
    if !within_reach || keys.clear_just_pressed(INTERACT_KEY) {
        *panel = OpenPanel::None;
    }
}

/// What the window shows: every lot's standing, the player's own, and who
/// is around to invite.
fn show_board_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    claims: Claims,
    changed_claims: Query<(), Changed<LotClaim>>,
    mut removed_claims: RemovedComponents<LotClaim>,
    player: Query<(&PlayerId, Ref<Money>), With<InputMarker<PlayerInput>>>,
    others: Query<&PlayerId, Without<InputMarker<PlayerInput>>>,
    names: Names,
    mut shown_players: Local<Vec<PeerId>>,
    mut window: Single<&mut Visibility, With<BoardWindow>>,
    listing: Single<Entity, With<BoardListing>>,
    mut commands: Commands,
) {
    let open = matches!(*panel, OpenPanel::Board(_));
    window.set_if_neq(ui::visible_if(open));
    let Ok((player, money)) = player.single() else {
        return;
    };
    let mut players: Vec<PeerId> = others.iter().map(|id| id.0).collect();
    players.sort_unstable_by_key(PeerId::to_bits);
    let changed = panel.is_changed()
        || money.is_changed()
        || !changed_claims.is_empty()
        || removed_claims.read().count() > 0
        || *shown_players != players;
    if !open || !changed {
        return;
    }
    *shown_players = players;
    let Some(key) = player_key(player.0) else {
        return;
    };

    let board = Board {
        content: &content,
        claims: &claims,
        key: &key,
        coins: money.0.coins(),
        others: &shown_players,
        names: &names,
    };
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| board.show(listing));
}

fn press_buttons(
    panel: Res<OpenPanel>,
    buttons: Query<(&Interaction, &BoardButton), Changed<Interaction>>,
    mut claims: Query<&mut MessageSender<ClaimLot>, With<Client>>,
    mut invites: Query<&mut MessageSender<InviteToLot>, With<Client>>,
    mut uninvites: Query<&mut MessageSender<Uninvite>, With<Client>>,
) {
    if !matches!(*panel, OpenPanel::Board(_)) {
        return;
    }
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.clone() {
            BoardButton::Claim(lot) => {
                if let Ok(mut sender) = claims.single_mut() {
                    sender.send::<ActionChannel>(ClaimLot { lot });
                }
            }
            BoardButton::Invite(lot, player) => {
                if let Ok(mut sender) = invites.single_mut() {
                    sender.send::<ActionChannel>(InviteToLot { lot, player });
                }
            }
            BoardButton::Uninvite(lot, player) => {
                if let Ok(mut sender) = uninvites.single_mut() {
                    sender.send::<ActionChannel>(Uninvite { lot, player });
                }
            }
        }
    }
}

/// The board as the local player reads it.
struct Board<'a> {
    content: &'a Content,
    claims: &'a Claims<'a, 'a>,
    /// The local player's key.
    key: &'a str,
    coins: u32,
    /// The other players around.
    others: &'a [PeerId],
    names: &'a Names<'a, 'a>,
}

impl Board<'_> {
    fn show(&self, listing: &mut ChildSpawnerCommands) {
        let price = self.content.village().lots.price;
        let owned = self.claims.owned_by(self.key);
        listing.spawn(ui::label("Farm lots", HEADING_SIZE, TEXT_COLOR));
        let terms = if owned == 0 {
            "Your first lot is free. Pick one and farm it.".to_owned()
        } else {
            format!(
                "Another lot costs {price} coins. You have {} coins.",
                self.coins
            )
        };
        listing.spawn(ui::label(terms, TEXT_SIZE, MUTED_TEXT_COLOR));

        listing
            .spawn(Node {
                column_gap: px(24),
                ..default()
            })
            .with_children(|columns| {
                for half in 0..2 {
                    columns
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(4),
                            ..default()
                        })
                        .with_children(|column| {
                            let first = half * LOTS_PER_COLUMN;
                            for lot in (first..(first + LOTS_PER_COLUMN).min(LOT_COUNT)).map(LotId)
                            {
                                self.show_lot(column, lot, owned == 0, price);
                            }
                        });
                }
            });

        let own: Vec<&LotClaim> = self
            .claims
            .iter()
            .filter(|claim| claim.owner == self.key)
            .collect();
        if own.is_empty() {
            return;
        }
        listing.spawn(ui::label(
            "Who may work your lots",
            HEADING_SIZE,
            TEXT_COLOR,
        ));
        for claim in own {
            self.show_guests(listing, claim);
        }
    }

    /// One line for `lot`: its number, whose it is, and a button to claim it
    /// if it is free.
    fn show_lot(&self, column: &mut ChildSpawnerCommands, lot: LotId, free: bool, price: u32) {
        let claim = self.claims.on(lot);
        let (status, color) = match claim {
            None => ("Free".to_owned(), MUTED_TEXT_COLOR),
            Some(claim) if claim.owner == self.key => ("Yours".to_owned(), ACCENT_COLOR),
            Some(claim) if claim.allows(self.key) => (
                format!("{}, with you", player_name(&claim.owner)),
                TEXT_COLOR,
            ),
            Some(claim) => (player_name(&claim.owner), TEXT_COLOR),
        };
        column
            .spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                min_height: px(28),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    ui::label(format!("Lot {}", lot.number()), TEXT_SIZE, TEXT_COLOR),
                    Node {
                        width: px(NUMBER_WIDTH),
                        ..default()
                    },
                ));
                row.spawn((
                    ui::label(status, TEXT_SIZE, color),
                    Node {
                        width: px(STATUS_WIDTH),
                        ..default()
                    },
                ));
                if claim.is_none() {
                    let text = if free {
                        "Claim".to_owned()
                    } else {
                        format!("Buy for {price}")
                    };
                    if free || self.coins >= price {
                        ui::spawn_button(row, text, BoardButton::Claim(lot));
                    } else {
                        row.spawn(ui::label(text, TEXT_SIZE, MUTED_TEXT_COLOR));
                    }
                }
            });
    }

    /// Who may work one of the player's lots, with a button to take each
    /// invitation back, and who is around to invite.
    fn show_guests(&self, listing: &mut ChildSpawnerCommands, claim: &LotClaim) {
        listing
            .spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(4),
                ..default()
            })
            .with_children(|row| {
                row.spawn(ui::label(
                    format!("Lot {}:", claim.lot.number()),
                    TEXT_SIZE,
                    TEXT_COLOR,
                ));
                if claim.invited.is_empty() {
                    row.spawn(ui::label("only you", TEXT_SIZE, MUTED_TEXT_COLOR));
                }
                for guest in &claim.invited {
                    ui::spawn_button(
                        row,
                        format!("{} \u{2715}", player_name(guest)),
                        BoardButton::Uninvite(claim.lot, guest.clone()),
                    );
                }
                let to_invite: Vec<PeerId> = self
                    .others
                    .iter()
                    .copied()
                    .filter(|&peer| player_key(peer).is_some_and(|key| !claim.allows(&key)))
                    .collect();
                if to_invite.is_empty() {
                    row.spawn(ui::label(
                        "Nobody else is near to invite.",
                        TEXT_SIZE,
                        MUTED_TEXT_COLOR,
                    ));
                }
                for peer in to_invite {
                    ui::spawn_button(
                        row,
                        format!("Invite {}", self.names.of(peer)),
                        BoardButton::Invite(claim.lot, peer),
                    );
                }
            });
    }
}
