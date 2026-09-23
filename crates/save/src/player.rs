//! A player's file: where they are and what they have.

use glam::Vec3;
use messoria_content::{Catalog, Quality};
use messoria_economy::{SalesLedger, Wallet};
use messoria_inventory::{Inventory, SLOTS, Stack};
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
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
struct SlotEntry {
    slot: u8,
    item: String,
    quality: Quality,
    count: u16,
    spoils_on: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct SaleEntry {
    shop: String,
    item: String,
    count: u16,
}

impl PlayerState {
    pub(crate) fn to_ron(&self, catalog: &Catalog) -> Result<String, Problem> {
        let inventory = self
            .inventory
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| {
                let stack = stack.as_ref()?;
                Some(SlotEntry {
                    slot: u8::try_from(slot).expect("slot indices fit in u8"),
                    item: catalog.item(stack.item).key.clone(),
                    quality: stack.quality,
                    count: stack.count,
                    spoils_on: stack.spoils_on,
                })
            })
            .collect();
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

        let mut slots = [None; SLOTS];
        for entry in file.inventory {
            let slot = slots
                .get_mut(usize::from(entry.slot))
                .ok_or_else(|| Problem::OutOfRange(format!("inventory slot {}", entry.slot)))?;
            let Some(item) = resolver.item(&entry.item) else {
                continue;
            };
            let max_stack = resolver.catalog.item(item).max_stack;
            if entry.count == 0 || entry.count > max_stack {
                return Err(Problem::OutOfRange(format!(
                    "a stack of {} `{}`",
                    entry.count, entry.item
                )));
            }
            *slot = Some(Stack {
                item,
                quality: entry.quality,
                count: entry.count,
                spoils_on: entry.spoils_on,
            });
        }

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
            inventory: Inventory::from_slots(slots),
            money: Wallet::with(file.money),
            sold_today,
            saved_on: file.saved_on,
        })
    }
}
