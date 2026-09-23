//! Item definitions.

use messoria_voxel::Material;
use serde::{Deserialize, Serialize};

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
}
