//! A player's file: where they are, what they have and who their friends
//! in the village are.

use glam::Vec3;
use messoria_content::{Catalog, VillagerId};
use messoria_economy::{SalesLedger, Wallet};
use messoria_inventory::Inventory;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    files::{Resolver, to_ron, unreadable, version_of},
    stacks::{self, SlotEntry},
};

/// Version of the format this game writes. Version 1 files had no
/// friendships; they are read as having none.
const VERSION: u32 = 2;
const WITHOUT_FRIENDSHIPS: u32 = 1;

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
    pub friendships: Vec<FriendshipState>,
}

/// How a player stands with one villager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendshipState {
    pub villager: VillagerId,
    pub points: u16,
    /// The last day they talked, and the last day the player gave a gift.
    pub talked_on: Option<u32>,
    pub gave_on: Option<u32>,
    /// Hearts at which the villager's gift was already given.
    pub received: Vec<u16>,
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
    #[serde(default)]
    friendships: Vec<FriendshipEntry>,
}

#[derive(Serialize, Deserialize)]
struct FriendshipEntry {
    villager: String,
    points: u16,
    talked_on: Option<u32>,
    gave_on: Option<u32>,
    received: Vec<u16>,
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
        let friendships = self
            .friendships
            .iter()
            .map(|friendship| FriendshipEntry {
                villager: catalog.villager(friendship.villager).key.clone(),
                points: friendship.points,
                talked_on: friendship.talked_on,
                gave_on: friendship.gave_on,
                received: friendship.received.clone(),
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
            friendships,
        })
    }

    pub(crate) fn from_ron(text: &str, mut resolver: Resolver<'_>) -> Result<Self, Problem> {
        let file: PlayerFile = match version_of(text)? {
            VERSION | WITHOUT_FRIENDSHIPS => ron::from_str(text)?,
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

        let friendships = file
            .friendships
            .into_iter()
            .filter_map(|entry| {
                let villager = resolver.catalog.villager_id(&entry.villager);
                if villager.is_none() {
                    resolver.note(&format!(
                        "villager `{}` no longer lives here; the friendship was forgotten",
                        entry.villager
                    ));
                }
                Some(FriendshipState {
                    villager: villager?,
                    points: entry.points,
                    talked_on: entry.talked_on,
                    gave_on: entry.gave_on,
                    received: entry.received,
                })
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
            friendships,
        })
    }
}
