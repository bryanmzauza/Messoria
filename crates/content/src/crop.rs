//! Crop definitions and the crops file.

use std::collections::HashMap;

use messoria_calendar::Season;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    item::{ItemId, ItemKind, Items},
    scenery::check_models,
};

/// Refers to a crop definition within a [`Catalog`](crate::Catalog), like
/// [`ItemId`] does for items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CropId(pub(crate) u16);

/// Everything the game knows about one kind of crop.
#[derive(Clone, Debug, PartialEq)]
pub struct CropDef {
    /// The id used in data files, such as `"turnip"`.
    pub key: String,
    pub name: String,
    /// The seed item planted to grow it.
    pub seeds: ItemId,
    /// The item a harvest yields.
    pub produce: ItemId,
    /// Seasons it grows in; out of season it withers.
    pub seasons: Vec<Season>,
    /// Days of watered growth each stage takes, from sprout to ripe.
    pub stages: Vec<u8>,
    /// For crops that keep producing, the days of growth they need to ripen
    /// again after a harvest.
    pub regrows_after: Option<u8>,
    /// Produce yielded by each harvest.
    pub harvest: u16,
    /// Color of the ripe produce, in sRGB, for drawing it.
    pub color: [f32; 3],
    /// Models the plant is drawn with, from just planted to ripe: one more
    /// than it has stages.
    pub models: Vec<String>,
    /// Whether the ripe model shows the produce; otherwise it is drawn on
    /// the plant in the produce's color.
    pub produce_shown: bool,
}

impl CropDef {
    /// Days of watered growth from planting to the first harvest.
    pub fn days_to_ripen(&self) -> u16 {
        self.stages.iter().map(|&days| u16::from(days)).sum()
    }
}

/// The crops file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CropsFile {
    crops: Vec<CropEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CropEntry {
    id: String,
    name: String,
    seeds: String,
    produce: String,
    seasons: Vec<Season>,
    stages: Vec<u8>,
    #[serde(default)]
    regrows_after: Option<u8>,
    #[serde(default = "single")]
    harvest: u16,
    color: (f32, f32, f32),
    models: Vec<String>,
    #[serde(default)]
    produce_shown: bool,
}

fn single() -> u16 {
    1
}

/// Crops with their references resolved.
pub(crate) struct Crops {
    pub definitions: Vec<CropDef>,
    pub by_key: HashMap<String, CropId>,
    pub by_seed: HashMap<ItemId, CropId>,
}

impl CropsFile {
    /// Resolves each crop's items, checks it, and checks that every seed item
    /// grows exactly one crop.
    pub(crate) fn resolve(self, items: &Items) -> Result<Crops, Problem> {
        let mut crops = Crops {
            definitions: Vec::with_capacity(self.crops.len()),
            by_key: HashMap::new(),
            by_seed: HashMap::new(),
        };
        for (entry, id) in self.crops.into_iter().zip((0..=u16::MAX).map(CropId)) {
            if crops.by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateCrop(entry.id));
            }
            let context = || format!("crop `{}`", entry.id);
            let seeds = items.resolve(context, &entry.seeds)?;
            let produce = items.resolve(context, &entry.produce)?;
            if items.get(seeds).kind != ItemKind::Seed {
                return Err(Problem::NotASeed {
                    crop: entry.id,
                    item: entry.seeds,
                });
            }
            if let Some(other) = crops.by_seed.insert(seeds, id) {
                return Err(Problem::SharedSeed {
                    seed: entry.seeds,
                    first: crops.definitions[usize::from(other.0)].key.clone(),
                    second: entry.id,
                });
            }

            let (red, green, blue) = entry.color;
            let crop = CropDef {
                key: entry.id,
                name: entry.name,
                seeds,
                produce,
                seasons: entry.seasons,
                stages: entry.stages,
                regrows_after: entry.regrows_after,
                harvest: entry.harvest,
                color: [red, green, blue],
                models: entry.models,
                produce_shown: entry.produce_shown,
            };
            validate(&crop)?;
            crops.definitions.push(crop);
        }

        if let Some(seed) = items
            .definitions
            .iter()
            .zip((0..=u16::MAX).map(ItemId))
            .find(|(item, id)| item.kind == ItemKind::Seed && !crops.by_seed.contains_key(id))
        {
            return Err(Problem::UnplantedSeed(seed.0.key.clone()));
        }
        Ok(crops)
    }
}

fn validate(crop: &CropDef) -> Result<(), Problem> {
    let problem = |reason: &str| {
        Err(Problem::InvalidCrop {
            crop: crop.key.clone(),
            reason: reason.to_owned(),
        })
    };
    if crop.seasons.is_empty() {
        return problem("it grows in no season");
    }
    if crop.stages.is_empty() || crop.stages.contains(&0) {
        return problem("every crop needs at least one stage, and every stage at least one day");
    }
    if crop.harvest == 0 {
        return problem("a harvest must yield something");
    }
    if crop
        .regrows_after
        .is_some_and(|days| days == 0 || u16::from(days) > crop.days_to_ripen())
    {
        return problem("it must take between one day and its full growth to regrow");
    }
    if crop.models.len() != crop.stages.len() + 1 {
        return problem("it needs a model for each stage, and one for when it is ripe");
    }
    if check_models(&crop.models).is_err() {
        return problem("its models must be .glb files inside the models folder");
    }
    Ok(())
}
