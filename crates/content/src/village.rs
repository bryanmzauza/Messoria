//! The village file: where the shops' stalls and the village's buildings
//! stand around the square.

use std::{collections::HashMap, f32::consts::PI};

use serde::Deserialize;

use crate::{
    error::Problem,
    shop::ShopId,
    structure::{Placement, StructureId},
};

/// The village's layout, relative to the middle of its square, and the
/// farm lots around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Village {
    pub stalls: Vec<Stall>,
    pub structures: Vec<Placement>,
    pub lots: Lots,
}

/// What the village asks for a farm lot, and how the lots are fenced.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lots {
    /// What a lot costs, in coins, after a player's first.
    pub price: u32,
    /// A section of fence, relative to the models folder: a post with
    /// rails running along +x to where the next section starts.
    pub fence: String,
    /// A gateway, 4 m across x, standing where the fence opens.
    pub gate: String,
}

/// Where a shop trades: the middle of its counter, and the way the counter
/// faces, in radians, as a heading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stall {
    pub shop: ShopId,
    pub at: (f32, f32),
    pub turn: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VillageFile {
    stalls: Vec<PlaceEntry>,
    #[serde(default)]
    structures: Vec<PlaceEntry>,
    lots: Lots,
}

/// Something placed in the village: a shop's stall or a structure, by id.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceEntry {
    id: String,
    at: (f32, f32),
    /// In degrees.
    #[serde(default)]
    turn: f32,
}

impl VillageFile {
    pub(crate) fn resolve(
        self,
        shops: &HashMap<String, ShopId>,
        structures: &HashMap<String, StructureId>,
    ) -> Result<Village, Problem> {
        let unknown = |what: &str, id: &str| {
            Problem::InvalidVillage(format!("it places unknown {what} `{id}`"))
        };
        let stalls = self
            .stalls
            .iter()
            .map(|entry| {
                Ok(Stall {
                    shop: *shops
                        .get(&entry.id)
                        .ok_or_else(|| unknown("shop", &entry.id))?,
                    at: entry.at,
                    turn: entry.turn * PI / 180.0,
                })
            })
            .collect::<Result<Vec<_>, Problem>>()?;
        let placed = self
            .structures
            .iter()
            .map(|entry| {
                Ok(Placement {
                    structure: *structures
                        .get(&entry.id)
                        .ok_or_else(|| unknown("structure", &entry.id))?,
                    at: entry.at,
                    turn: entry.turn * PI / 180.0,
                })
            })
            .collect::<Result<Vec<_>, Problem>>()?;
        if let Some((key, _)) = shops
            .iter()
            .find(|(_, id)| stalls.iter().filter(|stall| stall.shop == **id).count() != 1)
        {
            return Err(Problem::InvalidVillage(format!(
                "shop `{key}` must have exactly one stall"
            )));
        }
        if self.lots.price == 0 {
            return Err(Problem::InvalidVillage(
                "a farm lot must cost something after the first".to_owned(),
            ));
        }
        Ok(Village {
            stalls,
            structures: placed,
            lots: self.lots,
        })
    }
}
