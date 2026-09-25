//! Village shops on screen: their stalls, and the window for trading at one.
//!
//! `F` next to a stall opens its shop; `F` again, Escape or walking away
//! closes it. The window lists what the shop buys among what the player
//! carries, and what it sells. Every price and every refusal is worked out
//! with the same trade rules the server applies, on copies of the player's
//! state and the replicated market, so a button offers exactly what the
//! trade will pay.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, ItemId, Quality, ShopDef, ShopId};
use messoria_economy::{Customer, Market, PriceBasis, Refusal, SalesLedger, Wallet, buy, sell};
use messoria_inventory::Inventory;
use messoria_shared::{
    content::Content,
    protocol::{
        ActionChannel, Belongings, Deal, MarketState, Money, PlayerInput, Position, Shopfront,
        SoldToday, Trade,
    },
    shops,
};

use crate::{
    actions::INTERACT_KEY,
    art::Models,
    avatars::{Keeper, dress},
    camera::View,
    clock::LocalClock,
    figures::FigureMeshes,
    icons::ItemIcons,
    panels::OpenPanel,
    skins::Tailor,
    ui::{self, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

/// Units bought at once with the second buy button.
const BULK_PURCHASE: u16 = 10;

/// Where wares lie on a counter: spread across it, its height, and toward
/// the customers.
const WARE_SPOTS: [f32; 3] = [-0.8, 0.0, 0.8];
const COUNTER_HEIGHT: f32 = 0.95;
const COUNTER_FRONT: f32 = -0.45;
/// Size of the item icons in the listing.
const ROW_ICON_SIZE: f32 = 28.0;
/// Where the keeper stands: to one side of the middle post, behind the counter.
const KEEPER_SIDE: f32 = 0.55;
const KEEPER_BEHIND: f32 = 0.55;

pub(crate) struct ShopsPlugin;

impl Plugin for ShopsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(build_stall)
            .add_systems(Startup, spawn_shop_window)
            .add_systems(Update, open_or_close_shop.in_set(ShopSystems))
            .add_systems(Update, (show_shop_window, press_deals).chain());
    }
}

/// Opens and closes shops; systems that also answer `F` run after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ShopSystems;

#[derive(Component)]
struct ShopWindow;

/// The part of the shop window rebuilt whenever what it shows changes.
#[derive(Component)]
struct ShopListing;

/// A button that makes a deal with the open shop.
#[derive(Component, Clone, Copy)]
struct DealButton(Deal);

/// A stall: the shop's stall model with some of its wares on the counter,
/// and its keeper behind it. Built facing -Z, then turned the way the stall
/// faces.
fn build_stall(
    trigger: On<Add, Shopfront>,
    content: Res<Content>,
    models: Res<Models>,
    meshes: Res<FigureMeshes>,
    mut tailor: Tailor,
    stalls: Query<&Shopfront>,
    mut commands: Commands,
) {
    let Ok(stall) = stalls.get(trigger.entity) else {
        return;
    };
    let shop = content.shop(stall.shop);
    let wares: Vec<_> = shop
        .buys
        .iter()
        .map(|offer| offer.item)
        .chain(shop.sells.iter().map(|listing| listing.item))
        .filter_map(|item| content.item(item).model.as_deref())
        .filter_map(|model| models.scene(model))
        .take(WARE_SPOTS.len())
        .collect();
    commands
        .entity(trigger.entity)
        .insert((
            Transform::from_translation(stall.position)
                .with_rotation(Quat::from_rotation_y(stall.facing)),
            Visibility::default(),
        ))
        .with_children(|parts| {
            if let Some(scene) = models.scene(&shop.stall) {
                parts.spawn(scene);
            }
            for (ware, x) in wares.into_iter().zip(WARE_SPOTS) {
                parts.spawn((ware, Transform::from_xyz(x, COUNTER_HEIGHT, COUNTER_FRONT)));
            }
        });
    let keeper = commands
        .spawn((
            Name::new(format!("{} keeper", shop.name)),
            Keeper {
                stall: stall.position,
            },
            Transform::from_xyz(KEEPER_SIDE, 0.0, KEEPER_BEHIND),
            ChildOf(trigger.entity),
        ))
        .id();
    dress(
        &mut commands,
        &content,
        &meshes,
        &mut tailor,
        keeper,
        &shop.keeper,
    );
}

fn spawn_shop_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Shop window"),
            ShopWindow,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_child((
            ShopListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(6),
                min_width: px(560),
                ..default()
            }),
        ));
}

