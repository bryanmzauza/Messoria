//! What each player sold today, against shops' daily limits.

use std::collections::BTreeMap;

use messoria_content::{ItemId, ShopId};
use serde::{Deserialize, Serialize};

/// Units of each item one player sold each shop today.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesLedger {
    sold: BTreeMap<(ShopId, ItemId), u16>,
}

impl SalesLedger {
    pub fn sold(&self, shop: ShopId, item: ItemId) -> u16 {
        self.sold.get(&(shop, item)).copied().unwrap_or(0)
    }

    /// Units of `item` the shop still buys from this player today.
    pub fn remaining(&self, shop: ShopId, item: ItemId, daily_limit: u16) -> u16 {
        daily_limit.saturating_sub(self.sold(shop, item))
    }

    pub fn record(&mut self, shop: ShopId, item: ItemId, count: u16) {
        let sold = self.sold.entry((shop, item)).or_default();
        *sold = sold.saturating_add(count);
    }

    /// Starts a new day.
    pub fn clear(&mut self) {
        self.sold.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.sold.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use messoria_content::Catalog;

    use super::*;

    #[test]
    fn daily_limits_count_each_shop_and_item_apart_until_the_day_ends() {
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
        let catalog = Catalog::load(&data_dir).expect("the shipped content is valid");
        let grocer = catalog.shop_id("grocer").expect("the grocer exists");
        let (turnip, potato) = (
            catalog.id("turnip").expect("turnips exist"),
            catalog.id("potato").expect("potatoes exist"),
        );

        let mut ledger = SalesLedger::default();
        ledger.record(grocer, turnip, 45);
        ledger.record(grocer, turnip, 10);
        assert_eq!(ledger.remaining(grocer, turnip, 60), 5);
        assert_eq!(ledger.remaining(grocer, potato, 60), 60);
        ledger.record(grocer, turnip, 10);
        assert_eq!(ledger.remaining(grocer, turnip, 60), 0);

        ledger.clear();
        assert!(ledger.is_empty());
        assert_eq!(ledger.remaining(grocer, turnip, 60), 60);
    }
}
