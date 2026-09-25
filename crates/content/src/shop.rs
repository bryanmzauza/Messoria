//! Shop definitions, the rules market prices follow, and the shops file.

use std::collections::HashMap;

use messoria_calendar::{ClockTime, Season, Weekday, WorldTime};
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    item::{ItemId, Items, Quality},
    people::Look,
    scenery::check_models,
};

/// Refers to a shop definition within a [`Catalog`](crate::Catalog), like
/// [`ItemId`] does for items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShopId(pub(crate) u16);

/// Everything the game knows about one shop.
#[derive(Clone, Debug, PartialEq)]
pub struct ShopDef {
    /// The id used in data files, such as `"grocer"`.
    pub key: String,
    pub name: String,
    pub opens: ClockTime,
    pub closes: ClockTime,
    pub closed_on: Vec<Weekday>,
    /// Most units of each item one player can sell the shop in a day.
    pub daily_limit: u16,
    /// What the shop buys, at what base price per unit.
    pub buys: Vec<Offer>,
    /// What the shop sells.
    pub sells: Vec<Listing>,
    /// Model the stall is drawn with, relative to the models folder.
    pub stall: String,
    /// How whoever keeps the shop is dressed.
    pub keeper: Look,
}

/// An item a shop buys, and its base price per unit before the market moves
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub item: ItemId,
    pub base_price: u32,
}

/// An item a shop sells, at a fixed price.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub item: ItemId,
    pub price: u32,
    /// Seasons it is on sale in; all year if `None`.
    pub seasons: Option<Vec<Season>>,
}

impl Listing {
    pub fn on_sale_in(&self, season: Season) -> bool {
        self.seasons
            .as_ref()
            .is_none_or(|seasons| seasons.contains(&season))
    }
}

impl ShopDef {
    /// Whether the shop trades at `time`.
    pub fn is_open(&self, time: WorldTime) -> bool {
        let clock = time.clock();
        !self.closed_on.contains(&time.date().weekday) && self.opens <= clock && clock < self.closes
    }

    /// What the shop pays for `item`, if it buys it at all.
    pub fn offer(&self, item: ItemId) -> Option<Offer> {
        self.buys.iter().copied().find(|offer| offer.item == item)
    }

    /// How the shop sells `item`, if it sells it at all.
    pub fn listing(&self, item: ItemId) -> Option<&Listing> {
        self.sells.iter().find(|listing| listing.item == item)
    }
}

/// How the prices shops pay move. One set of rules applies to every shop on a
/// server.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketRules {
    /// Produce sells for this many times its base price outside the seasons
    /// its crop grows in, when it is scarce.
    pub off_season_markup: f32,
    /// Units of an item sold across the server that halve its price.
    pub halves_after: f32,
    /// Share of an item's saturation that fades each night.
    pub daily_recovery: f32,
    /// Price multipliers for better harvests.
    pub silver_bonus: f32,
    pub gold_bonus: f32,
}

impl MarketRules {
    /// The price multiplier for goods of `quality`.
    pub fn quality_bonus(&self, quality: Quality) -> f32 {
        match quality {
            Quality::Normal => 1.0,
            Quality::Silver => self.silver_bonus,
            Quality::Gold => self.gold_bonus,
        }
    }

    fn validate(&self) -> Result<(), Problem> {
        let problem = |reason: &str| Err(Problem::InvalidMarket(reason.to_owned()));
        // Written as ranges so that a missing number (NaN) fails every check.
        if !(1.0..).contains(&self.off_season_markup) {
            return problem("the off-season markup must be at least 1");
        }
        if !(f32::MIN_POSITIVE..).contains(&self.halves_after) {
            return problem("prices must take a positive number of sales to halve");
        }
        if !(0.0..=1.0).contains(&self.daily_recovery) {
            return problem("the daily recovery must be between 0 and 1");
        }
        if !(1.0..).contains(&self.silver_bonus)
            || !(self.silver_bonus..).contains(&self.gold_bonus)
        {
            return problem("quality bonuses must be at least 1 and grow with quality");
        }
        Ok(())
    }
}

