//! The village: its shops' stalls and its buildings, where village.ron puts
//! them around the square.
//!
//! The village is laid out from the content every time the server starts,
//! so saves do not keep it.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_shared::{
    content::Content,
    protocol::{Shopfront, Structure},
    valley::Valley,
    village,
};

pub(crate) struct VillagePlugin;

impl Plugin for VillagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, lay_out_village);
    }
}

/// Part of the village, which the content lays out rather than players.
#[derive(Component)]
pub(crate) struct Landmark;

fn lay_out_village(content: Res<Content>, valley: Res<Valley>, mut commands: Commands) {
    let on_ground = |(x, z): (f32, f32)| {
        let place = village::CENTER + Vec2::new(x, z);
        Vec3::new(place.x, valley.height(place), place.y)
    };
    let layout = content.village();
    for stall in &layout.stalls {
        commands.spawn((
            Name::new(format!("{} stall", content.shop(stall.shop).name)),
            Shopfront {
                shop: stall.shop,
                position: on_ground(stall.at),
                facing: stall.turn,
            },
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
    for placement in &layout.structures {
        commands.spawn((
            Name::new(content.structure(placement.structure).name.clone()),
            Structure {
                kind: placement.structure,
                position: on_ground(placement.at),
                facing: placement.turn,
            },
            Landmark,
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
}

#[cfg(test)]
mod tests {
    use messoria_shared::content::load_content;

    use super::*;

    #[test]
    fn the_village_stands_on_its_protected_ground() {
        let content = load_content().expect("the shipped content is valid");
        let layout = content.village();
        let places = layout
            .stalls
            .iter()
            .map(|stall| stall.at)
            .chain(layout.structures.iter().map(|placement| placement.at));
        for (x, z) in places {
            let place = village::CENTER + Vec2::new(x, z);
            assert!(
                village::reaches(Vec3::new(place.x, 0.0, place.y), 0.0),
                "({x}, {z}) is outside the village"
            );
        }
    }
}
