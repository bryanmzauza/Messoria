//! Trading at a shop's counter.
//!
//! Each trade either happens completely or changes nothing, and says why
//! when it cannot happen. The server carries trades out; clients use the
//! same functions on copies of their state to show what a shop would pay.

use messoria_calendar::WorldTime;
use messoria_content::{Catalog, ItemId, Quality, ShopId};
use messoria_inventory::Inventory;
use thiserror::Error;

use crate::{
    ledger::SalesLedger,
    market::{Market, PriceBasis},
    wallet::Wallet,
};

/// What a player brings to the counter.
pub struct Customer<'a> {
    pub inventory: &'a mut Inventory,
    pub wallet: &'a mut Wallet,
    pub ledger: &'a mut SalesLedger,
}

/// Why a trade cannot happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Refusal {
    #[error("the shop is closed")]
    Closed,
    #[error("there is nothing to trade")]
    Nothing,
    #[error("the shop does not buy that")]
    NotWanted,
    #[error("the shop will not buy more of that today")]
    LimitReached,
    #[error("the shop does not sell that")]
    NotStocked,
    #[error("that is not on sale this season")]
    OutOfSeason,
    #[error("not enough money")]
    CannotAfford,
    #[error("not enough room to carry it")]
    NoRoom,
    #[error("the wallet cannot hold that much")]
    WalletFull,
}

/// Sells up to `count` items from inventory `slot` to `shop` at `now`: as
/// many as the slot holds and the shop still buys from this customer today.
/// Returns how many sold and what they fetched.
///
/// # Errors
///
/// If the shop is closed, does not buy the item or has bought its daily
/// limit of it, or there is nothing to sell.
pub fn sell(
    catalog: &Catalog,
    market: &mut Market,
    shop: ShopId,
    now: WorldTime,
    customer: Customer<'_>,
    slot: usize,
    count: u16,
) -> Result<(u16, u32), Refusal> {
    let definition = catalog.shop(shop);
    if !definition.is_open(now) {
        return Err(Refusal::Closed);
    }
    let stack = *customer.inventory.slot(slot).ok_or(Refusal::Nothing)?;
    let offer = definition.offer(stack.item).ok_or(Refusal::NotWanted)?;
    let remaining = customer
        .ledger
        .remaining(shop, stack.item, definition.daily_limit);
    if remaining == 0 {
        return Err(Refusal::LimitReached);
    }
    let count = count.min(stack.count).min(remaining);
    if count == 0 {
        return Err(Refusal::Nothing);
    }

    let basis = PriceBasis::new(catalog, offer, stack.quality, now.season());
    let paid = market.quote(basis, count, catalog.market());
    customer
        .wallet
        .receive(paid)
        .map_err(|_| Refusal::WalletFull)?;
    market.sell(basis, count, catalog.market());
    customer.inventory.take(slot, count);
    customer.ledger.record(shop, stack.item, count);
    Ok((count, paid))
}

