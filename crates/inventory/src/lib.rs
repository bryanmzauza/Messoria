//! The items a character carries: slots, stacks and spoilage.
//!
//! An inventory has a hotbar, whose selected slot decides what the character
//! holds, followed by a backpack. Slots hold stacks of one item of one
//! quality each, up to that item's stack size. Perishable stacks remember the
//! day they spoil on.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

use messoria_content::{Catalog, ItemId, Quality};
use serde::{Deserialize, Serialize};

/// Slots in the hotbar, which come first.
pub const HOTBAR_SLOTS: usize = 10;
/// Slots in the backpack, after the hotbar.
pub const BACKPACK_SLOTS: usize = 20;
/// All slots.
pub const SLOTS: usize = HOTBAR_SLOTS + BACKPACK_SLOTS;

/// Some number of one item in one slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    pub item: ItemId,
    pub quality: Quality,
    pub count: u16,
    /// Day the stack spoils on, for perishable items.
    pub spoils_on: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    slots: [Option<Stack>; SLOTS],
}

impl Inventory {
    /// An inventory holding exactly `slots`, such as one read back from a
    /// save.
    pub fn from_slots(slots: [Option<Stack>; SLOTS]) -> Self {
        Self { slots }
    }

    pub fn slots(&self) -> &[Option<Stack>; SLOTS] {
        &self.slots
    }

    pub fn slot(&self, index: usize) -> Option<&Stack> {
        self.slots.get(index)?.as_ref()
    }

    /// How many of `item` the inventory holds.
    pub fn count(&self, item: ItemId) -> u32 {
        self.stacks_of(item)
            .map(|stack| u32::from(stack.count))
            .sum()
    }

    /// How many more of `item` in `quality` fit.
    pub fn room_for(&self, catalog: &Catalog, item: ItemId, quality: Quality) -> u32 {
        let max = catalog.item(item).max_stack;
        self.slots
            .iter()
            .map(|slot| match slot {
                None => u32::from(max),
                Some(stack) if stack.item == item && stack.quality == quality => {
                    u32::from(max - stack.count)
                }
                Some(_) => 0,
            })
            .sum()
    }

    /// Adds `count` of `item` in `quality`, acquired on day `today`: first
    /// onto stacks of the same item and quality, then into empty slots,
    /// hotbar first. Returns how many did not fit.
    pub fn add(
        &mut self,
        catalog: &Catalog,
        item: ItemId,
        quality: Quality,
        count: u16,
        today: u32,
    ) -> u16 {
        let definition = catalog.item(item);
        let spoils_on = definition.shelf_life.map(|days| today + u32::from(days));
        let mut left = count;

        for stack in self.slots.iter_mut().flatten() {
            if left == 0 {
                break;
            }
            if stack.item == item && stack.quality == quality {
                let moved = left.min(definition.max_stack - stack.count);
                stack.spoils_on = blend_freshness(stack.spoils_on, stack.count, spoils_on, moved);
                stack.count += moved;
                left -= moved;
            }
        }
        for slot in self.slots.iter_mut().filter(|slot| slot.is_none()) {
            if left == 0 {
                break;
            }
            let moved = left.min(definition.max_stack);
            *slot = Some(Stack {
                item,
                quality,
                count: moved,
                spoils_on,
            });
            left -= moved;
        }
        left
    }

    /// Removes `count` of `item` of any quality, the lowest quality and then
    /// those closest to spoiling first. Removes nothing and returns `false` if
    /// there are not enough.
    pub fn remove(&mut self, item: ItemId, count: u16) -> bool {
        if self.count(item) < u32::from(count) {
            return false;
        }
        let mut order: Vec<usize> = (0..SLOTS)
            .filter(|&index| self.slots[index].is_some_and(|stack| stack.item == item))
            .collect();
        order.sort_by_key(|&index| {
            self.slots[index].map(|stack| (stack.quality, stack.spoils_on.unwrap_or(u32::MAX)))
        });

        let mut left = count;
        for index in order {
            let slot = &mut self.slots[index];
            if let Some(stack) = slot {
                let taken = left.min(stack.count);
                stack.count -= taken;
                left -= taken;
                if stack.count == 0 {
                    *slot = None;
                }
            }
            if left == 0 {
                break;
            }
        }
        true
    }

    /// Removes one item from `slot`, returning what it was.
    pub fn take_one(&mut self, slot: usize) -> Option<ItemId> {
        self.take(slot, 1).map(|taken| taken.item)
    }

