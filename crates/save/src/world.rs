//! The world file: the world's seed, clock, market, fields and the scenery
//! players gathered.

use glam::IVec2;
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, PropId};
use messoria_economy::Market;
use messoria_farming::Planting;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
};

/// Version of the format this game writes. Version 1 had no gathered
/// scenery.
const VERSION: u32 = 2;

/// Everything about a world that is not terrain or players.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldState {
    /// Seeds everything that varies from world to world, such as weather.
    pub seed: u64,
    pub clock: WorldTime,
    pub market: Market,
    pub fields: Vec<FieldState>,
    pub gathered: Vec<GatheredProp>,
}

/// A prop players gathered. The seed grows every prop again when the world
/// loads, so the save only says which ones were gathered, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatheredProp {
    pub kind: PropId,
    /// The cell of its kind's scattering grid it grew in. A kind grows at
    /// most one prop per cell, so this and the kind name the prop.
    pub cell: [u16; 2],
    /// The day it was gathered.
    pub day: u32,
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
    // Absent from version 1 files, which read as nothing gathered.
    #[serde(default)]
    gathered: Vec<GatheredEntry>,
}

#[derive(Serialize, Deserialize)]
struct GatheredEntry {
    prop: String,
    cell: [u16; 2],
    day: u32,
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
            gathered: self
                .gathered
                .iter()
                .map(|gathered| GatheredEntry {
                    prop: catalog.prop(gathered.kind).key.clone(),
                    cell: gathered.cell,
                    day: gathered.day,
                })
                .collect(),
        };
        to_ron(&file)
    }

    pub(crate) fn from_ron(text: &str, mut resolver: Resolver<'_>) -> Result<Self, Problem> {
        let file: WorldFile = match version_of(text)? {
            1 | VERSION => ron::from_str(text)?,
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
        let gathered = file
            .gathered
            .into_iter()
            .filter_map(|entry| {
                let kind = resolver.catalog.prop_id(&entry.prop);
                if kind.is_none() {
                    resolver.note(&format!(
                        "prop `{}` no longer exists; what was gathered of it was forgotten",
                        entry.prop
                    ));
                }
                Some(GatheredProp {
                    kind: kind?,
                    cell: entry.cell,
                    day: entry.day,
                })
            })
            .collect();
        Ok(Self {
            seed: file.seed,
            clock: file.clock,
            market,
            fields,
            gathered,
        })
    }
}
