//! The valley the world grew from its seed, and which of its columns this
//! app has loaded.
//!
//! The server knows the valley from the start. A client learns it when the
//! server tells it the world's seed, before any terrain, and works out the
//! rest itself: the shape of the land it draws past the terrain it was
//! sent, and the scenery on every column it has.

use bevy::{ecs::message::Message, prelude::*};
pub use messoria_worldgen::{Landscape, PlacedProp, PropKey};

/// The valley, once this app knows the world's seed.
#[derive(Resource, Deref)]
pub struct Valley(pub Landscape);

/// A chunk column's terrain was loaded: generated on a server, received on
/// a client. What stands on it can now be worked out.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnLoaded(pub IVec2);

/// A chunk column's terrain was unloaded, and what stands on it with it.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnUnloaded(pub IVec2);

pub(crate) struct ValleyPlugin;

impl Plugin for ValleyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ColumnLoaded>()
            .add_message::<ColumnUnloaded>();
    }
}