    /// Removes up to `count` items from `slot`, returning what was removed,
    /// or `None` if the slot is empty or `count` is 0.
    pub fn take(&mut self, slot: usize, count: u16) -> Option<Stack> {
        let entry = self.slots.get_mut(slot)?;
        let stack = entry.as_mut()?;
        let taken = count.min(stack.count);
        if taken == 0 {
            return None;
        }
        stack.count -= taken;
        let removed = Stack {
            count: taken,
            ..*stack
        };
        if stack.count == 0 {
            *entry = None;
        }
        Some(removed)
    }

    /// Moves the stack in slot `from` onto slot `to`. Stacks of the same item
    /// and quality merge as far as they fit; anything else swaps places.
    /// Returns `false` if either slot does not exist.
    pub fn move_stack(&mut self, catalog: &Catalog, from: usize, to: usize) -> bool {
        if from >= SLOTS || to >= SLOTS {
            return false;
        }
        // Two different slots, both within the inventory.
        if let Ok([source, target]) = self.slots.get_disjoint_mut([from, to]) {
            put(catalog, source, target);
        }
        true
    }

    /// Moves the stack in slot `from` of `source` onto slot `to` of
    /// `target`, merging or swapping as [`Self::move_stack`] does. Returns
    /// `false` if either slot does not exist.
    pub fn move_between(
        catalog: &Catalog,
        source: &mut Self,
        from: usize,
        target: &mut Self,
        to: usize,
    ) -> bool {
        if from >= SLOTS || to >= SLOTS {
            return false;
        }
        put(catalog, &mut source.slots[from], &mut target.slots[to]);
        true
    }

    /// Turns every stack that has spoiled by day `today` into what it spoils
    /// into, as many as fit in the slot. Returns how many stacks spoiled.
    pub fn spoil(&mut self, catalog: &Catalog, today: u32) -> usize {
        let mut spoiled = 0;
        for slot in &mut self.slots {
            let Some(stack) = *slot else {
                continue;
            };
            if stack.spoils_on.is_none_or(|day| day > today) {
                continue;
            }
            // Loaded content guarantees that perishable items say what they
            // spoil into.
            let Some(remains) = catalog.item(stack.item).spoils_into else {
                continue;
            };
            let definition = catalog.item(remains);
            *slot = Some(Stack {
                item: remains,
                quality: Quality::Normal,
                count: stack.count.min(definition.max_stack),
                spoils_on: definition.shelf_life.map(|days| today + u32::from(days)),
            });
            spoiled += 1;
        }
        spoiled
    }

    fn stacks_of(&self, item: ItemId) -> impl Iterator<Item = &Stack> {
        self.slots
            .iter()
            .flatten()
            .filter(move |stack| stack.item == item)
    }
}

/// The spoil day of a stack made by combining two. Freshness is averaged by
/// count, so merging neither saves old items nor ruins fresh ones.
/// Puts the stack in `source` onto `target`: merged as far as it fits if both
/// hold the same item and quality, swapped otherwise.
fn put(catalog: &Catalog, source: &mut Option<Stack>, target: &mut Option<Stack>) {
    match (*source, *target) {
        (Some(moving), Some(staying))
            if moving.item == staying.item && moving.quality == staying.quality =>
        {
            let room = catalog.item(staying.item).max_stack - staying.count;
            let moved = room.min(moving.count);
            *target = Some(Stack {
                count: staying.count + moved,
                spoils_on: blend_freshness(
                    staying.spoils_on,
                    staying.count,
                    moving.spoils_on,
                    moved,
                ),
                ..staying
            });
            *source = (moving.count > moved).then_some(Stack {
                count: moving.count - moved,
                ..moving
            });
        }
        _ => std::mem::swap(source, target),
    }
}

