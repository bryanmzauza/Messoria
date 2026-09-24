//! Chests: moving stacks between a character and the chest they stand at.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::Purpose;
use messoria_inventory::Inventory;
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{Asleep, Belongings, MoveStored, Place, Position, Stored},
    tools,
};

use crate::{building::Built, players::ControlledCharacter};

/// How far from where a client says a chest stands the chest may be, which
/// absorbs rounding on the way.
const CHEST_SLACK: f32 = 0.1;

pub(crate) struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, move_stored.after(MessageSystems::Receive));
    }
}

fn move_stored(
    content: Res<Content>,
    built: Built,
    mut clients: Query<(&mut MessageReceiver<MoveStored>, &ControlledCharacter)>,
    mut characters: Query<(&Position, &mut Belongings), Without<Asleep>>,
    mut chests: Query<&mut Stored>,
) {
    for (mut requests, character) in &mut clients {
        for MoveStored { chest, from, to } in requests.receive() {
            let Ok((feet, mut belongings)) = characters.get_mut(character.0) else {
                continue;
            };
            let Some((entity, structure)) = built.nearest(Purpose::Storage, chest, CHEST_SLACK)
            else {
                continue;
            };
            if !tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, structure.position) {
                debug!("rejected a move at the chest at {chest}: out of reach");
                continue;
            }
            let Ok(mut stored) = chests.get_mut(entity) else {
                continue;
            };
            // Copies, so that a move that does nothing changes nothing.
            let (mut carried, mut kept) = (belongings.0.clone(), stored.0.clone());
            let moved = match (from, to) {
                (Place::Carried(a), Place::Carried(b)) => {
                    carried.move_stack(&content, usize::from(a), usize::from(b))
                }
                (Place::Stored(a), Place::Stored(b)) => {
                    kept.move_stack(&content, usize::from(a), usize::from(b))
                }
                (Place::Carried(a), Place::Stored(b)) => Inventory::move_between(
                    &content,
                    &mut carried,
                    usize::from(a),
                    &mut kept,
                    usize::from(b),
                ),
                (Place::Stored(a), Place::Carried(b)) => Inventory::move_between(
                    &content,
                    &mut kept,
                    usize::from(a),
                    &mut carried,
                    usize::from(b),
                ),
            };
            if moved {
                belongings.set_if_neq(Belongings(carried));
                stored.set_if_neq(Stored(kept));
            }
        }
    }
}
