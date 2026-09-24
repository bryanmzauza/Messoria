//! How crops grow, wither and yield.
//!
//! Crops grow in whole days: each night, a crop whose field was watered
//! during the day grows by one day. A crop ripens once its days of growth add
//! up to its stages, so one planted on day `d` and watered every day ripens at
//! dawn of day `d + days_to_ripen`. Out of season it withers overnight.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

use messoria_calendar::Season;
use messoria_content::{CropDef, CropId, ItemId, Quality};
use serde::{Deserialize, Serialize};

/// A crop growing in a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Planting {
    pub crop: CropId,
    /// Days of watered growth so far, up to what the crop needs to ripen.
    pub days_grown: u16,
}

/// What a night did to a crop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overnight {
    Grew,
    /// Its field was not watered, so it did not grow.
    Dry,
    /// It was already ripe and waits to be harvested.
    Waiting,
    /// The new day is out of its season; the crop is gone.
    Withered,
}

/// What a harvest yields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harvest {
    pub produce: ItemId,
    pub count: u16,
    pub quality: Quality,
    /// Whether the plant stays in the field to ripen again.
    pub regrows: bool,
}

impl Planting {
    pub fn new(crop: CropId) -> Self {
        Self {
            crop,
            days_grown: 0,
        }
    }

    pub fn is_ripe(&self, crop: &CropDef) -> bool {
        self.days_grown >= crop.days_to_ripen()
    }

    /// Stages completed, from 0 when just planted to the number of stages
    /// when ripe.
    pub fn stage(&self, crop: &CropDef) -> usize {
        let mut days = 0;
        crop.stages
            .iter()
            .take_while(|&&stage| {
                days += u16::from(stage);
                days <= self.days_grown
            })
            .count()
    }

    /// Advances the crop through one night. `watered` is whether its field was
    /// watered during the day that ended; `season` is the season of the day
    /// that begins.
    pub fn grow_overnight(&mut self, crop: &CropDef, watered: bool, season: Season) -> Overnight {
        if !crop.seasons.contains(&season) {
            Overnight::Withered
        } else if self.is_ripe(crop) {
            Overnight::Waiting
        } else if watered {
            self.days_grown += 1;
            Overnight::Grew
        } else {
            Overnight::Dry
        }
    }

    /// Harvests a ripe crop in the given quality. Crops that regrow start over
    /// from the stage they regrow from; the rest are done. Returns `None`
    /// if the crop is not ripe.
    pub fn harvest(&mut self, crop: &CropDef, quality: Quality) -> Option<Harvest> {
        if !self.is_ripe(crop) {
            return None;
        }
        if let Some(days) = crop.regrows_after {
            self.days_grown = crop.days_to_ripen() - u16::from(days);
        }
        Some(Harvest {
            produce: crop.produce,
            count: crop.harvest,
            quality,
            regrows: crop.regrows_after.is_some(),
        })
    }
}

/// The quality of a harvest, from `roll`, a number drawn uniformly from
/// `0..100`. Fertilized fields make better harvests more likely.
pub fn harvest_quality(roll: u8, fertilized: bool) -> Quality {
    let (gold, silver) = if fertilized { (20, 60) } else { (5, 25) };
    if roll < gold {
        Quality::Gold
    } else if roll < silver {
        Quality::Silver
    } else {
        Quality::Normal
    }
}

#[cfg(test)]
mod tests {
    use messoria_content::{Catalog, Sources};

    use super::*;

