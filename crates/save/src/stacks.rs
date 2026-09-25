//! Inventories as saved: the slots that hold something, with items by their
//! text id.

use messoria_content::{Catalog, Quality};
use messoria_inventory::{Inventory, SLOTS, Stack};
use serde::{Deserialize, Serialize};

use crate::{error::Problem, files::Resolver};

#[derive(Serialize, Deserialize)]
pub(crate) struct SlotEntry {
    slot: u8,
    item: String,
    quality: Quality,
    count: u16,
    spoils_on: Option<u32>,
}

pub(crate) fn to_entries(catalog: &Catalog, inventory: &Inventory) -> Vec<SlotEntry> {
    inventory
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
        .collect()
}

/// The inventory `entries` describe. Items the content no longer defines are
/// left out, and noted.
pub(crate) fn from_entries(
    entries: Vec<SlotEntry>,
    resolver: &mut Resolver<'_>,
) -> Result<Inventory, Problem> {
    let mut slots = [None; SLOTS];
    for entry in entries {
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
    Ok(Inventory::from_slots(slots))
}