/// The shops file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShopsFile {
    market: MarketRules,
    shops: Vec<ShopEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShopEntry {
    id: String,
    name: String,
    opens: String,
    closes: String,
    #[serde(default)]
    closed_on: Vec<Weekday>,
    daily_limit: u16,
    buys: Vec<(String, u32)>,
    sells: Vec<ListingEntry>,
    stall: String,
    keeper: Look,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListingEntry {
    item: String,
    price: u32,
    #[serde(default)]
    seasons: Option<Vec<Season>>,
}

/// Shops with their references resolved.
pub(crate) struct Shops {
    pub market: MarketRules,
    pub definitions: Vec<ShopDef>,
    pub by_key: HashMap<String, ShopId>,
}

impl ShopsFile {
    /// Resolves each shop's items and checks every shop and the market rules.
    pub(crate) fn resolve(self, items: &Items) -> Result<Shops, Problem> {
        self.market.validate()?;
        let mut shops = Shops {
            market: self.market,
            definitions: Vec::with_capacity(self.shops.len()),
            by_key: HashMap::new(),
        };
        for (entry, id) in self.shops.into_iter().zip((0..=u16::MAX).map(ShopId)) {
            if shops.by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateShop(entry.id));
            }
            let shop = entry.resolve(items)?;
            validate(&shop)?;
            shops.definitions.push(shop);
        }
        Ok(shops)
    }
}

impl ShopEntry {
    fn resolve(self, items: &Items) -> Result<ShopDef, Problem> {
        let context = || format!("shop `{}`", self.id);
        let time = |text: &str| {
            text.parse().map_err(|_| Problem::InvalidShop {
                shop: self.id.clone(),
                reason: format!("`{text}` is not a time such as 09:30"),
            })
        };
        let buys = self
            .buys
            .iter()
            .map(|(item, base_price)| {
                Ok(Offer {
                    item: items.resolve(context, item)?,
                    base_price: *base_price,
                })
            })
            .collect::<Result<_, Problem>>()?;
        let sells = self
            .sells
            .into_iter()
            .map(|listing| {
                Ok(Listing {
                    item: items.resolve(context, &listing.item)?,
                    price: listing.price,
                    seasons: listing.seasons,
                })
            })
            .collect::<Result<_, Problem>>()?;
        Ok(ShopDef {
            opens: time(&self.opens)?,
            closes: time(&self.closes)?,
            key: self.id,
            name: self.name,
            closed_on: self.closed_on,
            daily_limit: self.daily_limit,
            buys,
            sells,
            stall: self.stall,
            keeper: self.keeper,
        })
    }
}

fn validate(shop: &ShopDef) -> Result<(), Problem> {
    let problem = |reason: &str| {
        Err(Problem::InvalidShop {
            shop: shop.key.clone(),
            reason: reason.to_owned(),
        })
    };
    // Days start at dawn, so a shop open past midnight would span two days.
    let dawn = ClockTime { hour: 6, minute: 0 };
    if shop.opens < dawn || shop.closes <= shop.opens {
        return problem("it must open after 06:00 and close later the same day");
    }
    if shop.daily_limit == 0 {
        return problem("its daily limit must allow selling something");
    }
    if check_models(std::slice::from_ref(&shop.stall)).is_err() {
        return problem("its stall must be a .glb file inside the models folder");
    }
    if let Err(reason) = shop.keeper.validate() {
        return problem(&format!("its keeper's look is invalid: {reason}"));
    }
    if shop.buys.iter().any(|offer| offer.base_price == 0)
        || shop.sells.iter().any(|listing| listing.price == 0)
    {
        return problem("every price must be above 0");
    }
    if shop
        .sells
        .iter()
        .any(|listing| listing.seasons.as_ref().is_some_and(Vec::is_empty))
    {
        return problem("an item on sale in no season should not be listed");
    }
    let mut bought: Vec<_> = shop.buys.iter().map(|offer| offer.item).collect();
    let mut sold: Vec<_> = shop.sells.iter().map(|listing| listing.item).collect();
    for items in [&mut bought, &mut sold] {
        let count = items.len();
        items.sort_unstable();
        items.dedup();
        if items.len() != count {
            return problem("it lists an item twice");
        }
    }
    Ok(())
}
