//! Gathering the valley's scenery: felling trees for wood, breaking rocks for
//! stone and picking berries by hand; what was gathered growing back, being
//! kept by the save, and selling at the carpenter's.

mod common;

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use common::HostedWorld;
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, Gathering, Tool};
use messoria_save::SaveDir;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    obstacles::Obstacles,
    protocol::{Deal, Gather, Gathered, Notice, Prop},
    terrain::Terrain,
};

#[test]
fn a_felled_tree_gives_wood_and_leaves_a_stump_that_grows_back() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let (tree, oak) = world.nearest_prop(&content, "oak");
    let gathering = gathering_of(&content, oak);
    let wood = content.id("wood").expect("wood exists");
    let axe = world.slot_holding(&content, "axe");
    let target = world.stand_by(&content, oak);

    for _ in 1..gathering.strikes {
        world.use_item(axe, target);
    }
    assert_eq!(
        world.carried(wood),
        0,
        "the tree stands until the last strike"
    );
    assert!(world.gathered(tree).is_none());
    world.use_item(axe, target);
    assert_eq!(world.carried(wood), u32::from(gathering.yields[0].1));
    assert!(world.gathered(tree).is_some());
    assert!(
        world.obstacle_ahead(oak).is_some(),
        "the stump still stands in the way"
    );

    world.use_item(axe, target);
    assert_eq!(world.heard().notices.last(), Some(&Notice::NothingToGather));

    let days = gathering.regrows_after.expect("trees grow back");
    for _ in 1..days {
        world.sleep_through_the_night();
    }
    assert!(world.gathered(tree).is_some(), "not grown back a day early");
    world.sleep_through_the_night();
    assert!(
        world.gathered(tree).is_none(),
        "grown back after {days} days"
    );
}

#[test]
fn the_carpenter_buys_wood() {
    let content = load_content().expect("the shipped content is valid");
    let wood = content.id("wood").expect("wood exists");
    let mut world = HostedWorld::new(content.clone());
    world.give(&content, "wood", 20);
    world.set_time("10:00");
    world.go_to_stall(&content, "carpenter");
    let coins = world.coins();

    let slot = world.slot_of(wood).expect("the wood is carried");
    world.trade(&content, "carpenter", Deal::Sell { slot, count: 20 });
    assert_eq!(world.carried(wood), 0);
    assert!(world.coins() > coins, "the wood sold");
}

#[test]
fn berries_are_picked_by_hand_and_grow_back() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let berries = content.id("wild_berries").expect("wild berries exist");
    let (bush, berry_bush) = world.nearest_prop(&content, "berry_bush");
    let gathering = gathering_of(&content, berry_bush);
    let target = world.stand_by(&content, berry_bush);
    let before = world.carried(berries);

    let axe = world.slot_holding(&content, "axe");
    world.use_item(axe, target);
    assert_eq!(world.heard().notices.last(), Some(&Notice::PickByHand));

    world.send(Gather { target });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(
        world.carried(berries),
        before + u32::from(gathering.yields[0].1)
    );
    assert!(world.gathered(bush).is_some());
    assert!(world.obstacle_ahead(berry_bush).is_some(), "the bush stays");

    for _ in 0..gathering.regrows_after.expect("berries grow back") {
        world.sleep_through_the_night();
    }
    assert!(world.gathered(bush).is_none());
}

#[test]
fn trees_are_not_felled_by_hand_nor_rocks_with_an_axe() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let (_, oak) = world.nearest_prop(&content, "oak");
    let target = world.stand_by(&content, oak);
    world.send(Gather { target });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(
        world.heard().notices.last(),
        Some(&Notice::NeedsTool(Tool::Axe))
    );

    let (_, boulder) = world.nearest_prop(&content, "boulder");
    let target = world.stand_by(&content, boulder);
    let axe = world.slot_holding(&content, "axe");
    world.use_item(axe, target);
    assert_eq!(
        world.heard().notices.last(),
        Some(&Notice::NeedsTool(Tool::Pickaxe))
    );
}

