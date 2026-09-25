//! A world hosted from the game shares one Bevy world between the server and
//! the host's client, which draws characters, fields and structures by
//! hanging models under the very entities the server replicates.

mod common;

use bevy::prelude::*;
use common::HostedWorld;
use lightyear::prelude::{ReplicateLike, input::native::InputMarker};
use messoria_shared::{
    content::load_content,
    protocol::{PlayerInput, Structure},
};

#[test]
fn what_the_host_draws_under_replicated_things_stays_its_own() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content);
    world.run(10);

    let owners: Vec<Entity> = {
        let world = world.world();
        let character = world
            .query_filtered::<Entity, With<InputMarker<PlayerInput>>>()
            .single(world)
            .expect("the host has a character");
        let structure = world
            .query_filtered::<Entity, With<Structure>>()
            .iter(world)
            .next()
            .expect("the village has structures");
        vec![character, structure]
    };
    let drawn: Vec<Entity> = owners
        .iter()
        .map(|&owner| world.world().spawn(ChildOf(owner)).id())
        .collect();
    world.run(5);

    for &model in &drawn {
        assert!(
            world.world().get::<ReplicateLike>(model).is_none(),
            "a model the host draws is not sent to other players"
        );
    }
    // Swapping a held item or a figure's parts takes models down like this.
    for model in drawn {
        world.world().despawn(model);
    }
    world.run(5);
}