/// `F` at a stall opens its shop, and closes it again; walking away closes
/// it too. Consumes the key press, so it does not also harvest.
fn open_or_close_shop(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    view: Res<View>,
    mut panel: ResMut<OpenPanel>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
    stalls: Query<(Entity, &Shopfront)>,
) {
    let Ok(feet) = player.single() else {
        return;
    };
    if let OpenPanel::Shop(open) = *panel {
        let at_stall = stalls
            .get(open)
            .is_ok_and(|(_, stall)| shops::can_reach(feet.0, stall));
        if !at_stall || keys.clear_just_pressed(INTERACT_KEY) {
            *panel = OpenPanel::None;
        }
        return;
    }
    if !view.captured || !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    let nearest = stalls
        .iter()
        .filter(|(_, stall)| shops::can_reach(feet.0, stall))
        .min_by(|(_, a), (_, b)| {
            let distance = |stall: &Shopfront| feet.0.distance_squared(stall.position);
            distance(a).total_cmp(&distance(b))
        });
    if let Some((stall, _)) = nearest {
        *panel = OpenPanel::Shop(stall);
        keys.clear_just_pressed(INTERACT_KEY);
    }
}

/// Rebuilds the open shop's listing whenever anything it shows changes:
/// what the player carries, their money and sales, the market, or the time.
fn show_shop_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    icons: Res<ItemIcons>,
    clock: Res<LocalClock>,
    stalls: Query<&Shopfront>,
    player: Query<(Ref<Belongings>, Ref<Money>, Ref<SoldToday>), With<InputMarker<PlayerInput>>>,
    market: Query<Ref<MarketState>>,
    mut shown_at: Local<Option<WorldTime>>,
    mut window: Single<&mut Visibility, With<ShopWindow>>,
    listing: Single<Entity, With<ShopListing>>,
    mut commands: Commands,
) {
    let stall = match *panel {
        OpenPanel::Shop(stall) => stalls.get(stall).ok(),
        OpenPanel::None
        | OpenPanel::Backpack
        | OpenPanel::Chest(_)
        | OpenPanel::Menu
        | OpenPanel::Options => None,
    };
    window.set_if_neq(ui::visible_if(stall.is_some()));
    let (Some(stall), Ok((belongings, money, sold)), Ok(market), Some(now)) =
        (stall, player.single(), market.single(), clock.time())
    else {
        return;
    };
    let changed = panel.is_changed()
        || belongings.is_changed()
        || money.is_changed()
        || sold.is_changed()
        || market.is_changed()
        || *shown_at != Some(now);
    if !changed {
        return;
    }
    *shown_at = Some(now);

    let counter = Counter {
        content: &content,
        shop: stall.shop,
        now,
        inventory: &belongings.0,
        wallet: money.0,
        ledger: &sold.0,
        market: &market.0,
        icons: &icons,
    };
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| counter.show(listing));
}

fn press_deals(
    panel: Res<OpenPanel>,
    stalls: Query<&Shopfront>,
    buttons: Query<(&Interaction, &DealButton), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<Trade>, With<Client>>,
) {
    let OpenPanel::Shop(stall) = *panel else {
        return;
    };
    let (Ok(stall), Ok(mut sender)) = (stalls.get(stall), sender.single_mut()) else {
        return;
    };
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            sender.send::<ActionChannel>(Trade {
                shop: stall.shop,
                deal: button.0,
            });
        }
    }
}

/// A shop's counter as the local player sees it: everything needed to work
/// out what each deal would do.
struct Counter<'a> {
    content: &'a Catalog,
    shop: ShopId,
    now: WorldTime,
    inventory: &'a Inventory,
    wallet: Wallet,
    ledger: &'a SalesLedger,
    market: &'a Market,
    icons: &'a ItemIcons,
}

