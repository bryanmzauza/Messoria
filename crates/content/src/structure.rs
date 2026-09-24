//! Structure definitions and the structures file: things players build, such
//! as their cabin, and what comes with them, such as a bed and a chest.
//!
//! A structure is laid out in its own frame, in meters: its front faces -Z
//! and its middle is where it is placed. Its models, the boxes nobody walks
//! through and the structures built with it are placed in that frame, and
//! the whole is turned the way it faces when built.

use std::{collections::HashMap, f32::consts::PI};

use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    item::{ItemId, ItemKind, Items},
    scenery::check_models,
};

/// Refers to a structure definition within a [`Catalog`](crate::Catalog),
/// like [`ItemId`] does for items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructureId(pub(crate) u16);

#[derive(Clone, Debug, PartialEq)]
pub struct StructureDef {
    /// The id used in data files, such as `"cabin"`.
    pub key: String,
    pub name: String,
    /// The item placed to build it, if players build it themselves.
    pub built_from: Option<ItemId>,
    /// Size of the ground it stands on, along its own x and z. Nobody can
    /// dig, raise or till it, and nothing else can be built on it.
    pub size: (f32, f32),
    /// Whether building it levels the ground under it, which then becomes
    /// its floor; otherwise it needs ground that is already nearly flat.
    pub levels_ground: bool,
    /// Whether it is each player's home. Players without one are given the
    /// item it is built from.
    pub home: bool,
    pub purpose: Purpose,
    pub parts: Vec<Part>,
    pub solids: Vec<Solid>,
    /// Where warm lamps light it, in its frame (x, height, z).
    pub lamps: Vec<(f32, f32, f32)>,
    /// Structures built along with it, such as the furniture of a cabin.
    pub contains: Vec<Placement>,
}

/// What players do with a structure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum Purpose {
    #[default]
    None,
    /// Players sleep in it.
    Bed,
    /// It keeps items.
    Storage,
}

/// A model a structure is drawn with.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// Relative to the models folder.
    pub model: String,
    #[serde(default)]
    pub at: (f32, f32, f32),
    /// Turn around the vertical axis, counterclockwise seen from above. In
    /// degrees in the file, in radians once loaded.
    #[serde(default)]
    pub turn: f32,
    #[serde(default = "unscaled")]
    pub scale: f32,
}

/// A box nobody walks through, standing on the structure's ground.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solid {
    /// Middle of the box on the ground, in the structure's frame.
    pub at: (f32, f32),
    /// Size along the structure's x and z.
    pub size: (f32, f32),
    pub height: f32,
}

/// A structure built along with another, where it stands in that other's
/// frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub structure: StructureId,
    pub at: (f32, f32),
    /// In radians.
    pub turn: f32,
}

fn unscaled() -> f32 {
    1.0
}

/// The structures file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StructuresFile {
    structures: Vec<StructureEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructureEntry {
    id: String,
    name: String,
    #[serde(default)]
    built_from: Option<String>,
    size: (f32, f32),
    #[serde(default)]
    levels_ground: bool,
    #[serde(default)]
    home: bool,
    #[serde(default)]
    purpose: Purpose,
    parts: Vec<Part>,
    #[serde(default)]
    solids: Vec<Solid>,
    #[serde(default)]
    lamps: Vec<(f32, f32, f32)>,
    #[serde(default)]
    contains: Vec<PlacementEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlacementEntry {
    structure: String,
    at: (f32, f32),
    #[serde(default)]
    turn: f32,
}

/// Structures, checked.
pub(crate) struct Structures {
    pub definitions: Vec<StructureDef>,
    pub by_key: HashMap<String, StructureId>,
    pub by_item: HashMap<ItemId, StructureId>,
}

impl StructuresFile {
    pub(crate) fn resolve(self, items: &Items) -> Result<Structures, Problem> {
        let mut by_key = HashMap::new();
        for (entry, id) in self.structures.iter().zip((0..=u16::MAX).map(StructureId)) {
            if by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateStructure(entry.id.clone()));
            }
        }
        let mut structures = Structures {
            definitions: Vec::with_capacity(self.structures.len()),
            by_key,
            by_item: HashMap::new(),
        };
        for (entry, id) in self
            .structures
            .into_iter()
            .zip((0..=u16::MAX).map(StructureId))
        {
            let structure = entry.resolve(items, &structures.by_key)?;
            if let Some(item) = structure.built_from {
                if items.get(item).kind != ItemKind::Structure {
                    return Err(invalid(
                        &structure.key,
                        "it is built from an item that is not a structure",
                    ));
                }
                if structures.by_item.insert(item, id).is_some() {
                    return Err(invalid(
                        &structure.key,
                        "another structure is built from the same item",
                    ));
                }
            }
            structures.definitions.push(structure);
        }
        for structure in &structures.definitions {
            validate(structure, &structures)?;
        }
        if structures
            .definitions
            .iter()
            .filter(|structure| structure.home)
            .count()
            > 1
        {
            return Err(Problem::InvalidStructure {
                structure: "home".to_owned(),
                reason: "only one structure can be every player's home".to_owned(),
            });
        }
        if let Some((item, _)) = items
            .definitions
            .iter()
            .zip((0..=u16::MAX).map(ItemId))
            .find(|(item, id)| {
                item.kind == ItemKind::Structure && !structures.by_item.contains_key(id)
            })
        {
            return Err(Problem::UnbuiltStructureItem(item.key.clone()));
        }
        Ok(structures)
    }
}

