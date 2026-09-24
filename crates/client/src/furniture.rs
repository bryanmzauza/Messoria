//! Furniture: going to bed and getting up, and opening chests.
//!
//! `F` on a bed goes to sleep in it, and `F` again gets up. `F` on a chest
//! opens its window, which closes when the character walks away. The server
//! decides whether the character may sleep, and says why not.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::Purpose;
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{ActionChannel, Asleep, PlayerInput, Position, SleepRequest, Structure},
    tools,
};

use crate::{
    actions::{Aim, AimSystems, INTERACT_KEY},
    camera::{LookSystems, View},
    panels::OpenPanel,
    shops::ShopSystems,
};

pub(crate) struct FurniturePlugin;

impl Plugin for FurniturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                use_furniture
                    .in_set(FurnitureSystems)
                    .after(AimSystems)
                    .after(ShopSystems)
                    .before(LookSystems),
                close_far_chest,
            ),
        );
    }
}

/// Answers the interact key at beds and chests; harvesting runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FurnitureSystems;

/// Consumes the key press it acts on, so it does not also harvest.
fn use_furniture(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    view: Res<View>,
    aim: Res<Aim>,
    content: Res<Content>,
    structures: Query<&Structure>,
    player: Query<Has<Asleep>, With<InputMarker<PlayerInput>>>,
    mut panel: ResMut<OpenPanel>,
    mut sleep: Query<&mut MessageSender<SleepRequest>, With<Client>>,
) {
    if !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    if player.single().unwrap_or(false) {
        if let Ok(mut sender) = sleep.single_mut() {
            sender.send::<ActionChannel>(SleepRequest::Wake);
        }
        keys.clear_just_pressed(INTERACT_KEY);
        return;
    }
    let Some((thing, _)) = aim.thing.filter(|_| view.captured) else {
        return;
    };
    let Ok(structure) = structures.get(thing) else {
        return;
    };
    match content.structure(structure.kind).purpose {
        Purpose::Bed => {
            if let Ok(mut sender) = sleep.single_mut() {
                sender.send::<ActionChannel>(SleepRequest::Sleep);
            }
        }
        Purpose::Storage => *panel = OpenPanel::Chest(thing),
        Purpose::None => return,
    }
    keys.clear_just_pressed(INTERACT_KEY);
}

fn close_far_chest(
    mut panel: ResMut<OpenPanel>,
    structures: Query<&Structure>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
) {
    let OpenPanel::Chest(chest) = *panel else {
        return;
    };
    let within_reach = player
        .single()
        .ok()
        .zip(structures.get(chest).ok())
        .is_some_and(|(feet, chest)| {
            tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, chest.position)
        });
    if !within_reach {
        *panel = OpenPanel::None;
    }
}