    const ITEMS: &str = r#"(
        items: [
            (id: "soil", name: "Soil", kind: Terrain(materials: [Grass, Soil, Stone, Sand]), stack: 99),
            (id: "turnip_seeds", name: "Turnip seeds", kind: Seed, stack: 99),
            (id: "turnip", name: "Turnip", kind: Goods, stack: 99),
            (id: "berry_seeds", name: "Berry seeds", kind: Seed, stack: 99),
            (id: "berry", name: "Berry", kind: Goods, stack: 99),
        ],
        starting_inventory: [],
        starting_money: 0,
    )"#;
    /// Market rules and no shops.
    /// No scenery, and a palette with only the ground colors.
    const SCENERY: &str = "(props: [], cover: [])";
    const PALETTE: &str = "(colors: {\"terrain_grass\": (0.3, 0.5, 0.2), \"terrain_soil\": (0.4, 0.3, 0.2), \"terrain_stone\": (0.5, 0.5, 0.5), \"terrain_sand\": (0.8, 0.7, 0.5)})";
    const SHOPS: &str = "(market: (off_season_markup: 1.5, halves_after: 100.0, daily_recovery: 0.25, silver_bonus: 1.25, gold_bonus: 1.5), shops: [])";
    const CROPS: &str = r#"(
        crops: [
            (id: "turnip", name: "Turnip", seeds: "turnip_seeds", produce: "turnip",
             seasons: [Spring], stages: [1, 1, 2], color: (1, 1, 1),
             models: ["a.glb", "b.glb", "c.glb", "d.glb"]),
            (id: "berry", name: "Berry", seeds: "berry_seeds", produce: "berry",
             seasons: [Spring, Summer], stages: [2, 2], regrows_after: 3, harvest: 2, color: (1, 0, 0),
             models: ["a.glb", "b.glb", "c.glb"]),
        ],
    )"#;

    fn crops() -> (Catalog, CropId, CropId) {
        let catalog = Catalog::from_sources(Sources {
            items: ITEMS,
            crops: CROPS,
            shops: SHOPS,
            scenery: SCENERY,
            palette: PALETTE,
            structures: "(structures: [])",
            characters: "(models: [\"people/a.glb\"], scale: 1.0, hand: \"hand\", grip: (at: (0.0, 0.0, 0.0), turn: (0.0, 0.0, 0.0), scale: 1.0), animations: (idle: \"idle\", walk: \"walk\", run: \"run\", jump: \"jump\", fall: \"fall\", swing: \"swing\", work: \"work\", pick: \"pick\", greet: \"greet\"))",
            village: "(stalls: [])",
        })
        .unwrap();
        let turnip = catalog.crop_id("turnip").unwrap();
        let berry = catalog.crop_id("berry").unwrap();
        (catalog, turnip, berry)
    }

    #[test]
    fn a_crop_watered_daily_ripens_when_its_stages_say() {
        let (catalog, turnip, _) = crops();
        let crop = catalog.crop(turnip);
        let mut planting = Planting::new(turnip);

        let mut nights = 0;
        while !planting.is_ripe(crop) {
            assert_eq!(
                planting.grow_overnight(crop, true, Season::Spring),
                Overnight::Grew
            );
            nights += 1;
        }
        assert_eq!(nights, crop.days_to_ripen());
        assert_eq!(nights, 4);
    }

    #[test]
    fn stages_follow_the_days_grown() {
        let (catalog, turnip, _) = crops();
        let crop = catalog.crop(turnip);
        let stage_after = |days_grown| {
            Planting {
                crop: turnip,
                days_grown,
            }
            .stage(crop)
        };

        assert_eq!([0, 1, 2, 3, 4].map(stage_after), [0, 1, 2, 2, 3]);
    }

    #[test]
    fn dry_nights_do_not_count() {
        let (catalog, turnip, _) = crops();
        let crop = catalog.crop(turnip);
        let mut planting = Planting::new(turnip);

        assert_eq!(
            planting.grow_overnight(crop, false, Season::Spring),
            Overnight::Dry
        );
        assert_eq!(planting.days_grown, 0);
    }

    #[test]
    fn crops_wither_out_of_season() {
        let (catalog, turnip, _) = crops();
        let mut planting = Planting::new(turnip);
        assert_eq!(
            planting.grow_overnight(catalog.crop(turnip), true, Season::Summer),
            Overnight::Withered
        );
    }

    #[test]
    fn only_ripe_crops_can_be_harvested() {
        let (catalog, turnip, _) = crops();
        let crop = catalog.crop(turnip);
        let mut planting = Planting::new(turnip);
        assert_eq!(planting.harvest(crop, Quality::Normal), None);

        planting.days_grown = crop.days_to_ripen();
        let harvest = planting.harvest(crop, Quality::Silver).unwrap();
        assert_eq!(harvest.produce, crop.produce);
        assert_eq!(
            (harvest.count, harvest.quality, harvest.regrows),
            (1, Quality::Silver, false)
        );
    }

    #[test]
    fn regrowing_crops_ripen_again() {
        let (catalog, _, berry) = crops();
        let crop = catalog.crop(berry);
        let mut planting = Planting {
            crop: berry,
            days_grown: crop.days_to_ripen(),
        };

        let harvest = planting.harvest(crop, Quality::Normal).unwrap();
        assert!(harvest.regrows);
        assert_eq!(harvest.count, 2);

        let mut nights = 0;
        while !planting.is_ripe(crop) {
            planting.grow_overnight(crop, true, Season::Summer);
            nights += 1;
        }
        assert_eq!(nights, 3);
    }

    #[test]
    fn ripe_crops_wait() {
        let (catalog, turnip, _) = crops();
        let crop = catalog.crop(turnip);
        let mut planting = Planting {
            crop: turnip,
            days_grown: crop.days_to_ripen(),
        };
        assert_eq!(
            planting.grow_overnight(crop, true, Season::Spring),
            Overnight::Waiting
        );
        assert_eq!(planting.days_grown, crop.days_to_ripen());
    }

    #[test]
    fn fertilizer_improves_the_odds() {
        let tally = |fertilized| {
            let mut counts = [0_u32; 3];
            for roll in 0..100 {
                counts[harvest_quality(roll, fertilized) as usize] += 1;
            }
            counts
        };
        assert_eq!(tally(false), [75, 20, 5]);
        assert_eq!(tally(true), [40, 40, 20]);
    }
}
