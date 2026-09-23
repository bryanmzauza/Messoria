//! What shops pay for goods, which falls as goods flood in and recovers over
//! time.
//!
//! The market remembers a saturation per item: roughly, how many units of it
//! were sold lately. Every unit sold adds one, and each night a share of it
//! fades. A unit sells for its full price divided by `1 + saturation /
//! halves_after`, so the price halves once `halves_after` units have piled
//! up, and never quite reaches zero.

use std::collections::BTreeMap;

use messoria_calendar::Season;
use messoria_content::{Catalog, ItemId, MarketRules, Offer, Quality};
use serde::{Deserialize, Serialize};

/// Saturation low enough to count as none, so recovered items drop out of
/// the market instead of lingering forever.
const FORGOTTEN: f32 = 0.01;

/// Saturation of every item sold lately, shared by the whole server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Market {
    saturation: BTreeMap<ItemId, f32>,
}

/// What one unit of some goods is worth before the market moves the price.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriceBasis {
    pub item: ItemId,
    /// Price of one unit on an unsaturated market.
    pub full_price: f32,
}

impl PriceBasis {
    /// Goods of `quality` sold to a shop making `offer`, in `season`. Produce
    /// is scarce, and worth more, outside the seasons it is harvested in.
    pub fn new(catalog: &Catalog, offer: Offer, quality: Quality, season: Season) -> Self {
        let rules = catalog.market();
        let scarce = catalog
            .harvest_seasons(offer.item)
            .is_some_and(|seasons| !seasons.contains(&season));
        let markup = if scarce { rules.off_season_markup } else { 1.0 };
        #[expect(clippy::cast_precision_loss, reason = "prices are small")]
        let base = offer.base_price as f32;
        Self {
            item: offer.item,
            full_price: base * markup * rules.quality_bonus(quality),
        }
    }
}

impl Market {
    /// Every item sold lately, with its saturation.
    pub fn saturations(&self) -> impl Iterator<Item = (ItemId, f32)> + '_ {
        self.saturation
            .iter()
            .map(|(&item, &saturation)| (item, saturation))
    }

    pub fn saturation(&self, item: ItemId) -> f32 {
        self.saturation.get(&item).copied().unwrap_or(0.0)
    }

    /// What the next unit sells for.
    pub fn unit_price(&self, basis: PriceBasis, rules: &MarketRules) -> u32 {
        price_at(basis, self.saturation(basis.item), rules)
    }

    /// What `count` units fetch sold one after another, each one lowering
    /// the price of the next. Selling in bulk pays the same as selling one
    /// at a time.
    pub fn quote(&self, basis: PriceBasis, count: u16, rules: &MarketRules) -> u32 {
        let saturation = self.saturation(basis.item);
        (0..count)
            .map(|sold| price_at(basis, saturation + f32::from(sold), rules))
            .fold(0, u32::saturating_add)
    }

    /// Sells `count` units, returning what they fetch.
    pub fn sell(&mut self, basis: PriceBasis, count: u16, rules: &MarketRules) -> u32 {
        let paid = self.quote(basis, count, rules);
        *self.saturation.entry(basis.item).or_default() += f32::from(count);
        paid
    }

    /// Lets a night pass, fading every item's saturation.
    pub fn recover(&mut self, rules: &MarketRules) {
        self.saturation.retain(|_, saturation| {
            *saturation *= 1.0 - rules.daily_recovery;
            *saturation >= FORGOTTEN
        });
    }

    pub fn is_empty(&self) -> bool {
        self.saturation.is_empty()
    }
}

/// A market holding these saturations, such as one read back from a save.
impl FromIterator<(ItemId, f32)> for Market {
    fn from_iter<T: IntoIterator<Item = (ItemId, f32)>>(saturations: T) -> Self {
        Self {
            saturation: saturations.into_iter().collect(),
        }
    }
}

