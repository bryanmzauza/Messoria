//! Homes: every newcomer builds a cabin from their deed and sleeps in its
//! bed; its chest keeps what is put in it, through a restart; whoever
//! collapses at night wakes up at home; and nothing is built where something
//! already stands.

mod common;

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use common::HostedWorld;
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, Purpose};
use messoria_save::SaveDir;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    protocol::{Happened, MoveItem, MoveStored, Notice, Place, SleepRequest, Stored, Structure},
    terrain::Terrain,
    village,
};

#[test]
fn a_newcomer_builds_a_cabin_and_sleeps_in_its_bed() {
    let content = load_content().expect("the shipped content is valid");
    let deed = content.id("cabin_deed").expect("the cabin deed exists");
    let mut world = HostedWorld::new(content.clone());
    world.run(2);
    assert_eq!(world.carried(deed), 1, "newcomers are given a deed");

    world.build_cabin(&content);
    assert_eq!(world.carried(deed), 0, "the deed was used");
    assert!(
        world
            .heard()
            .happenings
            .iter()
            .any(|happening| happening.what == Happened::Built)
    );
    for purpose in [Purpose::Bed, Purpose::Storage] {
        assert!(
            world.structure(&content, purpose).is_some(),
            "the cabin comes with its {purpose:?}"
        );
    }

    world.set_time("21:00");
    world.send(SleepRequest::Sleep);
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.heard().notices.last(), Some(&Notice::SleepInABed));

    let bed = world.structure(&content, Purpose::Bed).expect("a bed");
    world.teleport(bed.to_world(Vec3::new(0.95, 0.0, 0.0)));
    let today = world.clock().day();
    world.send(SleepRequest::Sleep);
    // Everyone in the world is asleep, so the night passes at once.
    world.run_until("the next day", |world| world.clock().day() == today + 1);
    assert_eq!(
        world.heard().notices.len(),
        1,
        "only the first try was refused"
    );
}

#[test]
fn a_chest_keeps_the_harvest_through_a_restart() {
    let content = load_content().expect("the shipped content is valid");
    let turnip = content.id("turnip").expect("turnips exist");
    let save_dir = scratch_folder("chest");
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut world = HostedWorld::with_world(content.clone(), setup);
    world.run(2);
    world.build_cabin(&content);

    let chest = world
        .structure(&content, Purpose::Storage)
        .expect("a chest");
    world.teleport(chest.to_world(Vec3::new(0.0, 0.0, -1.2)));
    world.give(&content, "turnip", 12);
    let slot = world.slot_of(turnip).expect("the turnips are carried");
    world.send(MoveStored {
        chest: chest.position,
        from: Place::Carried(slot),
        to: Place::Stored(0),
    });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.carried(turnip), 0);
    assert_eq!(world.stored(&content).count(turnip), 12);
    world.stop();
    drop(world);

    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    let mut resumed = HostedWorld::with_world(content.clone(), setup);
    resumed.run(2);
    assert_eq!(resumed.stored(&content).count(turnip), 12);
    let deed = content.id("cabin_deed").expect("the cabin deed exists");
    assert_eq!(
        resumed.carried(deed),
        0,
        "a player with a home is not given another deed"
    );
}

#[test]
fn whoever_collapses_at_night_wakes_up_beside_their_bed() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    world.run(2);
    world.build_cabin(&content);
    let bed = world.structure(&content, Purpose::Bed).expect("a bed");

    let (feet, _) = world.character().expect("the host has a character");
    let away = ground(&mut world, feet + Vec3::new(15.0, 0.0, 0.0));
    world.teleport(away);
    let today = world.clock().day();
    world.set_time("01:58");
    world.run_until("the day to run out", |world| {
        world.clock().day() == today + 1
    });

    let (feet, _) = world.character().expect("the host has a character");
    assert!(
        feet.xz().distance(bed.position.xz()) < 1.5,
        "woke up at {feet}, the bed is at {}",
        bed.position
    );
}