impl StructureEntry {
    fn resolve(
        self,
        items: &Items,
        keys: &HashMap<String, StructureId>,
    ) -> Result<StructureDef, Problem> {
        let context = || format!("structure `{}`", self.id);
        let built_from = self
            .built_from
            .as_deref()
            .map(|key| items.resolve(context, key))
            .transpose()?;
        let contains = self
            .contains
            .iter()
            .map(|placement| {
                let structure = keys.get(&placement.structure).copied().ok_or_else(|| {
                    invalid(
                        &self.id,
                        &format!("it contains unknown structure `{}`", placement.structure),
                    )
                })?;
                Ok(Placement {
                    structure,
                    at: placement.at,
                    turn: placement.turn * PI / 180.0,
                })
            })
            .collect::<Result<_, Problem>>()?;
        let parts = self
            .parts
            .into_iter()
            .map(|part| Part {
                turn: part.turn * PI / 180.0,
                ..part
            })
            .collect();
        Ok(StructureDef {
            key: self.id,
            name: self.name,
            built_from,
            size: self.size,
            levels_ground: self.levels_ground,
            home: self.home,
            purpose: self.purpose,
            parts,
            solids: self.solids,
            lamps: self.lamps,
            contains,
        })
    }
}

fn invalid(structure: &str, reason: &str) -> Problem {
    Problem::InvalidStructure {
        structure: structure.to_owned(),
        reason: reason.to_owned(),
    }
}

fn validate(structure: &StructureDef, structures: &Structures) -> Result<(), Problem> {
    let problem = |reason: &str| Err(invalid(&structure.key, reason));
    let positive = |value: f32| (f32::MIN_POSITIVE..).contains(&value);
    if !positive(structure.size.0) || !positive(structure.size.1) {
        return problem("its size must be above 0 both ways");
    }
    let models: Vec<String> = structure
        .parts
        .iter()
        .map(|part| part.model.clone())
        .collect();
    if let Err(reason) = check_models(&models) {
        return problem(&reason);
    }
    if structure.parts.iter().any(|part| !positive(part.scale)) {
        return problem("its parts must be drawn at a scale above 0");
    }
    let sizes_positive =
        |solid: &Solid| positive(solid.size.0) && positive(solid.size.1) && positive(solid.height);
    if !structure.solids.iter().all(sizes_positive) {
        return problem("its solids must have a size and height above 0");
    }
    let contained =
        |placement: &Placement| &structures.definitions[usize::from(placement.structure.0)];
    if structure
        .contains
        .iter()
        .any(|placement| !contained(placement).contains.is_empty())
    {
        return problem("what it contains cannot contain structures in turn");
    }
    if structure.home {
        if structure.built_from.is_none() {
            return problem("it is every player's home, so players must be able to build it");
        }
        let has_bed = structure
            .contains
            .iter()
            .any(|placement| contained(placement).purpose == Purpose::Bed);
        if !has_bed {
            return problem("it is every player's home, so it must contain a bed");
        }
    }
    Ok(())
}
