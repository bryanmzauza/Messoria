//! Player characters: one per connected client, gone when it disconnects.
//!
//! The server remembers every player who has been in the world. A player who
//! comes back finds their character as they left it, friendships, name and
//! look and all; if days went by, they slept through them, and their
//! perishables aged.

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
pub(crate) use messoria_shared::players::player_key;
use messoria_shared::{
    content::Content,
    energy::Energy,
    players::name_of,
    protocol::{
        Appearance, Belongings, Heading, Money, PlayerId, PlayerName, Position, SoldToday,
        Velocity, WorldClock,
    },
    valley::Valley,
};

use crate::{
    Beginning, WorldStart,
    appearance::{Unintroduced, wearable},
    friendship::Bonds,
    hosting::Admitted,
};

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
    &'a Bonds,
    &'a Appearance,
    &'a PlayerName,
    Has<Unintroduced>,
);

/// What a [`CharacterState`] query yields.
pub(crate) type CharacterStateItem<'a> = (
    &'a PlayerId,
    &'a Position,
    &'a Heading,
    &'a Energy,
    &'a Belongings,
    &'a Money,
    &'a SoldToday,
    &'a Bonds,
    &'a Appearance,
    &'a PlayerName,
    bool,
);

/// A character's state, to save or to keep while its player is away. A
/// player who never introduced themselves is kept without a name or a
/// look, to be asked again.
pub(crate) fn state_of(
    (_, position, heading, energy, belongings, money, sold, bonds, look, name, unintroduced): CharacterStateItem<'_>,
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
        friendships: bonds.to_saved(),
        name: (!unintroduced).then(|| name.0.clone()),
        look: (!unintroduced).then(|| look.0.clone()),
    }
}

fn remember_saved_players(beginning: Res<Beginning>, mut absent: ResMut<AbsentPlayers>) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        absent.players.clone_from(&saved.players);
    }
}

/// Spawns a character once the client is admitted, which takes a hello of
/// the right version on a connection the world has room for.
fn spawn_player(
    trigger: On<Add, Admitted>,
    clients: Query<&RemoteId, With<ClientOf>>,
    valley: Res<Valley>,
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
        newcomer(&content, spawn.with_y(valley.height(spawn.xz())), today)
    };

    // A look the content no longer has, or none yet, shows as one picked
    // from the player's id.
    let look = state
        .look
        .clone()
        .filter(|look| wearable(&content, look))
        .unwrap_or_else(|| content.characters().look_for(peer.to_bits()));
    let name = state.name.clone().unwrap_or_else(|| name_of(peer));
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
            Bonds::from_saved(&state.friendships),
            (Appearance(look), PlayerName(name)),
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
    if state.look.is_none() {
        commands.entity(character).insert(Unintroduced);
    }
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
        friendships: Vec::new(),
        name: None,
        look: None,
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
        player.friendships.clear();
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
