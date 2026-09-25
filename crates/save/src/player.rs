//! A player's file: where they are and what they have.

use glam::Vec3;
use messoria_content::Catalog;
use messoria_economy::{SalesLedger, Wallet};
use messoria_inventory::Inventory;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
    stacks::{self, SlotEntry},
};

/// Version of the format this game writes.
const VERSION: u32 = 1;

/// Everything about a player that outlasts their connection.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    /// Where their character's feet were.
    pub position: Vec3,
    /// Which way the character faced.
    pub heading: f32,
    pub energy: u16,
    pub inventory: Inventory,
    pub money: Wallet,
    pub sold_today: SalesLedger,
    /// The day the state was saved on. Days that passed since then happened
    /// while the player was away.
    pub saved_on: u32,
}

/// The player file as written.
#[derive(Serialize, Deserialize)]
struct PlayerFile {
    version: u32,
    position: Vec3,
    heading: f32,
    energy: u16,
    money: u32,
    saved_on: u32,
    /// Only the slots that hold something.
    inventory: Vec<SlotEntry>,
    sold_today: Vec<SaleEntry>,
}

#[derive(Serialize, Deserialize)]
struct SaleEntry {
    shop: String,
    item: String,
    count: u16,
}

impl PlayerState {
    pub(crate) fn to_ron(&self, catalog: &Catalog) -> Result<String, Problem> {
        let inventory = stacks::to_entries(catalog, &self.inventory);
        let sold_today = self
            .sold_today
            .entries()
            .map(|(shop, item, count)| SaleEntry {
                shop: catalog.shop(shop).key.clone(),
                item: catalog.item(item).key.clone(),
                count,
            })
            .collect();
        to_ron(&PlayerFile {
            version: VERSION,
            position: self.position,
            heading: self.heading,
            energy: self.energy,
            money: self.money.coins(),
            saved_on: self.saved_on,
            inventory,
            sold_today,
        })
    }

    pub(crate) fn from_ron(text: &str, mut resolver: Resolver<'_>) -> Result<Self, Problem> {
        let file: PlayerFile = match version_of(text)? {
            VERSION => ron::from_str(text)?,
            // Files from older versions of the format are read and upgraded here.
            found => return Err(unreadable(found, VERSION)),
        };

        let inventory = stacks::from_entries(file.inventory, &mut resolver)?;

        let sold_today = file
            .sold_today
            .iter()
            .filter_map(|sale| {
                let shop = resolver.catalog.shop_id(&sale.shop);
                if shop.is_none() {
                    resolver.note(&format!("shop `{}` no longer exists", sale.shop));
                }
                Some((shop?, resolver.item(&sale.item)?, sale.count))
            })
            .collect();

        Ok(Self {
            position: file.position,
            heading: file.heading,
            energy: file.energy,
            inventory,
            money: Wallet::with(file.money),
            sold_today,
            saved_on: file.saved_on,
        })
    }
}
