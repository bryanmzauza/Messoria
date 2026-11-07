//! Hosting: a world hosted from the game shares one Bevy world between the
//! server and the host's client, which draws characters, fields and
//! structures by hanging models under the very entities the server
//! replicates; and a world takes guests only while it is open and has
//! room, telling the others why not.

mod common;

use std::net::{Ipv4Addr, SocketAddr};

use bevy::prelude::*;
use common::{HostedWorld, client_app, free_udp_port, local_character_if_any, run_apps_until};
use lightyear::prelude::{ReplicateLike, input::native::InputMarker};
use messoria_calendar::WorldTime;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    protocol::{PlayerInput, Rejected, Rejection, Structure},
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
            "what hangs under a replicated entity is not replicated itself"
        );
    }
    for &owner in &owners {
        world.world().entity_mut(owner).despawn();
    }
    world.run(5);
}

#[test]
fn a_full_world_turns_the_next_guest_away_with_the_reason() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = common::server_app_for(
        server_addr,
        WorldSetup::fresh(1, WorldTime::FIRST_DAWN),
        true,
        1,
    );
    let (mut first, mut second) = (client_app(server_addr, 31), client_app(server_addr, 32));
    run_apps_until(
        &mut server,
        &mut [&mut first],
        "the first guest to control a character",
        |_, clients| local_character_if_any(clients[0]).is_some(),
    );
    run_apps_until(
        &mut server,
        &mut [&mut first, &mut second],
        "the second guest to be turned away",
        |_, clients| {
            !clients[1]
                .world()
                .resource::<common::Heard>()
                .rejections
                .is_empty()
        },
    );
    let heard = second.world().resource::<common::Heard>();
    assert_eq!(
        heard.rejections,
        [Rejected {
            reason: Rejection::Full
        }]
    );
    assert!(
        local_character_if_any(&mut second).is_none(),
        "a guest turned away gets no character"
    );
}

#[test]
fn a_closed_world_takes_nobody() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let mut server = common::server_app_for(
        server_addr,
        WorldSetup::fresh(1, WorldTime::FIRST_DAWN),
        false,
        8,
    );
    let mut guest = client_app(server_addr, 33);
    run_apps_until(
        &mut server,
        &mut [&mut guest],
        "the guest to be turned away",
        |_, clients| {
            !clients[0]
                .world()
                .resource::<common::Heard>()
                .rejections
                .is_empty()
        },
    );
    assert_eq!(
        guest.world().resource::<common::Heard>().rejections[0].reason,
        Rejection::Closed
    );
}
