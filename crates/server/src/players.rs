//! Player characters: one per connected client, gone when it disconnects.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
use messoria_content::Quality;
use messoria_economy::Wallet;
use messoria_inventory::Inventory;
use messoria_shared::{
    content::Content,
    energy::Energy,
    protocol::{Belongings, Heading, Money, PlayerId, Position, SoldToday, Velocity, WorldClock},
    terrain::Terrain,
};

use crate::terrain::ground_height;

/// Distance from the world origin at which players appear.
const SPAWN_RADIUS: f32 = 3.0;

pub(crate) struct PlayersPlugin;

impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_player);
    }
}

/// The character a client's connection controls, set on the connection entity.
#[derive(Component)]
pub(crate) struct ControlledCharacter(pub Entity);

/// Spawns a character once the connection is confirmed, not when the link is
/// first created: the server may still reject a connection attempt.
fn spawn_player(
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    terrain: Res<Terrain>,
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut commands: Commands,
) {
    let Ok(&RemoteId(peer)) = clients.get(trigger.entity) else {
        return;
    };

    let mut starting_kit = Inventory::default();
    for &(item, count) in content.starting_inventory() {
        let left = starting_kit.add(&content, item, Quality::Normal, count, clock.0.day());
        if left > 0 {
            warn!("the starting inventory does not fit; {left} of it is left out");
        }
    }

    let spawn = spawn_point(peer);
    let ground = ground_height(&terrain, spawn.x, spawn.z).unwrap_or_default();
    let character = commands
        .spawn((
            Name::new(format!("Player {peer:?}")),
            PlayerId(peer),
            Position(spawn.with_y(ground)),
            Velocity::default(),
            Heading::default(),
            Energy::FULL,
            Belongings(starting_kit),
            Money(Wallet::with(content.starting_money())),
            SoldToday::default(),
            Replicate::to_clients(NetworkTarget::All),
            // The owner predicts its own character; everyone else interpolates it.
            PredictionTarget::to_clients(NetworkTarget::Single(peer)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(peer)),
            ControlledBy {
                owner: trigger.entity,
                lifetime: Lifetime::SessionBased,
            },
        ))
        .id();
    commands
        .entity(trigger.entity)
        .insert(ControlledCharacter(character));
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
