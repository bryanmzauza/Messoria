//! What characters carry: moving items between slots, using them, and
//! spoilage at dawn.
//!
//! Using an item is dispatched by what the item is. Uses that act on the
//! world are handed to the systems responsible for that part of the world as
//! Bevy messages; simple uses such as eating are handled here.

use std::{collections::HashSet, time::Duration};

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_content::{CropId, ItemKind, Tool};
use messoria_inventory::HOTBAR_SLOTS;
use messoria_shared::{
    content::Content,
    energy::Energy,
    protocol::{Asleep, Belongings, ItemAction, MoveItem, UseItem},
    tools::{self, ShovelAction},
};

use crate::{
    day_cycle::{ClockSystems, DayStarted},
    players::ControlledCharacter,
};

/// Clients pace their uses at `tools::USE_INTERVAL`; network jitter can
/// bunch them up in transit, so the server enforces a slightly shorter gap.
const MIN_USE_INTERVAL: Duration = tools::USE_INTERVAL.saturating_sub(Duration::from_millis(50));

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShovelUse>()
            .add_message::<FieldWork>()
            .add_systems(
                PreUpdate,
                (move_items, use_items.in_set(ItemUseSystems))
                    .chain()
                    .after(MessageSystems::Receive),
            )
            .add_systems(FixedUpdate, spoil_items.after(ClockSystems));
    }
}

/// Turns item uses into work for the systems that carry them out; order
/// those systems after this set.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ItemUseSystems;

/// A character using a shovel, for the terrain systems to carry out.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct ShovelUse {
    pub character: Entity,
    pub target: Vec3,
    pub action: ShovelAction,
}

/// A character working the field at `target`, for the field systems to carry
/// out. Seeds and fertilizer come from `slot`.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct FieldWork {
    pub character: Entity,
    pub slot: usize,
    pub target: Vec3,
    pub task: FieldTask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldTask {
    Till,
    Water,
    Plant(CropId),
    Fertilize,
}

/// When the player behind a connection last used an item.
#[derive(Component)]
struct LastItemUse(Duration);

fn move_items(
    content: Res<Content>,
    mut clients: Query<(&mut MessageReceiver<MoveItem>, &ControlledCharacter)>,
    mut characters: Query<&mut Belongings>,
) {
    for (mut requests, character) in &mut clients {
        let Ok(mut belongings) = characters.get_mut(character.0) else {
            continue;
        };
        for MoveItem { from, to } in requests.receive() {
            belongings
                .0
                .move_stack(&content, usize::from(from), usize::from(to));
        }
    }
}

/// Checks what is in the used slot and acts on it. Only hotbar items can be
/// used, not by sleeping characters, and not faster than the use interval.
fn use_items(
    time: Res<Time>,
    content: Res<Content>,
    mut clients: Query<(
        Entity,
        &mut MessageReceiver<UseItem>,
        &ControlledCharacter,
        Option<&LastItemUse>,
    )>,
    mut characters: Query<(&mut Belongings, &mut Energy), Without<Asleep>>,
    mut shovel_uses: MessageWriter<ShovelUse>,
    mut field_work: MessageWriter<FieldWork>,
    mut commands: Commands,
) {
    let now = time.elapsed();
    let mut used_this_frame = HashSet::new();
    for (client, mut requests, character, last_use) in &mut clients {
        for request in requests.receive() {
            let resting =
                last_use.is_some_and(|last| now.saturating_sub(last.0) < MIN_USE_INTERVAL);
            if resting || used_this_frame.contains(&client) {
                continue;
            }
            let Ok((mut belongings, mut energy)) = characters.get_mut(character.0) else {
                continue;
            };
            let slot = usize::from(request.slot);
            let Some(stack) = belongings.0.slot(slot).filter(|_| slot < HOTBAR_SLOTS) else {
                continue;
            };
            let field_task = |task| FieldWork {
                character: character.0,
                slot,
                target: request.target.unwrap_or_default(),
                task,
            };

            let used = match (&content.item(stack.item).kind, request.target) {
                (ItemKind::Tool(Tool::Shovel), Some(target)) => {
                    shovel_uses.write(ShovelUse {
                        character: character.0,
                        target,
                        action: request.action.into(),
                    });
                    true
                }
                (ItemKind::Tool(Tool::Hoe), Some(_)) => {
                    field_work.write(field_task(FieldTask::Till));
                    true
                }
                (ItemKind::Tool(Tool::WateringCan), Some(_)) => {
                    field_work.write(field_task(FieldTask::Water));
                    true
                }
                (ItemKind::Seed, Some(_)) => match content.crop_grown_from(stack.item) {
                    Some(crop) => {
                        field_work.write(field_task(FieldTask::Plant(crop)));
                        true
                    }
                    None => false,
                },
                (ItemKind::Fertilizer, Some(_)) => {
                    field_work.write(field_task(FieldTask::Fertilize));
                    true
                }
                (
                    &ItemKind::Food {
                        energy: nourishment,
                    },
                    _,
                ) if request.action == ItemAction::Primary && *energy < Energy::FULL => {
                    belongings.0.take_one(slot);
                    energy.gain(nourishment);
                    true
                }
                _ => false,
            };
            if used {
                used_this_frame.insert(client);
                commands.entity(client).insert(LastItemUse(now));
            }
        }
    }
}

fn spoil_items(
    content: Res<Content>,
    mut days: MessageReader<DayStarted>,
    mut characters: Query<&mut Belongings>,
) {
    for day in days.read() {
        for mut belongings in &mut characters {
            // Only report a change when something spoiled, so unchanged
            // inventories are not sent to clients again.
            if belongings
                .bypass_change_detection()
                .0
                .spoil(&content, day.0)
                > 0
            {
                belongings.set_changed();
            }
        }
    }
}
