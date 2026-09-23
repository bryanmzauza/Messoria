//! The world file: the world's seed, clock, market and fields.

use glam::IVec2;
use messoria_calendar::WorldTime;
use messoria_content::Catalog;
use messoria_economy::Market;
use messoria_farming::Planting;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
};

/// Version of the format this game writes.
const VERSION: u32 = 1;

/// Everything about a world that is not terrain or players.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldState {
    /// Seeds everything that varies from world to world, such as weather.
    pub seed: u64,
    pub clock: WorldTime,
    pub market: Market,
    pub fields: Vec<FieldState>,
}

/// One tilled field and what grows in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldState {
    pub tile: IVec2,
    /// Height of the ground in the middle of the field.
    pub height: f32,
    pub watered: bool,
    pub fertilized: bool,
    pub crop: Option<Planting>,
}

/// The world file as written.
#[derive(Serialize, Deserialize)]
struct WorldFile {
    version: u32,
    seed: u64,
    clock: WorldTime,
    /// Saturation of each item, by item id.
    market: Vec<(String, f32)>,
    fields: Vec<FieldEntry>,
}

#[derive(Serialize, Deserialize)]
struct FieldEntry {
    tile: IVec2,
    height: f32,
    watered: bool,
    fertilized: bool,
    crop: Option<CropEntry>,
}

#[derive(Serialize, Deserialize)]
struct CropEntry {
    crop: String,
    days_grown: u16,
}

impl WorldState {
    pub(crate) fn to_ron(&self, catalog: &Catalog) -> Result<String, Problem> {
        let file = WorldFile {
            version: VERSION,
            seed: self.seed,
            clock: self.clock,
            market: self
                .market
                .saturations()
                .map(|(item, saturation)| (catalog.item(item).key.clone(), saturation))
                .collect(),
            fields: self
                .fields
                .iter()
                .map(|field| FieldEntry {
                    tile: field.tile,
                    height: field.height,
                    watered: field.watered,
                    fertilized: field.fertilized,
                    crop: field.crop.map(|planting| CropEntry {
                        crop: catalog.crop(planting.crop).key.clone(),
                        days_grown: planting.days_grown,
                    }),
                })
                .collect(),
        };
        to_ron(&file)
    }

    pub(crate) fn from_ron(text: &str, mut resolver: Resolver<'_>) -> Result<Self, Problem> {
        let file: WorldFile = match version_of(text)? {
            VERSION => ron::from_str(text)?,
            // Files from older versions of the format are read and upgraded here.
            found => return Err(unreadable(found, VERSION)),
        };
        let (day, minute) = (file.clock.day(), file.clock.minutes_since_dawn());
        if WorldTime::new(day, minute).is_none() {
            return Err(Problem::OutOfRange(format!(
                "the time {minute} minutes after dawn"
            )));
        }

        let market = file
            .market
            .iter()
            .filter_map(|(key, saturation)| Some((resolver.item(key)?, *saturation)))
            .collect();
        let fields = file
            .fields
            .into_iter()
            .map(|field| {
                let crop = field.crop.and_then(|entry| {
                    let crop = resolver.catalog.crop_id(&entry.crop);
                    if crop.is_none() {
                        resolver.note(&format!(
                            "crop `{}` no longer exists; its field was left bare",
                            entry.crop
                        ));
                    }
                    crop.map(|crop| Planting {
                        crop,
                        days_grown: entry.days_grown,
                    })
                });
                FieldState {
                    tile: field.tile,
                    height: field.height,
                    watered: field.watered,
                    fertilized: field.fertilized,
                    crop,
                }
            })
            .collect();
        Ok(Self {
            seed: file.seed,
            clock: file.clock,
            market,
            fields,
        })
    }
}
