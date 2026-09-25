//! Every player's home: the deed to build it, and waking up in it.
//!
//! Players without a home, and without the item to build one, are given it
//! when they arrive. Whoever collapses at the end of a day wakes up beside
//! the bed of their home, if they have one.

use bevy::prelude::*;
use messoria_content::{Purpose, Quality};
use messoria_shared::{
    content::Content,
    protocol::{Belongings, Heading, PlayerId, Position, Structure, WorldClock},
};

use crate::{
    building::{Built, Home},
    day_cycle::{ClockSystems, PassedOut},
    players::player_key,
};

/// Where, beside a bed and in the bed's frame, whoever collapsed wakes up.
const BESIDE_BED: Vec3 = Vec3::new(0.95, 0.0, 0.0);

pub(crate) struct HomesPlugin;

impl Plugin for HomesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, give_deeds)
            .add_systems(FixedUpdate, return_home.after(ClockSystems));
    }
}

fn give_deeds(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    homes: Query<&Home>,
    mut arrivals: Query<(&PlayerId, &mut Belongings), Added<PlayerId>>,
) {
    let Some(deed) = content
        .home()
        .and_then(|home| content.structure(home).built_from)
    else {
        return;
    };
    for (player, mut belongings) in &mut arrivals {
        let Some(key) = player_key(player.0) else {
            continue;
        };
        let housed = homes.iter().any(|home| home.0 == key);
        if !housed && belongings.0.count(deed) == 0 {
            belongings
                .0
                .add(&content, deed, Quality::Normal, 1, clock.0.day());
        }
    }
}

fn return_home(
    content: Res<Content>,
    built: Built,
    mut collapsed: MessageReader<PassedOut>,
    homes: Query<(&Home, &Structure)>,
    mut characters: Query<(&PlayerId, &mut Position, &mut Heading)>,
) {
    for &PassedOut(character) in collapsed.read() {
        let Ok((player, mut position, mut heading)) = characters.get_mut(character) else {
            continue;
        };
        let Some(key) = player_key(player.0) else {
            continue;
        };
        let Some((_, cabin)) = homes.iter().find(|(home, _)| home.0 == key) else {
            continue;
        };
        let bed = built
            .within(cabin)
            .find(|(_, structure)| content.structure(structure.kind).purpose == Purpose::Bed);
        if let Some((_, bed)) = bed {
            position.0 = bed.to_world(BESIDE_BED);
            heading.0 = bed.facing;
        }
    }
}
