//! Villagers keep to their schedules: followed through a day, the grocer
//! tends her garden at dawn, keeps her stall in its hours, lingers by the
//! well and goes home at night, walking the village's paths to each; and
//! with nobody near the village, villagers are still where their day has
//! them.

mod common;

use bevy::prelude::*;
use common::HostedWorld;
use messoria_content::Catalog;
use messoria_shared::{
    content::load_content,
    protocol::{Doing, Position, Villager},
    village,
};

/// Updates for a villager to walk anywhere in the village: a few minutes.
const A_WALK: u32 = 2_400;

#[test]
fn the_grocer_keeps_to_her_day() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let marit = content.villager_id("marit").expect("Marit lives here");
    // Near enough to the village to watch her walk.
    world.teleport_to(Vec2::new(0.0, 10.0));

    world.set_time("07:00");
    world.run(A_WALK);
    let (at, doing) = villager(&mut world, marit);
    assert_near(at, spot(&content, "garden_north_west"), "the garden");
    assert_eq!(doing, Doing::Tending);

    world.set_time("09:00");
    world.run(A_WALK);
    let (at, doing) = villager(&mut world, marit);
    assert_near(at, spot(&content, "grocer_stall"), "her stall");
    assert_eq!(doing, Doing::Standing);
    let facing = {
        let world = world.world();
        world
            .query::<(&Villager, &messoria_shared::protocol::Heading)>()
            .iter(world)
            .find(|(villager, _)| villager.0 == marit)
            .map(|(_, heading)| heading.0)
            .expect("Marit is in the world")
    };
    let stall = content
        .village()
        .spot("grocer_stall")
        .expect("the stall spot");
    assert!(
        (facing - stall.face).abs() < 1e-4,
        "facing {facing}, the spot says {}",
        stall.face
    );

    world.set_time("17:30");
    world.run(A_WALK);
    let (at, doing) = villager(&mut world, marit);
    assert_near(at, spot(&content, "well_side"), "the well");
    assert_eq!(doing, Doing::Standing);

    world.set_time("21:00");
    world.run(A_WALK);
    let (at, doing) = villager(&mut world, marit);
    let home = content
        .village()
        .labeled("greengage_house")
        .expect("her house is labeled");
    assert!(
        at.xz().distance(village::CENTER + Vec2::from(home.at)) < 6.0,
        "home at {:?}, she is at {at}",
        home.at
    );
    assert_eq!(doing, Doing::Indoors);
}

#[test]
fn every_villager_walks_to_work_and_is_there_even_unwatched() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    world.teleport_to(Vec2::new(0.0, 10.0));
    // Fifty minutes into the morning's slots, everyone has had time to
    // walk to theirs.
    world.set_time("11:00");
    world.run(1_500);
    let hester = content
        .villager_id("hester")
        .expect("Hester keeps the tavern");
    let (at, doing) = villager(&mut world, hester);
    assert_near(at, spot(&content, "tavern_counter"), "the tavern's counter");
    assert_eq!(doing, Doing::Standing);
    let still_walking: Vec<(String, Vec3)> = {
        let world = world.world();
        world
            .query::<(&Villager, &Position, &Doing)>()
            .iter(world)
            .filter(|(.., doing)| **doing == Doing::Walking)
            .map(|(villager, position, _)| (content.villager(villager.0).name.clone(), position.0))
            .collect()
    };
    assert!(still_walking.is_empty(), "still walking: {still_walking:?}");

    // Far from the village, nobody watches: villagers are simply where they
    // belong when the time comes.
    world.teleport_to(Vec2::new(600.0, 600.0));
    world.set_time("17:35");
    world.run(120);
    let marit = content.villager_id("marit").expect("Marit lives here");
    let (at, doing) = villager(&mut world, marit);
    assert_near(at, spot(&content, "well_side"), "the well");
    assert_eq!(doing, Doing::Standing);
}

fn spot(content: &Catalog, label: &str) -> Vec2 {
    let spot = content.village().spot(label).expect("the spot exists");
    village::CENTER + Vec2::from(spot.at)
}

fn villager(world: &mut HostedWorld, id: messoria_content::VillagerId) -> (Vec3, Doing) {
    let world = world.world();
    world
        .query::<(&Villager, &Position, &Doing)>()
        .iter(world)
        .find(|(villager, ..)| villager.0 == id)
        .map(|(_, position, doing)| (position.0, *doing))
        .expect("the villager is in the world")
}

fn assert_near(at: Vec3, place: Vec2, what: &str) {
    assert!(
        at.xz().distance(place) < 1.0,
        "expected {what} at {place}, found {at}"
    );
}
