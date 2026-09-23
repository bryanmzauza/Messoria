//! Telling players what became of their actions.
//!
//! Systems that carry out actions write a [`Tell`] when they refuse one for a
//! reason the player can do something about, and a [`Show`] when something
//! happens that everyone nearby should hear and see. This module sends them
//! over the network.

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_shared::protocol::{FeedbackChannel, Happened, Happening, Notice};

pub(crate) struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Tell>().add_message::<Show>().add_systems(
            PostUpdate,
            (send_notices, send_happenings).before(MessageSystems::Send),
        );
    }
}

/// Tells the player controlling `character` why something they asked for
/// did not happen.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Tell {
    pub character: Entity,
    pub notice: Notice,
}

/// Something happened at a place, for every player to hear and see.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Show(pub Happening);

impl Show {
    pub(crate) fn at(what: Happened, at: Vec3) -> Self {
        Self(Happening { what, at })
    }
}

fn send_notices(
    mut tells: MessageReader<Tell>,
    owners: Query<&ControlledBy>,
    mut senders: Query<&mut MessageSender<Notice>>,
) {
    for tell in tells.read() {
        let Ok(owner) = owners.get(tell.character) else {
            continue;
        };
        if let Ok(mut sender) = senders.get_mut(owner.owner) {
            sender.send::<FeedbackChannel>(tell.notice);
        }
    }
}

fn send_happenings(
    mut shows: MessageReader<Show>,
    mut senders: Query<&mut MessageSender<Happening>>,
) {
    for Show(happening) in shows.read() {
        for mut sender in &mut senders {
            sender.send::<FeedbackChannel>(*happening);
        }
    }
}