#[test]
fn nothing_is_built_where_something_stands() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    world.run(2);
    world.give(&content, "chest", 3);
    // Into the hotbar, which is full, in place of the seeds in its sixth slot.
    let carried = world
        .slot_of(content.id("chest").expect("chests exist"))
        .expect("the chests are carried");
    world.send(MoveItem {
        from: carried,
        to: 5,
    });
    world.run(common::PAUSE_BETWEEN_USES);
    let chest = world.slot_holding(&content, "chest");

    let square = Vec3::new(village::CENTER.x, 0.0, village::CENTER.y + 4.0);
    let square = ground(&mut world, square);
    world.teleport(square + Vec3::new(0.0, 0.0, 3.0));
    world.use_item(chest, square);
    assert_eq!(world.heard().notices.last(), Some(&Notice::ProtectedGround));

    let arrival = ground(&mut world, Vec3::ZERO);
    world.teleport(arrival);
    world.build_cabin(&content);
    let cabin = world.home(&content).expect("the cabin stands");
    let chests = world.count(&content, Purpose::Storage);

    // Its wall is in the way of a chest across it.
    let across_wall = cabin.to_world(Vec3::new(-2.6, 0.0, 0.0));
    world.teleport(cabin.to_world(Vec3::new(-4.5, 0.0, 0.0)));
    world.use_item(chest, across_wall);
    assert_eq!(world.heard().notices.last(), Some(&Notice::NoRoomToBuild));

    // Its floor has room for one more.
    world.teleport(cabin.to_world(Vec3::new(0.0, 0.0, -1.5)));
    world.use_item(chest, cabin.to_world(Vec3::new(0.0, 0.0, 0.2)));
    assert_eq!(world.count(&content, Purpose::Storage), chests + 1);

    // And its ground is not dug.
    let shovel = world.slot_holding(&content, "shovel");
    world.use_item(shovel, cabin.to_world(Vec3::new(1.0, 0.0, -1.5)));
    assert_eq!(world.heard().notices.last(), Some(&Notice::SceneryInTheWay));
}

impl HostedWorld {
    /// Builds the host's cabin with its front 3.5 m ahead of them.
    fn build_cabin(&mut self, content: &Catalog) {
        let (feet, _) = self.character().expect("the host has a character");
        let front = ground(self, feet + Vec3::new(0.0, 0.0, -3.5));
        let deed = self.slot_holding(content, "cabin_deed");
        self.use_item(deed, front);
        assert!(self.home(content).is_some(), "the cabin was built");
    }

    fn home(&mut self, content: &Catalog) -> Option<Structure> {
        let cabin = content.home().expect("players have a home");
        let world = self.world();
        world
            .query::<&Structure>()
            .iter(world)
            .find(|structure| structure.kind == cabin)
            .copied()
    }

    fn structure(&mut self, content: &Catalog, purpose: Purpose) -> Option<Structure> {
        let world = self.world();
        world
            .query::<&Structure>()
            .iter(world)
            .find(|structure| content.structure(structure.kind).purpose == purpose)
            .copied()
    }

    fn count(&mut self, content: &Catalog, purpose: Purpose) -> usize {
        let world = self.world();
        world
            .query::<&Structure>()
            .iter(world)
            .filter(|structure| content.structure(structure.kind).purpose == purpose)
            .count()
    }

    /// What the first chest keeps.
    fn stored(&mut self, content: &Catalog) -> messoria_inventory::Inventory {
        let world = self.world();
        world
            .query::<(&Structure, &Stored)>()
            .iter(world)
            .find(|(structure, _)| content.structure(structure.kind).purpose == Purpose::Storage)
            .map(|(_, stored)| stored.0.clone())
            .expect("a chest")
    }
}

/// The ground under `point`.
fn ground(world: &mut HostedWorld, point: Vec3) -> Vec3 {
    let height = world
        .world()
        .resource::<Terrain>()
        .surface_below(point.with_y(60.0), 120.0)
        .expect("ground there");
    point.with_y(height)
}

fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
