//! The world file: the world's seed, clock, market, fields, the scenery
//! players gathered and what they built.

use glam::{IVec2, Vec3};
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, PropId, StructureId};
use messoria_economy::Market;
use messoria_farming::Planting;
use messoria_inventory::Inventory;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
    stacks::{self, SlotEntry},
};

/// Version of the format this game writes. Version 1 had no gathered
/// scenery, and versions 1 and 2 no structures.
const VERSION: u32 = 3;

/// Everything about a world that is not terrain or players.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldState {
    /// Seeds everything that varies from world to world, such as weather.
    pub seed: u64,
    pub clock: WorldTime,
    pub market: Market,
    pub fields: Vec<FieldState>,
    pub gathered: Vec<GatheredProp>,
    pub structures: Vec<StructureState>,
}

/// Something players built, and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureState {
    pub kind: StructureId,
    pub position: Vec3,
    pub facing: f32,
    /// The player whose home it is, by their key.
    pub home_of: Option<String>,
    /// What it keeps, if it keeps anything.
    pub stored: Option<Inventory>,
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
    // Absent from version 1 and 2 files, which read as nothing built.
    #[serde(default)]
    structures: Vec<StructureEntry>,
}

#[derive(Serialize, Deserialize)]
struct StructureEntry {
    structure: String,
    position: Vec3,
    facing: f32,
    #[serde(default)]
    home_of: Option<String>,
    #[serde(default)]
    stored: Option<Vec<SlotEntry>>,
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
            structures: self
                .structures
                .iter()
                .map(|structure| StructureEntry {
                    structure: catalog.structure(structure.kind).key.clone(),
                    position: structure.position,
                    facing: structure.facing,
                    home_of: structure.home_of.clone(),
                    stored: structure
                        .stored
                        .as_ref()
                        .map(|stored| stacks::to_entries(catalog, stored)),
                })
                .collect(),
        };
        to_ron(&file)
    }

    pub(crate) fn from_ron(text: &str, mut resolver: Resolver<'_>) -> Result<Self, Problem> {
        let file: WorldFile = match version_of(text)? {
            1 | 2 | VERSION => ron::from_str(text)?,
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
        let mut structures = Vec::with_capacity(file.structures.len());
        for entry in file.structures {
            let Some(kind) = resolver.catalog.structure_id(&entry.structure) else {
                resolver.note(&format!(
                    "structure `{}` no longer exists and was left out, with what it held",
                    entry.structure
                ));
                continue;
            };
            let stored = entry
                .stored
                .map(|entries| stacks::from_entries(entries, &mut resolver))
                .transpose()?;
            structures.push(StructureState {
                kind,
                position: entry.position,
                facing: entry.facing,
                home_of: entry.home_of,
                stored,
            });
        }
        Ok(Self {
            seed: file.seed,
            clock: file.clock,
            market,
            fields,
            gathered,
            structures,
        })
    }
}
