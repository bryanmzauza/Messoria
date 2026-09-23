//! Item definitions and the items file.

use std::collections::HashMap;

use messoria_voxel::Material;
use serde::{Deserialize, Serialize};

use crate::error::Problem;

/// Refers to an item definition within a [`Catalog`](crate::Catalog).
///
/// Ids are positions in the catalog, which keeps them small on the wire.
/// They are only meaningful between peers that loaded the same content; data
/// files and saves refer to items by their text id instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemId(pub(crate) u16);

/// Everything the game knows about one kind of item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemDef {
    /// The id used in data files, such as `"wild_berries"`.
    pub key: String,
    /// Name shown to players.
    pub name: String,
    pub kind: ItemKind,
    /// Most items one inventory slot holds.
    pub max_stack: u16,
    /// Days after being acquired that the item spoils, if it is perishable.
    pub shelf_life: Option<u16>,
    /// What the item becomes when it spoils.
    pub spoils_into: Option<ItemId>,
}

/// What an item is for, which decides what using it does.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum ItemKind {
    Tool(Tool),
    /// Planted in tilled soil; the crops file says what it grows into.
    Seed,
    /// Spread on tilled soil to improve the quality of its harvests.
    Fertilizer,
    /// Eaten to restore energy.
    Food {
        energy: u16,
    },
    /// Ground carried in the inventory: digging any of these materials yields
    /// this item.
    Terrain {
        materials: Vec<Material>,
    },
    /// Anything else: kept, traded and used as an ingredient.
    Goods,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Tool {
    /// Digs and raises the ground.
    Shovel,
    /// Tills soil so crops can be planted in it.
    Hoe,
    /// Waters tilled soil for the day.
    WateringCan,
}

/// How good an item is. Only harvests vary in quality; everything else is
/// normal. Items of different quality never share a stack.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Quality {
    #[default]
    Normal,
    Silver,
    Gold,
}

/// The items file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemsFile {
    items: Vec<ItemEntry>,
    starting_inventory: Vec<(String, u16)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemEntry {
    id: String,
    name: String,
    kind: ItemKind,
    #[serde(default = "single")]
    stack: u16,
    #[serde(default)]
    shelf_life: Option<u16>,
    #[serde(default)]
    spoils_into: Option<String>,
}

fn single() -> u16 {
    1
}

/// Items with their references resolved.
pub(crate) struct Items {
    pub definitions: Vec<ItemDef>,
    pub by_key: HashMap<String, ItemId>,
    pub starting_inventory: Vec<(ItemId, u16)>,
}

impl Items {
    pub(crate) fn resolve(
        &self,
        context: impl FnOnce() -> String,
        key: &str,
    ) -> Result<ItemId, Problem> {
        self.by_key
            .get(key)
            .copied()
            .ok_or_else(|| Problem::UnknownItem {
                context: context(),
                item: key.to_owned(),
            })
    }

    pub(crate) fn get(&self, id: ItemId) -> &ItemDef {
        &self.definitions[usize::from(id.0)]
    }
}

impl ItemsFile {
    /// Resolves references between items and checks each one.
    pub(crate) fn resolve(self) -> Result<Items, Problem> {
        let mut by_key = HashMap::new();
        for (entry, id) in self.items.iter().zip((0..=u16::MAX).map(ItemId)) {
            if by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateItem(entry.id.clone()));
            }
        }
        let mut items = Items {
            definitions: Vec::with_capacity(self.items.len()),
            by_key,
            starting_inventory: Vec::new(),
        };

        for entry in self.items {
            let spoils_into = entry
                .spoils_into
                .as_deref()
                .map(|key| items.resolve(|| format!("item `{}`", entry.id), key))
                .transpose()?;
            let item = ItemDef {
                key: entry.id,
                name: entry.name,
                kind: entry.kind,
                max_stack: entry.stack,
                shelf_life: entry.shelf_life,
                spoils_into,
            };
            validate(&item, &items.by_key)?;
            items.definitions.push(item);
        }

        for (key, count) in self.starting_inventory {
            if count == 0 {
                return Err(Problem::EmptyStartingStack(key));
            }
            let id = items.resolve(|| "the starting inventory".to_owned(), &key)?;
            items.starting_inventory.push((id, count));
        }
        Ok(items)
    }
}

fn validate(item: &ItemDef, by_key: &HashMap<String, ItemId>) -> Result<(), Problem> {
    if item.max_stack == 0 {
        return Err(Problem::EmptyStack(item.key.clone()));
    }
    if matches!(item.kind, ItemKind::Tool(_)) && item.max_stack > 1 {
        return Err(Problem::StackedTool(item.key.clone()));
    }
    if item.shelf_life.is_some() && item.spoils_into.is_none() {
        return Err(Problem::SpoilsIntoNothing(item.key.clone()));
    }
    if item.spoils_into.is_some() && item.spoils_into == by_key.get(&item.key).copied() {
        return Err(Problem::SpoilsIntoItself(item.key.clone()));
    }
    Ok(())
}