fn blend_freshness(a: Option<u32>, a_count: u16, b: Option<u32>, b_count: u16) -> Option<u32> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let total = u64::from(a_count) + u64::from(b_count);
            if total == 0 {
                return Some(a.min(b));
            }
            let weighted = u64::from(a) * u64::from(a_count) + u64::from(b) * u64::from(b_count);
            Some(u32::try_from(weighted / total).expect("an average of two u32 values"))
        }
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use messoria_content::Sources;

    use super::*;

    const ITEMS: &str = r#"(
        items: [
            (id: "shovel", name: "Shovel", kind: Tool(Shovel)),
            (id: "soil", name: "Soil", kind: Terrain(materials: [Grass, Soil, Stone, Sand]), stack: 99),
            (id: "berries", name: "Berries", kind: Food(energy: 5), stack: 20, shelf_life: 3, spoils_into: "compost"),
            (id: "compost", name: "Compost", kind: Goods, stack: 99),
        ],
        starting_inventory: [],
        starting_money: 0,
    )"#;
    const CROPS: &str = "(crops: [])";
    /// Market rules and no shops.
    /// No scenery, and a palette with only the ground colors.
    const SCENERY: &str = "(props: [], cover: [])";
    const PALETTE: &str = "(colors: {\"terrain_grass\": (0.3, 0.5, 0.2), \"terrain_soil\": (0.4, 0.3, 0.2), \"terrain_stone\": (0.5, 0.5, 0.5), \"terrain_sand\": (0.8, 0.7, 0.5)})";
    const SHOPS: &str = "(market: (off_season_markup: 1.5, halves_after: 100.0, daily_recovery: 0.25, silver_bonus: 1.25, gold_bonus: 1.5), shops: [])";

    struct Fixture {
        catalog: Catalog,
        shovel: ItemId,
        soil: ItemId,
        berries: ItemId,
        compost: ItemId,
    }

    fn fixture() -> Fixture {
        let catalog = Catalog::from_sources(Sources {
            items: ITEMS,
            crops: CROPS,
            shops: SHOPS,
            scenery: SCENERY,
            palette: PALETTE,
            structures: "(structures: [])",
            characters: r#"(
                pixel: 0.1, texels: 1, skin_size: (4, 2),
                limbs: (body: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]), head: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]), right_arm: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]), left_arm: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]), right_leg: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]), left_leg: (joint: (0.0, 0.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))])),
                grip: (at: (0.0, 0.0, 0.0), turn: (0.0, 0.0, 0.0), tool_size: 1.0, item_size: 0.5),
                wardrobe: (bodies: ["b.png"], outfits: ["o.png"], hair: ["h.png"], hair_colors: [(0.0, 0.0, 0.0)]),
            )"#,
            village: "(stalls: [])",
        })
        .unwrap();
        let id = |key| catalog.id(key).unwrap();
        Fixture {
            shovel: id("shovel"),
            soil: id("soil"),
            berries: id("berries"),
            compost: id("compost"),
            catalog,
        }
    }

    #[test]
    fn items_fill_existing_stacks_before_empty_slots() {
        let f = fixture();
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(&f.catalog, f.soil, Quality::Normal, 60, 0), 0);
        assert_eq!(inventory.add(&f.catalog, f.soil, Quality::Normal, 60, 0), 0);

        assert_eq!(inventory.slot(0).unwrap().count, 99);
        assert_eq!(inventory.slot(1).unwrap().count, 21);
        assert_eq!(inventory.count(f.soil), 120);
    }

    #[test]
    fn items_that_do_not_fit_are_returned() {
        let f = fixture();
        let mut inventory = Inventory::default();
        let capacity = u16::try_from(SLOTS * 99).unwrap();
        assert_eq!(
            inventory.add(&f.catalog, f.soil, Quality::Normal, capacity + 5, 0),
            5
        );
        assert_eq!(inventory.room_for(&f.catalog, f.soil, Quality::Normal), 0);
        assert_eq!(
            inventory.room_for(&f.catalog, f.berries, Quality::Normal),
            0
        );
    }

    #[test]
    fn tools_do_not_stack() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.shovel, Quality::Normal, 2, 0);
        assert_eq!(inventory.slot(0).unwrap().count, 1);
        assert_eq!(inventory.slot(1).unwrap().count, 1);
    }

    #[test]
    fn removing_takes_the_oldest_first_and_is_all_or_nothing() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.soil, Quality::Normal, 1, 0);
        inventory.add(&f.catalog, f.berries, Quality::Normal, 20, 0);
        inventory.add(&f.catalog, f.berries, Quality::Normal, 5, 2);
        // Swap the fresh stack in front of the old one, so that slot order
        // and age disagree.
        inventory.move_stack(&f.catalog, 2, 0);

        assert!(!inventory.remove(f.berries, 26));
        assert_eq!(inventory.count(f.berries), 25);

        assert!(inventory.remove(f.berries, 22));
        assert!(
            inventory.slot(1).is_none(),
            "the older stack should go first"
        );
        assert_eq!(inventory.slot(0).unwrap().count, 3);
    }

    #[test]
    fn taking_the_last_item_empties_the_slot() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.berries, Quality::Normal, 1, 0);
        assert_eq!(inventory.take_one(0), Some(f.berries));
        assert_eq!(inventory.slot(0), None);
        assert_eq!(inventory.take_one(0), None);
        assert_eq!(inventory.take_one(SLOTS), None);
    }

    #[test]
    fn moving_merges_the_same_item_and_swaps_different_ones() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.soil, Quality::Normal, 90, 0);
        inventory.add(&f.catalog, f.shovel, Quality::Normal, 1, 0);
        inventory.move_stack(&f.catalog, 0, 2);
        inventory.add(&f.catalog, f.soil, Quality::Normal, 20, 0);

        assert!(inventory.move_stack(&f.catalog, 0, 2));
        assert_eq!(inventory.slot(2).unwrap().count, 99);
        assert_eq!(inventory.slot(0).unwrap().count, 11);

        assert!(inventory.move_stack(&f.catalog, 1, 2));
        assert_eq!(inventory.slot(2).unwrap().item, f.shovel);
        assert_eq!(inventory.slot(1).unwrap().item, f.soil);

        assert!(!inventory.move_stack(&f.catalog, 0, SLOTS));
    }

    #[test]
    fn stacks_move_between_inventories_as_within_one() {
        let f = fixture();
        let (mut carried, mut stored) = (Inventory::default(), Inventory::default());
        carried.add(&f.catalog, f.soil, Quality::Normal, 30, 0);
        stored.add(&f.catalog, f.soil, Quality::Normal, 80, 0);
        stored.add(&f.catalog, f.shovel, Quality::Normal, 1, 0);

        assert!(Inventory::move_between(
            &f.catalog,
            &mut carried,
            0,
            &mut stored,
            0
        ));
        assert_eq!(stored.slot(0).unwrap().count, 99);
        assert_eq!(carried.slot(0).unwrap().count, 11);

        assert!(Inventory::move_between(
            &f.catalog,
            &mut carried,
            0,
            &mut stored,
            1
        ));
        assert_eq!(carried.slot(0).unwrap().item, f.shovel);
        assert_eq!(stored.slot(1).unwrap().count, 11);

        assert!(Inventory::move_between(
            &f.catalog,
            &mut carried,
            0,
            &mut stored,
            5
        ));
        assert!(carried.slot(0).is_none());
        assert!(!Inventory::move_between(
            &f.catalog,
            &mut carried,
            SLOTS,
            &mut stored,
            0
        ));
    }

    #[test]
    fn spoiled_stacks_turn_into_compost() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.berries, Quality::Normal, 7, 0);
        inventory.add(&f.catalog, f.soil, Quality::Normal, 3, 0);

        assert_eq!(inventory.spoil(&f.catalog, 2), 0);
        assert_eq!(inventory.spoil(&f.catalog, 3), 1);

        let spoiled = inventory.slot(0).unwrap();
        assert_eq!(
            (spoiled.item, spoiled.count, spoiled.spoils_on),
            (f.compost, 7, None)
        );
        assert_eq!(inventory.count(f.soil), 3);
    }

    #[test]
    fn different_qualities_keep_separate_stacks() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.berries, Quality::Normal, 2, 0);
        inventory.add(&f.catalog, f.berries, Quality::Gold, 2, 0);
        assert_eq!(inventory.slot(1).unwrap().quality, Quality::Gold);

        inventory.move_stack(&f.catalog, 1, 0);
        assert_eq!(
            inventory.slot(0).unwrap().quality,
            Quality::Gold,
            "unlike stacks swap"
        );

        assert!(inventory.remove(f.berries, 3));
        assert_eq!(
            inventory.slot(0).unwrap().quality,
            Quality::Gold,
            "normal ones go first"
        );
    }

    #[test]
    fn merging_perishables_averages_their_freshness() {
        let f = fixture();
        let mut inventory = Inventory::default();
        inventory.add(&f.catalog, f.berries, Quality::Normal, 3, 0);
        inventory.add(&f.catalog, f.berries, Quality::Normal, 1, 4);

        let stack = inventory.slot(0).unwrap();
        assert_eq!(stack.count, 4);
        assert_eq!(stack.spoils_on, Some(4), "(3 × day 3 + 1 × day 7) / 4");
    }
}
