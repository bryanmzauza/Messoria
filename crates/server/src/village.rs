//! The village's layout: where each shop's stall stands.

use std::f32::consts::PI;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_shared::{content::Content, protocol::Shopfront, terrain::Terrain, village};

use crate::terrain::ground_height;

pub(crate) struct VillagePlugin;

impl Plugin for VillagePlugin {
    fn build(&self, app: &mut App) {
        // After startup, once the terrain the stalls stand on exists.
        app.add_systems(PostStartup, build_stalls);
    }
}

/// A shop's stall, placed relative to the village's center.
struct Stall {
    shop: &'static str,
    offset: Vec2,
    /// Which way the counter faces, as a heading.
    facing: f32,
}

/// Every stall in the village. Counters face the way players arrive from.
const STALLS: [Stall; 1] = [Stall {
    shop: "grocer",
    offset: Vec2::new(0.0, -3.0),
    facing: PI,
}];

fn build_stalls(content: Res<Content>, terrain: Res<Terrain>, mut commands: Commands) {
    for stall in &STALLS {
        let Some(shop) = content.shop_id(stall.shop) else {
            panic!(
                "the village has a stall for shop `{}`, which the content does not define",
                stall.shop
            );
        };
        let place = village::CENTER + stall.offset;
        let ground = ground_height(&terrain, place.x, place.y).unwrap_or_default();
        commands.spawn((
            Name::new(format!("{} stall", content.shop(shop).name)),
            Shopfront {
                shop,
                position: Vec3::new(place.x, ground, place.y),
                facing: stall.facing,
            },
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
}

#[cfg(test)]
mod tests {
    use messoria_shared::content::load_content;

    use super::*;

    #[test]
    fn every_stall_belongs_to_a_shop_in_the_content() {
        let content = load_content().expect("the shipped content is valid");
        for stall in &STALLS {
            assert!(
                content.shop_id(stall.shop).is_some(),
                "no shop `{}`",
                stall.shop
            );
            let place = village::CENTER + stall.offset;
            assert!(village::reaches(Vec3::new(place.x, 0.0, place.y), 0.0));
        }
    }
}
