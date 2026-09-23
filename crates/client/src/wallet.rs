//! Giving money to other players, from a panel shown beside the backpack.
//!
//! The panel lists everyone else in the world, with a button to give each of
//! them the chosen amount.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_shared::protocol::{ActionChannel, GiveMoney, Money, PlayerId, PlayerInput};

use crate::{
    panels::OpenPanel,
    ui::{self, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, WINDOW_COLOR},
};

/// Amount chosen when the game starts, and the steps it changes by.
const STARTING_GIFT: u32 = 10;
const GIFT_STEPS: [i64; 4] = [-100, -10, 10, 100];

pub(crate) struct WalletPlugin;

impl Plugin for WalletPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GiftAmount(STARTING_GIFT))
            .add_systems(Startup, spawn_gift_panel)
            .add_systems(Update, (press_gift_buttons, show_gift_panel).chain());
    }
}

/// How much the give buttons hand over.
#[derive(Resource)]
struct GiftAmount(u32);

#[derive(Component)]
struct GiftPanel;

/// The part of the panel rebuilt whenever what it shows changes.
#[derive(Component)]
struct GiftListing;

#[derive(Component, Clone, Copy)]
enum GiftButton {
    Change(i64),
    Give(PeerId),
}

fn spawn_gift_panel(mut commands: Commands) {
    commands.spawn((
        Name::new("Gift panel"),
        GiftPanel,
        GiftListing,
        Node {
            position_type: PositionType::Absolute,
            left: px(24),
            top: px(120),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(12)),
            row_gap: px(6),
            min_width: px(240),
            ..default()
        },
        BackgroundColor(WINDOW_COLOR),
        Visibility::Hidden,
    ));
}

fn press_gift_buttons(
    buttons: Query<(&Interaction, &GiftButton), Changed<Interaction>>,
    player: Query<&Money, With<InputMarker<PlayerInput>>>,
    mut amount: ResMut<GiftAmount>,
    mut sender: Query<&mut MessageSender<GiveMoney>, With<Client>>,
) {
    let coins = player.single().map_or(0, |money| money.0.coins());
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            GiftButton::Change(step) => {
                let changed = (i64::from(amount.0) + step).clamp(1, i64::from(coins.max(1)));
                amount.0 = u32::try_from(changed).expect("clamped to the u32 range");
            }
            GiftButton::Give(to) => {
                if let Ok(mut sender) = sender.single_mut() {
                    sender.send::<ActionChannel>(GiveMoney {
                        to,
                        amount: amount.0,
                    });
                }
            }
        }
    }
}

fn show_gift_panel(
    panel: Res<OpenPanel>,
    amount: Res<GiftAmount>,
    player: Query<Ref<Money>, With<InputMarker<PlayerInput>>>,
    others: Query<&PlayerId, Without<InputMarker<PlayerInput>>>,
    mut shown_players: Local<Vec<PeerId>>,
    window: Single<(Entity, &mut Visibility), With<GiftPanel>>,
    mut commands: Commands,
) {
    let (listing, mut visibility) = window.into_inner();
    let open = *panel == OpenPanel::Backpack;
    visibility.set_if_neq(ui::visible_if(open));
    let Ok(money) = player.single() else {
        return;
    };
    let mut players: Vec<PeerId> = others.iter().map(|id| id.0).collect();
    players.sort_unstable_by_key(PeerId::to_bits);
    let changed = panel.is_changed()
        || amount.is_changed()
        || money.is_changed()
        || *shown_players != players;
    if !open || !changed {
        return;
    }
    *shown_players = players;

    let coins = money.0.coins();
    commands
        .entity(listing)
        .despawn_related::<Children>()
        .with_children(|listing| {
            listing.spawn(ui::label("Give money", HEADING_SIZE, TEXT_COLOR));
            listing.spawn(ui::label(
                format!("You have {coins} coins."),
                TEXT_SIZE,
                TEXT_COLOR,
            ));
            listing
                .spawn(Node {
                    column_gap: px(4),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(ui::label(
                        format!("Amount: {}", amount.0),
                        TEXT_SIZE,
                        TEXT_COLOR,
                    ));
                    for step in GIFT_STEPS {
                        ui::spawn_button(row, format!("{step:+}"), GiftButton::Change(step));
                    }
                });
            if shown_players.is_empty() {
                listing.spawn(ui::label(
                    "Nobody else is here.",
                    TEXT_SIZE,
                    MUTED_TEXT_COLOR,
                ));
            }
            for &peer in &*shown_players {
                listing
                    .spawn(Node {
                        column_gap: px(8),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            ui::label(player_name(peer), TEXT_SIZE, TEXT_COLOR),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        if amount.0 <= coins {
                            ui::spawn_button(
                                row,
                                format!("Give {}", amount.0),
                                GiftButton::Give(peer),
                            );
                        }
                    });
            }
        });
}

/// How a player is named on screen until players choose their own names.
fn player_name(peer: PeerId) -> String {
    format!("Player {:04X}", peer.to_bits() & 0xFFFF)
}