/// Buys `count` of `item` from `shop` at `now`, returning what it cost.
///
/// # Errors
///
/// If the shop is closed or does not sell the item this season, or the
/// customer cannot pay for or carry all of it.
pub fn buy(
    catalog: &Catalog,
    shop: ShopId,
    now: WorldTime,
    customer: Customer<'_>,
    item: ItemId,
    count: u16,
) -> Result<u32, Refusal> {
    let definition = catalog.shop(shop);
    if !definition.is_open(now) {
        return Err(Refusal::Closed);
    }
    let listing = definition.listing(item).ok_or(Refusal::NotStocked)?;
    if !listing.on_sale_in(now.season()) {
        return Err(Refusal::OutOfSeason);
    }
    if count == 0 {
        return Err(Refusal::Nothing);
    }
    if customer.inventory.room_for(catalog, item, Quality::Normal) < u32::from(count) {
        return Err(Refusal::NoRoom);
    }
    let cost = listing
        .price
        .checked_mul(u32::from(count))
        .ok_or(Refusal::CannotAfford)?;
    customer
        .wallet
        .pay(cost)
        .map_err(|_| Refusal::CannotAfford)?;
    customer
        .inventory
        .add(catalog, item, Quality::Normal, count, now.day());
    Ok(cost)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    struct Counter {
        catalog: Catalog,
        grocer: ShopId,
        market: Market,
        inventory: Inventory,
        wallet: Wallet,
        ledger: SalesLedger,
    }

    impl Counter {
        fn new(coins: u32) -> Self {
            let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
            let catalog = Catalog::load(&data_dir).expect("the shipped content is valid");
            let grocer = catalog.shop_id("grocer").expect("the grocer exists");
            Self {
                catalog,
                grocer,
                market: Market::default(),
                inventory: Inventory::default(),
                wallet: Wallet::with(coins),
                ledger: SalesLedger::default(),
            }
        }

        fn item(&self, key: &str) -> ItemId {
            self.catalog.id(key).expect("the item exists")
        }

        fn give(&mut self, key: &str, count: u16) {
            let item = self.item(key);
            self.inventory
                .add(&self.catalog, item, Quality::Normal, count, 0);
        }

        fn customer(&mut self) -> Customer<'_> {
            Customer {
                inventory: &mut self.inventory,
                wallet: &mut self.wallet,
                ledger: &mut self.ledger,
            }
        }

        fn sell(&mut self, now: WorldTime, slot: usize, count: u16) -> Result<(u16, u32), Refusal> {
            let (catalog, grocer) = (self.catalog.clone(), self.grocer);
            let mut market = std::mem::take(&mut self.market);
            let result = sell(
                &catalog,
                &mut market,
                grocer,
                now,
                self.customer(),
                slot,
                count,
            );
            self.market = market;
            result
        }

        fn buy(&mut self, now: WorldTime, key: &str, count: u16) -> Result<u32, Refusal> {
            let (catalog, grocer, item) = (self.catalog.clone(), self.grocer, self.item(key));
            buy(&catalog, grocer, now, self.customer(), item, count)
        }
    }

    /// Monday of the first week of spring, when the grocer is open.
    fn monday_noon() -> WorldTime {
        WorldTime::at(0, "12:00".parse().unwrap()).unwrap()
    }

    #[test]
    fn selling_pays_what_the_market_quotes() {
        let mut counter = Counter::new(0);
        counter.give("turnip", 10);
        let turnip = counter.item("turnip");

        // Each turnip sold lowers the next one's price: 35 + 35 + 35 + 34.
        assert_eq!(counter.sell(monday_noon(), 0, 4), Ok((4, 139)));
        assert_eq!(counter.wallet.coins(), 139);
        assert_eq!(counter.inventory.count(turnip), 6);
        assert_eq!(counter.ledger.sold(counter.grocer, turnip), 4);
        assert!((counter.market.saturation(turnip) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn a_player_sells_no_more_than_the_daily_limit() {
        let mut counter = Counter::new(0);
        counter.give("turnip", 99);
        let limit = counter.catalog.shop(counter.grocer).daily_limit;

        let (sold, _) = counter.sell(monday_noon(), 0, 99).unwrap();
        assert_eq!(sold, limit);
        assert_eq!(
            counter.sell(monday_noon(), 0, 1),
            Err(Refusal::LimitReached)
        );

        counter.ledger.clear();
        assert!(counter.sell(monday_noon(), 0, 1).is_ok());
    }

    #[test]
    fn trades_need_an_open_shop_that_deals_in_the_item() {
        let mut counter = Counter::new(100);
        counter.give("turnip", 5);
        counter.give("compost", 5);
        let evening = WorldTime::at(0, "19:00".parse().unwrap()).unwrap();
        let sunday = WorldTime::at(6, "12:00".parse().unwrap()).unwrap();

        assert_eq!(counter.sell(evening, 0, 1), Err(Refusal::Closed));
        assert_eq!(counter.sell(sunday, 0, 1), Err(Refusal::Closed));
        assert_eq!(counter.sell(monday_noon(), 1, 1), Err(Refusal::NotWanted));
        assert_eq!(counter.sell(monday_noon(), 7, 1), Err(Refusal::Nothing));
        assert_eq!(
            counter.buy(monday_noon(), "hoe", 1),
            Err(Refusal::NotStocked)
        );
        assert_eq!(counter.wallet.coins(), 100);
    }

    #[test]
    fn buying_takes_money_and_needs_the_season_money_and_room() {
        let mut counter = Counter::new(100);
        let seeds = counter.item("turnip_seeds");

        assert_eq!(counter.buy(monday_noon(), "turnip_seeds", 5), Ok(50));
        assert_eq!(counter.inventory.count(seeds), 5);
        assert_eq!(counter.wallet.coins(), 50);

        assert_eq!(
            counter.buy(monday_noon(), "turnip_seeds", 6),
            Err(Refusal::CannotAfford)
        );
        assert_eq!(
            counter.buy(monday_noon(), "tomato_seeds", 1),
            Err(Refusal::OutOfSeason)
        );
        for _ in 0..30 {
            counter.give("shovel", 1);
        }
        assert_eq!(
            counter.buy(monday_noon(), "potato_seeds", 1),
            Err(Refusal::NoRoom)
        );
        assert_eq!(counter.wallet.coins(), 50);
    }
}
