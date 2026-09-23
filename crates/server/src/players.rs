//! Player characters: one per connected client, gone when it disconnects.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
use messoria_shared::protocol::{Heading, PlayerId, Position, Velocity};

/// Distance from the world origin at which players appear.
const SPAWN_RADIUS: f32 = 3.0;

pub(crate) struct PlayersPlugin;

impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_player);
    }
}

/// Spawns a character once the connection is confirmed, not when the link is
/// first created: the server may still reject a connection attempt.
fn spawn_player(
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    let Ok(&RemoteId(peer)) = clients.get(trigger.entity) else {
        return;
    };

    commands.spawn((
        Name::new(format!("Player {peer:?}")),
        PlayerId(peer),
        Position(spawn_point(peer)),
        Velocity::default(),
        Heading::default(),
        Replicate::to_clients(NetworkTarget::All),
        // The owner predicts its own character; everyone else interpolates it.
        PredictionTarget::to_clients(NetworkTarget::Single(peer)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(peer)),
        ControlledBy {
            owner: trigger.entity,
            lifetime: Lifetime::SessionBased,
        },
    ));
}

/// Spreads players around a circle so they do not appear inside each other.
fn spawn_point(peer: PeerId) -> Vec3 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "only an angle is derived from the id; precision is irrelevant"
    )]
    let angle = (peer.to_bits() % 360) as f32 / 360.0 * TAU;
    Vec3::new(angle.cos(), 0.0, angle.sin()) * SPAWN_RADIUS
}