impl Counter<'_> {
    fn definition(&self) -> &ShopDef {
        self.content.shop(self.shop)
    }

    /// Units sold and coins paid if the player sold up to `count` from
    /// `slot` now.
    fn try_sell(&self, slot: usize, count: u16) -> Result<(u16, u32), Refusal> {
        let (mut inventory, mut wallet, mut ledger) =
            (self.inventory.clone(), self.wallet, self.ledger.clone());
        let customer = Customer {
            inventory: &mut inventory,
            wallet: &mut wallet,
            ledger: &mut ledger,
        };
        let mut market = self.market.clone();
        sell(
            self.content,
            &mut market,
            self.shop,
            self.now,
            customer,
            slot,
            count,
        )
    }

    /// What buying `count` of `item` now would cost.
    fn try_buy(&self, item: ItemId, count: u16) -> Result<u32, Refusal> {
        let (mut inventory, mut wallet, mut ledger) =
            (self.inventory.clone(), self.wallet, self.ledger.clone());
        let customer = Customer {
            inventory: &mut inventory,
            wallet: &mut wallet,
            ledger: &mut ledger,
        };
        buy(self.content, self.shop, self.now, customer, item, count)
    }

    fn show(&self, listing: &mut ChildSpawnerCommands) {
        let shop = self.definition();
        listing.spawn(ui::label(shop.name.clone(), HEADING_SIZE, TEXT_COLOR));
        listing.spawn(ui::label(self.hours(), TEXT_SIZE, MUTED_TEXT_COLOR));
        listing.spawn(ui::label(
            format!("You have {} coins.", self.wallet.coins()),
            TEXT_SIZE,
            TEXT_COLOR,
        ));

        listing.spawn(ui::label("Sell", HEADING_SIZE, TEXT_COLOR));
        let mut selling = false;
        for (slot, stack) in self.inventory.slots().iter().enumerate() {
            let Some(stack) = stack else {
                continue;
            };
            let Some(offer) = shop.offer(stack.item) else {
                continue;
            };
            selling = true;
            let basis = PriceBasis::new(self.content, offer, stack.quality, self.now.season());
            let remaining = self
                .ledger
                .remaining(self.shop, stack.item, shop.daily_limit);
            let description = format!(
                "{}{}, {} carried: {} each, buys {remaining} more today",
                self.content.item(stack.item).name,
                quality_note(stack.quality),
                stack.count,
                self.market.unit_price(basis, self.content.market()),
            );
            let slot_index = u8::try_from(slot).expect("slot indices fit in u8");
            let deals = [1, stack.count].map(|count| {
                self.try_sell(slot, count).map(|(sold, paid)| {
                    (
                        format!("Sell {sold} for {paid}"),
                        Deal::Sell {
                            slot: slot_index,
                            count,
                        },
                    )
                })
            });
            spawn_row(listing, self.icons.get(stack.item), description, deals);
        }
        if !selling {
            listing.spawn(ui::label(
                format!("You carry nothing the {} buys.", shop.name.to_lowercase()),
                TEXT_SIZE,
                MUTED_TEXT_COLOR,
            ));
        }

        listing.spawn(ui::label("Buy", HEADING_SIZE, TEXT_COLOR));
        let season = self.now.season();
        for listed in shop.sells.iter().filter(|listed| listed.on_sale_in(season)) {
            let description = format!(
                "{}: {} each",
                self.content.item(listed.item).name,
                listed.price
            );
            let deals = [1, BULK_PURCHASE].map(|count| {
                self.try_buy(listed.item, count).map(|cost| {
                    (
                        format!("Buy {count} for {cost}"),
                        Deal::Buy {
                            item: listed.item,
                            count,
                        },
                    )
                })
            });
            spawn_row(listing, self.icons.get(listed.item), description, deals);
        }
    }

    /// Whether the shop is open, and when it trades.
    fn hours(&self) -> String {
        let shop = self.definition();
        let status = if shop.is_open(self.now) {
            "Open now"
        } else {
            "Closed now"
        };
        let closed_on: Vec<_> = shop
            .closed_on
            .iter()
            .map(|weekday| weekday.short_name())
            .collect();
        let days = if closed_on.is_empty() {
            String::new()
        } else {
            format!(", closed on {}", closed_on.join(", "))
        };
        format!(
            "{status}. Trades from {} to {}{days}.",
            shop.opens, shop.closes
        )
    }
}

/// One line of the listing: the item's icon and a description, then a
/// button for each deal that can be made, or why the first cannot.
fn spawn_row(
    listing: &mut ChildSpawnerCommands,
    icon: Option<Handle<Image>>,
    description: String,
    deals: [Result<(String, Deal), Refusal>; 2],
) {
    listing
        .spawn(Node {
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            if let Some(icon) = icon {
                row.spawn((
                    ImageNode::new(icon),
                    Node {
                        width: px(ROW_ICON_SIZE),
                        height: px(ROW_ICON_SIZE),
                        ..default()
                    },
                ));
            }
            row.spawn((
                ui::label(description, TEXT_SIZE, TEXT_COLOR),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            let [first, second] = deals;
            match first {
                Ok((first_text, first_deal)) => {
                    // A stack of one would offer the same sale twice.
                    let second = second.ok().filter(|(text, _)| *text != first_text);
                    ui::spawn_button(row, first_text, DealButton(first_deal));
                    if let Some((text, deal)) = second {
                        ui::spawn_button(row, text, DealButton(deal));
                    }
                }
                Err(refusal) => {
                    row.spawn(ui::label(sentence(refusal), TEXT_SIZE, MUTED_TEXT_COLOR));
                }
            }
        });
}

fn quality_note(quality: Quality) -> &'static str {
    match quality {
        Quality::Normal => "",
        Quality::Silver => " (silver)",
        Quality::Gold => " (gold)",
    }
}

/// A refusal as a sentence.
fn sentence(refusal: Refusal) -> String {
    let text = refusal.to_string();
    let mut letters = text.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}
