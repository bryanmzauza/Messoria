//! What other players are called on screen: the name they introduced
//! themselves with, or one made from their id until they have.

use bevy::{ecs::system::SystemParam, prelude::*};
use lightyear::prelude::PeerId;
use messoria_shared::{
    players::name_of,
    protocol::{PlayerId, PlayerName},
};

#[derive(SystemParam)]
pub(crate) struct Names<'w, 's> {
    players: Query<'w, 's, (&'static PlayerId, &'static PlayerName)>,
}

impl Names<'_, '_> {
    pub(crate) fn of(&self, peer: PeerId) -> String {
        self.players
            .iter()
            .find(|(id, _)| id.0 == peer)
            .map_or_else(|| name_of(peer), |(_, name)| name.0.clone())
    }
}