/// Price of one unit when the market holds `saturation` of it; always at
/// least one coin.
fn price_at(basis: PriceBasis, saturation: f32, rules: &MarketRules) -> u32 {
    let price = basis.full_price / (1.0 + saturation / rules.halves_after);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "prices are positive and far below u32::MAX"
    )]
    let coins = price.round() as u32;
    coins.max(1)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    const RULES: MarketRules = MarketRules {
        off_season_markup: 1.5,
        halves_after: 150.0,
        daily_recovery: 0.25,
        silver_bonus: 1.25,
        gold_bonus: 1.5,
    };

    fn shipped_catalog() -> Catalog {
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
        Catalog::load(&data_dir).expect("the shipped content is valid")
    }

    fn basis(catalog: &Catalog, item: &str, quality: Quality, season: Season) -> PriceBasis {
        let grocer = catalog.shop(catalog.shop_id("grocer").expect("the grocer exists"));
        let offer = grocer
            .offer(catalog.id(item).expect("the item exists"))
            .expect("the grocer buys it");
        PriceBasis::new(catalog, offer, quality, season)
    }

    fn turnips(full_price: f32) -> PriceBasis {
        PriceBasis {
            item: shipped_catalog().id("turnip").expect("turnips exist"),
            full_price,
        }
    }

    #[test]
    fn prices_halve_after_the_set_number_of_sales() {
        let turnips = turnips(40.0);
        let mut market = Market::default();
        assert_eq!(market.unit_price(turnips, &RULES), 40);

        market.sell(turnips, 150, &RULES);
        assert_eq!(market.unit_price(turnips, &RULES), 20);
        market.sell(turnips, 150, &RULES);
        assert_eq!(market.unit_price(turnips, &RULES), 13);
    }

    #[test]
    fn selling_in_bulk_pays_the_same_as_one_at_a_time() {
        let turnips = turnips(35.0);
        let (mut bulk, mut single) = (Market::default(), Market::default());

        let bulk_paid = bulk.sell(turnips, 60, &RULES);
        let single_paid: u32 = (0..60).map(|_| single.sell(turnips, 1, &RULES)).sum();

        assert_eq!(bulk_paid, single_paid);
        assert_eq!(bulk, single);
        assert!(bulk_paid < 60 * 35);
    }

    #[test]
    fn prices_never_fall_below_one_coin() {
        let turnips = turnips(2.0);
        let mut market = Market::default();
        market.sell(turnips, 10_000, &RULES);
        assert_eq!(market.unit_price(turnips, &RULES), 1);
    }

    #[test]
    fn saturation_fades_night_after_night() {
        let turnips = turnips(35.0);
        let mut market = Market::default();
        market.sell(turnips, 100, &RULES);

        market.recover(&RULES);
        assert!((market.saturation(turnips.item) - 75.0).abs() < 1e-3);
        for _ in 0..40 {
            market.recover(&RULES);
        }
        assert!(market.is_empty());
        assert_eq!(market.unit_price(turnips, &RULES), 35);
    }

    #[test]
    fn produce_is_worth_more_out_of_season_and_at_better_quality() {
        let catalog = shipped_catalog();
        let price = |item, quality, season| {
            Market::default().unit_price(basis(&catalog, item, quality, season), catalog.market())
        };

        // Turnips are a spring crop; wild berries grow in no field.
        assert_eq!(price("turnip", Quality::Normal, Season::Spring), 35);
        assert_eq!(price("turnip", Quality::Normal, Season::Winter), 53);
        assert_eq!(price("turnip", Quality::Gold, Season::Spring), 53);
        assert_eq!(price("wild_berries", Quality::Normal, Season::Spring), 10);
        assert_eq!(price("wild_berries", Quality::Normal, Season::Winter), 10);
    }

    /// Runs `days` of trading in which `sellers` players each sell a
    /// grocer's full daily limit of every item in `crops`, and returns the
    /// average price per unit on the last day.
    fn average_price_on_last_day(sellers: u16, crops: &[&str], days: u32) -> f32 {
        let catalog = shipped_catalog();
        let rules = catalog.market();
        let grocer = catalog.shop(catalog.shop_id("grocer").expect("the grocer exists"));
        let mut market = Market::default();
        let mut last_day = (0, 0);
        for _ in 0..days {
            last_day = (0, 0);
            for _ in 0..sellers {
                for crop in crops {
                    let basis = basis(&catalog, crop, Quality::Normal, Season::Spring);
                    last_day.0 += market.sell(basis, grocer.daily_limit, rules);
                    last_day.1 += u32::from(grocer.daily_limit);
                }
            }
            market.recover(rules);
        }
        #[expect(clippy::cast_precision_loss, reason = "small totals")]
        let average = last_day.0 as f32 / last_day.1 as f32;
        average
    }

    /// The design: one farmer selling a full day's limit every day keeps
    /// most of the price, while a whole server flooding the market with one
    /// crop makes it nearly worthless, and spreading the same work over
    /// several crops pays far better.
    #[test]
    fn prices_under_load_match_the_design() {
        let turnip_base = 35.0;
        let alone = average_price_on_last_day(1, &["turnip"], 30);
        let crowd = average_price_on_last_day(50, &["turnip"], 30);
        let diversified = average_price_on_last_day(
            10,
            &["turnip", "potato", "strawberry", "wheat", "tomato"],
            30,
        );

        assert!(alone > turnip_base * 0.4, "alone: {alone}");
        assert!(crowd < turnip_base * 0.1, "crowd: {crowd}");
        assert!(
            diversified > crowd * 3.0,
            "diversified: {diversified}, crowd: {crowd}"
        );
    }

    /// Selling the same amount every day settles where each night's recovery
    /// undoes the day's sales: `s = daily × (1 - r) / r` at dawn.
    #[test]
    fn steady_selling_settles_where_recovery_balances_sales() {
        let turnips = turnips(35.0);
        let mut market = Market::default();
        for _ in 0..200 {
            market.sell(turnips, 60, &RULES);
            market.recover(&RULES);
        }
        let expected = 60.0 * (1.0 - RULES.daily_recovery) / RULES.daily_recovery;
        assert!(
            (market.saturation(turnips.item) - expected).abs() < 0.01,
            "saturation {}",
            market.saturation(turnips.item)
        );
    }
}
