//! The village file: where the shops' stalls and the village's buildings
//! stand around the square, the spots villagers go to and the paths they
//! walk between them.

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
    /// Which of `structures` each label names.
    pub labels: HashMap<String, usize>,
    pub spots: Vec<Spot>,
    pub paths: Paths,
    pub lots: Lots,
}

impl Village {
    /// The labeled building `label` names.
    pub fn labeled(&self, label: &str) -> Option<&Placement> {
        self.labels.get(label).map(|&index| &self.structures[index])
    }

    pub fn spot(&self, label: &str) -> Option<&Spot> {
        self.spots.iter().find(|spot| spot.label == label)
    }
}

/// A place a villager goes to: where to stand, which way to face, how to
/// hold themselves there, and the spots to pass on the way in from the
/// paths, such as a shop's door, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub label: String,
    pub at: (f32, f32),
    /// As a heading, in radians.
    pub face: f32,
    pub pose: SpotPose,
    pub via: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum SpotPose {
    #[default]
    Stand,
    Sit,
    /// Bent over the ground, as over a garden bed.
    Tend,
}

/// The paths villagers walk: points joined by straight stretches.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    pub nodes: Vec<(f32, f32)>,
    /// Pairs of indices into `nodes`.
    pub edges: Vec<(usize, usize)>,
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
    #[serde(default)]
    spots: Vec<SpotEntry>,
    #[serde(default)]
    paths: Paths,
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
    /// A name villagers' homes refer to.
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpotEntry {
    label: String,
    at: (f32, f32),
    /// In degrees.
    #[serde(default)]
    face: f32,
    #[serde(default)]
    pose: SpotPose,
    #[serde(default)]
    via: Vec<String>,
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
        let mut labels = HashMap::new();
        for (index, entry) in self.structures.iter().enumerate() {
            if let Some(label) = &entry.label
                && labels.insert(label.clone(), index).is_some()
            {
                return Err(Problem::InvalidVillage(format!(
                    "two buildings are labeled `{label}`"
                )));
            }
        }
        let mut spots: Vec<Spot> = Vec::with_capacity(self.spots.len());
        for entry in self.spots {
            if spots.iter().any(|spot| spot.label == entry.label) {
                return Err(Problem::InvalidVillage(format!(
                    "two spots are labeled `{}`",
                    entry.label
                )));
            }
            spots.push(Spot {
                label: entry.label,
                at: entry.at,
                face: entry.face * PI / 180.0,
                pose: entry.pose,
                via: entry.via,
            });
        }
        for spot in &spots {
            if let Some(unknown) = spot
                .via
                .iter()
                .find(|label| !spots.iter().any(|other| other.label == **label))
            {
                return Err(Problem::InvalidVillage(format!(
                    "spot `{}` is reached via unknown spot `{unknown}`",
                    spot.label
                )));
            }
        }
        let paths = self.paths;
        if let Some(&(a, b)) = paths
            .edges
            .iter()
            .find(|(a, b)| *a >= paths.nodes.len() || *b >= paths.nodes.len() || a == b)
        {
            return Err(Problem::InvalidVillage(format!(
                "the path edge ({a}, {b}) does not join two different nodes"
            )));
        }
        Ok(Village {
            stalls,
            structures: placed,
            labels,
            spots,
            paths,
            lots: self.lots,
        })
    }
}