#[test]
fn broken_rocks_and_felled_trees_stay_so_after_a_restart() {
    let content = load_content().expect("the shipped content is valid");
    let save_dir = scratch_folder("gathered");
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut world = HostedWorld::with_world(content.clone(), setup);

    let (_, boulder) = world.nearest_prop(&content, "boulder");
    let target = world.stand_by(&content, boulder);
    let pickaxe = world.slot_holding(&content, "pickaxe");
    for _ in 0..gathering_of(&content, boulder).strikes {
        world.use_item(pickaxe, target);
    }
    let stone = content.id("stone").expect("stone exists");
    assert_eq!(
        world.carried(stone),
        u32::from(gathering_of(&content, boulder).yields[0].1)
    );
    assert!(
        world.obstacle_ahead(boulder).is_none(),
        "nothing stands where the rock was"
    );

    let (_, oak) = world.nearest_prop(&content, "oak");
    let target = world.stand_by(&content, oak);
    let axe = world.slot_holding(&content, "axe");
    for _ in 0..gathering_of(&content, oak).strikes {
        world.use_item(axe, target);
    }
    world.stop();
    drop(world);

    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    let mut resumed = HostedWorld::with_world(content, setup);
    for prop in [boulder, oak] {
        let entity = resumed
            .prop_at(prop.position)
            .expect("the seed grows it again");
        assert!(
            resumed.gathered(entity).is_some(),
            "{prop:?} is still gathered"
        );
    }
    assert!(resumed.obstacle_ahead(boulder).is_none());
    assert!(resumed.obstacle_ahead(oak).is_some());
}

fn gathering_of(content: &Catalog, prop: Prop) -> Gathering {
    content
        .prop(prop.kind)
        .gather
        .clone()
        .expect("the prop can be gathered")
}

impl HostedWorld {
    /// The standing prop of kind `key` nearest where players arrive.
    fn nearest_prop(&mut self, content: &Catalog, key: &str) -> (Entity, Prop) {
        let kind = content.prop_id(key).expect("the prop exists");
        let world = self.world();
        world
            .query_filtered::<(Entity, &Prop), Without<Gathered>>()
            .iter(world)
            .filter(|(_, prop)| prop.kind == kind)
            .min_by(|(_, a), (_, b)| a.position.length().total_cmp(&b.position.length()))
            .map(|(entity, prop)| (entity, *prop))
            .expect("the valley grows some")
    }

    fn prop_at(&mut self, position: Vec3) -> Option<Entity> {
        let world = self.world();
        world
            .query::<(Entity, &Prop)>()
            .iter(world)
            .find(|(_, prop)| prop.position == position)
            .map(|(entity, _)| entity)
    }

    fn gathered(&mut self, prop: Entity) -> Option<Gathered> {
        self.world().get::<Gathered>(prop).copied()
    }

    /// Puts the host's character a step from `prop`, on the side facing
    /// where players arrive, and returns a point on the prop to aim at.
    fn stand_by(&mut self, content: &Catalog, prop: Prop) -> Vec3 {
        let footprint = content.prop(prop.kind).radius * prop.scale;
        let toward_arrival = (-prop.position.xz()).normalize_or(Vec2::X);
        let spot = prop.position.xz() + toward_arrival * (footprint + 1.2);
        let ground = self
            .world()
            .resource::<Terrain>()
            .surface_below(Vec3::new(spot.x, prop.position.y + 5.0, spot.y), 10.0)
            .expect("ground beside the prop");
        self.teleport(Vec3::new(spot.x, ground, spot.y));
        prop.position + Vec3::Y
    }

    /// The obstacle a ray across `prop`'s middle, at waist height, meets.
    fn obstacle_ahead(&mut self, prop: Prop) -> Option<Entity> {
        let start = prop.position + Vec3::new(-3.0, 1.0, 0.0);
        self.world()
            .resource::<Obstacles>()
            .raycast(start, Vec3::X, 3.0)
            .map(|(owner, _)| owner)
    }
}

fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
