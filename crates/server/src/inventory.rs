//! What characters carry: moving items between slots, using them, and
//! spoilage at dawn.
//!
//! Using an item is dispatched by what the item is. Tools that act on the
//! world are handed to the systems responsible for that part of the world as
//! Bevy messages; simple uses such as eating are handled here.

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_content::{ItemKind, Tool};
use messoria_inventory::HOTBAR_SLOTS;
use messoria_shared::{
    content::Content,
    energy::Energy,
    protocol::{Asleep, Belongings, ItemAction, MoveItem, UseItem},
    shovel::ShovelAction,
};

use crate::{
    day_cycle::{ClockSystems, DayStarted},
    players::ControlledCharacter,
};

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShovelUse>()
            .add_systems(
                PreUpdate,
                (move_items, use_items)
                    .chain()
                    .after(MessageSystems::Receive),
            )
            .add_systems(FixedUpdate, spoil_items.after(ClockSystems));
    }
}

/// A character using a shovel, for the terrain systems to carry out.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct ShovelUse {
    /// The connection that asked.
    pub client: Entity,
    pub character: Entity,
    pub target: Vec3,
    pub action: ShovelAction,
}

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
/// used, and not by sleeping characters.
pub(crate) fn use_items(
    content: Res<Content>,
    mut clients: Query<(Entity, &mut MessageReceiver<UseItem>, &ControlledCharacter)>,
    mut characters: Query<(&mut Belongings, &mut Energy), Without<Asleep>>,
    mut shovel_uses: MessageWriter<ShovelUse>,
) {
    for (client, mut requests, character) in &mut clients {
        for request in requests.receive() {
            let Ok((mut belongings, mut energy)) = characters.get_mut(character.0) else {
                continue;
            };
            let slot = usize::from(request.slot);
            let Some(stack) = belongings.0.slot(slot).filter(|_| slot < HOTBAR_SLOTS) else {
                continue;
            };
            match (&content.item(stack.item).kind, request.target) {
                (ItemKind::Tool(Tool::Shovel), Some(target)) => {
                    shovel_uses.write(ShovelUse {
                        client,
                        character: character.0,
                        target,
                        action: request.action.into(),
                    });
                }
                (
                    &ItemKind::Food {
                        energy: nourishment,
                    },
                    _,
                ) if request.action == ItemAction::Primary && *energy < Energy::FULL => {
                    belongings.0.take_one(slot);
                    energy.gain(nourishment);
                }
                _ => {}
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
