//! Money changing hands: trading at shops, players giving each other money,
//! and the market recovering overnight.
//!
//! Trades are carried out on copies of a character's belongings, money and
//! sales, which replace the originals only when the trade happens, so a
//! refused trade changes nothing and sends nothing to clients.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_economy::{Customer, buy, sell};
use messoria_shared::{
    content::Content,
    protocol::{
        Asleep, Belongings, Deal, GiveMoney, Happened, MarketState, Money, Notice, PlayerId,
        Position, Shopfront, SoldToday, Trade, WorldClock,
    },
    shops,
};

use crate::{
    Beginning, WorldStart,
    day_cycle::{ClockSystems, DayStarted},
    feedback::{Show, Tell},
    players::ControlledCharacter,
};

pub(crate) struct MarketPlugin;

impl Plugin for MarketPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, open_market)
            .add_systems(
                PreUpdate,
                (trade, give_money).after(MessageSystems::Receive),
            )
            .add_systems(FixedUpdate, start_market_day.after(ClockSystems));
    }
}

fn open_market(beginning: Res<Beginning>, mut commands: Commands) {
    let market = match &beginning.0 {
        WorldStart::New { .. } => MarketState::default(),
        WorldStart::Resume(saved) => MarketState(saved.world.market.clone()),
    };
    commands.spawn((
        Name::new("Market"),
        market,
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn trade(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut market: Single<&mut MarketState>,
    stalls: Query<&Shopfront>,
    mut clients: Query<(&mut MessageReceiver<Trade>, &ControlledCharacter)>,
    mut characters: Query<
        (&Position, &mut Belongings, &mut Money, &mut SoldToday),
        Without<Asleep>,
    >,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
) {
    let now = clock.0;
    for (mut requests, character) in &mut clients {
        for Trade { shop, deal } in requests.receive() {
            let Ok((feet, mut belongings, mut money, mut sold)) = characters.get_mut(character.0)
            else {
                continue;
            };
            let Some(stall) = stalls
                .iter()
                .find(|stall| stall.shop == shop && shops::can_reach(feet.0, stall))
            else {
                debug!("rejected {deal:?}: not at the stall of {shop:?}");
                continue;
            };

            let (mut inventory, mut wallet, mut ledger) =
                (belongings.0.clone(), money.0, sold.0.clone());
            let customer = Customer {
                inventory: &mut inventory,
                wallet: &mut wallet,
                ledger: &mut ledger,
            };
            // The market only changes when a sale goes through.
            let outcome = match deal {
                Deal::Sell { slot, count } => sell(
                    &content,
                    &mut market.bypass_change_detection().0,
                    shop,
                    now,
                    customer,
                    usize::from(slot),
                    count,
                )
                .map(|_| market.set_changed()),
                Deal::Buy { item, count } => {
                    buy(&content, shop, now, customer, item, count).map(|_| ())
                }
            };
            match outcome {
                Ok(()) => {
                    belongings.0 = inventory;
                    money.0 = wallet;
                    sold.set_if_neq(SoldToday(ledger));
                    show.write(Show::at(Happened::Traded, stall.position));
                }
                Err(refusal) => {
                    tell.write(Tell {
                        character: character.0,
                        notice: Notice::Trade(refusal),
                    });
                }
            }
        }
    }
}

fn give_money(
    mut clients: Query<(&mut MessageReceiver<GiveMoney>, &ControlledCharacter)>,
    players: Query<(Entity, &PlayerId)>,
    mut wallets: Query<&mut Money>,
) {
    for (mut requests, giver) in &mut clients {
        for GiveMoney { to, amount } in requests.receive() {
            let Some((recipient, _)) = players.iter().find(|(_, id)| id.0 == to) else {
                continue;
            };
            if recipient == giver.0 || amount == 0 {
                continue;
            }
            let Ok([mut from, mut to]) = wallets.get_many_mut([giver.0, recipient]) else {
                continue;
            };
            let (mut from_wallet, mut to_wallet) = (from.0, to.0);
            match from_wallet.transfer(&mut to_wallet, amount) {
                Ok(()) => {
                    from.0 = from_wallet;
                    to.0 = to_wallet;
                }
                Err(error) => debug!("rejected a gift of {amount}: {error}"),
            }
        }
    }
}

/// At dawn the market recovers a little and everyone's daily sales start
/// over.
fn start_market_day(
    content: Res<Content>,
    mut days: MessageReader<DayStarted>,
    mut market: Single<&mut MarketState>,
    mut sales: Query<&mut SoldToday>,
) {
    for _ in days.read() {
        if !market.0.is_empty() {
            market.0.recover(content.market());
        }
        for mut sold in &mut sales {
            if !sold.0.is_empty() {
                sold.0.clear();
            }
        }
    }
}
