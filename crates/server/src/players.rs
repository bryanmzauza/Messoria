//! Player characters: one per connected client, gone when it disconnects.
//!
//! The server remembers every player who has been in the world. A player who
//! comes back finds their character as they left it; if days went by, they
//! slept through them, and their perishables aged.

use std::{
    collections::{HashMap, HashSet},
    f32::consts::TAU,
};

use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
use messoria_content::{Catalog, Quality};
use messoria_economy::Wallet;
use messoria_inventory::Inventory;
use messoria_save::PlayerState;
use messoria_shared::{
    content::Content,
    energy::Energy,
    protocol::{Belongings, Heading, Money, PlayerId, Position, SoldToday, Velocity, WorldClock},
    terrain::Terrain,
};

use crate::{Beginning, WorldStart, terrain::ground_height};

/// Distance from the world origin at which players appear.
const SPAWN_RADIUS: f32 = 3.0;

pub(crate) struct PlayersPlugin;

impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbsentPlayers>()
            .add_systems(Startup, remember_saved_players)
            .add_observer(spawn_player)
            .add_observer(remember_departing_player);
    }
}

/// The character a client's connection controls, set on the connection entity.
#[derive(Component)]
pub(crate) struct ControlledCharacter(pub Entity);

/// Players who have been in the world but are not in it now, by key.
#[derive(Resource, Default)]
pub(crate) struct AbsentPlayers {
    pub players: HashMap<String, PlayerState>,
    /// Players who left since the world was last saved.
    pub unsaved: HashSet<String>,
}

/// What a character is made of, as saved.
pub(crate) type CharacterState<'a> = (
    &'a PlayerId,
    &'a Position,
    &'a Heading,
    &'a Energy,
    &'a Belongings,
    &'a Money,
    &'a SoldToday,
);

/// The name of a player's save file, for the players the server can
/// recognize when they come back.
pub(crate) fn player_key(peer: PeerId) -> Option<String> {
    match peer {
        PeerId::Netcode(id) => Some(format!("{id:016x}")),
        // The player hosting the world from the game.
        PeerId::Local(_) => Some("host".to_owned()),
        PeerId::Entity(_) | PeerId::Raw(_) | PeerId::Steam(_) | PeerId::Server => None,
    }
}

/// A character's state, to save or to keep while its player is away.
pub(crate) fn state_of(
    (_, position, heading, energy, belongings, money, sold): CharacterState<'_>,
    today: u32,
) -> PlayerState {
    PlayerState {
        position: position.0,
        heading: heading.0,
        energy: energy.current(),
        inventory: belongings.0.clone(),
        money: money.0,
        sold_today: sold.0.clone(),
        saved_on: today,
    }
}

fn remember_saved_players(beginning: Res<Beginning>, mut absent: ResMut<AbsentPlayers>) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        absent.players.clone_from(&saved.players);
    }
}

/// Spawns a character once the connection is confirmed, not when the link is
/// first created: the server may still reject a connection attempt.
fn spawn_player(
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    terrain: Res<Terrain>,
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut absent: ResMut<AbsentPlayers>,
    mut commands: Commands,
) {
    let Ok(&RemoteId(peer)) = clients.get(trigger.entity) else {
        return;
    };
    let today = clock.0.day();
    let returning = player_key(peer).and_then(|key| absent.players.remove(&key));
    let state = if let Some(state) = returning {
        back_after_absence(state, &content, today)
    } else {
        let spawn = spawn_point(peer);
        let ground = ground_height(&terrain, spawn.x, spawn.z).unwrap_or_default();
        newcomer(&content, spawn.with_y(ground), today)
    };

    let character = commands
        .spawn((
            Name::new(format!("Player {peer:?}")),
            PlayerId(peer),
            Position(state.position),
            Velocity::default(),
            Heading(state.heading),
            Energy::new(state.energy),
            Belongings(state.inventory),
            Money(state.money),
            SoldToday(state.sold_today),
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

/// A character leaving the world, whose state is kept until they return.
/// Its components are still there while it is being despawned.
fn remember_departing_player(
    trigger: On<Remove, PlayerId>,
    characters: Query<CharacterState<'_>>,
    clock: Single<&WorldClock>,
    mut absent: ResMut<AbsentPlayers>,
) {
    let Ok(character) = characters.get(trigger.entity) else {
        return;
    };
    let Some(key) = player_key(character.0.0) else {
        return;
    };
    absent
        .players
        .insert(key.clone(), state_of(character, clock.0.day()));
    absent.unsaved.insert(key);
}

/// What a player new to the world starts with.
fn newcomer(content: &Catalog, position: Vec3, today: u32) -> PlayerState {
    let mut inventory = Inventory::default();
    for &(item, count) in content.starting_inventory() {
        let left = inventory.add(content, item, Quality::Normal, count, today);
        if left > 0 {
            warn!("the starting inventory does not fit; {left} of it is left out");
        }
    }
    PlayerState {
        position,
        heading: 0.0,
        energy: Energy::FULL.current(),
        inventory,
        money: Wallet::with(content.starting_money()),
        sold_today: default(),
        saved_on: today,
    }
}

/// A returning player's state, caught up with the days they were away: they
/// slept through each night, their sales limits started over and what they
/// carry aged.
fn back_after_absence(mut state: PlayerState, content: &Catalog, today: u32) -> PlayerState {
    if state.saved_on < today {
        state.energy = Energy::FULL.current();
        state.sold_today.clear();
        state.inventory.spoil(content, today);
        state.saved_on = today;
    }
    state
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

#[cfg(test)]
mod tests {
    use messoria_shared::content::load_content;

    use super::*;

    fn tired_player(content: &Catalog, saved_on: u32) -> PlayerState {
        let mut player = newcomer(content, Vec3::ZERO, saved_on);
        player.energy = 10;
        let grocer = content.shop_id("grocer").expect("the grocer exists");
        let berries = content.id("wild_berries").expect("wild berries exist");
        player.sold_today.record(grocer, berries, 5);
        player
    }

    #[test]
    fn players_back_the_same_day_find_everything_as_they_left_it() {
        let content = load_content().expect("the shipped content is valid");
        let player = tired_player(&content, 4);
        assert_eq!(back_after_absence(player.clone(), &content, 4), player);
    }

    #[test]
    fn players_back_days_later_slept_and_their_food_aged() {
        let content = load_content().expect("the shipped content is valid");
        let berries = content.id("wild_berries").expect("wild berries exist");
        let shelf_life = content.item(berries).shelf_life.expect("berries spoil");
        let away = u32::from(shelf_life);

        let back = back_after_absence(tired_player(&content, 4), &content, 4 + away);
        assert_eq!(back.energy, Energy::FULL.current());
        assert!(back.sold_today.is_empty());
        assert_eq!(back.inventory.count(berries), 0, "the berries spoiled");
        assert_eq!(back.saved_on, 4 + away);
    }
}
