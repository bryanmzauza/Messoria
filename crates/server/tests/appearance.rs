//! Looks: a newcomer is asked who they are and introduces themselves once;
//! clothes bought from the tailor are changed into at the wardrobe in the
//! cabin, the old ones going back into the backpack; the barber restyles
//! hair for a price; and all of it is seen the same after a restart, while
//! a player who never introduced themselves is asked again.

mod common;

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use common::HostedWorld;
use lightyear::prelude::input::native::InputMarker;
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, Look, Purpose};
use messoria_save::SaveDir;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    protocol::{
        Appearance, ChangeClothes, Introduce, Notice, PlayerInput, PlayerName, Restyle, Structure,
    },
    terrain::Terrain,
};

#[test]
fn a_look_of_ones_own_is_made_changed_and_kept_through_a_restart() {
    let content = load_content().expect("the shipped content is valid");
    let wardrobe = &content.characters().wardrobe;
    let save_dir = scratch_folder("look");
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut world = HostedWorld::with_world(content.clone(), setup);
    world.run(2);
    assert_eq!(
        world.heard().newcomers,
        1,
        "a newcomer is asked who they are"
    );

    // The look chosen is seen, a hat is not offered, and only the first
    // introduction counts.
    let chosen = Look {
        body: wardrobe.bodies[2].clone(),
        eyes: wardrobe.eye_colors[3],
        outfit: wardrobe.outfits[1].clone(),
        hair: wardrobe.hair[4].clone(),
        hair_color: wardrobe.hair_colors[5],
        hat: None,
    };
    let mut hatted = chosen.clone();
    hatted.hat = Some("hat-straw.png".to_owned());
    world.send(Introduce {
        name: " Wren ".to_owned(),
        look: hatted,
    });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_ne!(world.look().0, chosen, "a hatted look is not offered");
    world.send(Introduce {
        name: " Wren ".to_owned(),
        look: chosen.clone(),
    });
    world.run_until("the look to be taken", |world| world.look().0 == chosen);
    assert_eq!(world.look().1, "Wren", "the name is trimmed");
    world.send(Introduce {
        name: "Tam".to_owned(),
        look: content.characters().look_for(9),
    });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.look(), (chosen.clone(), "Wren".to_owned()));

    change_clothes_at_home(&mut world, &content);
    visit_the_barber(&mut world, &content, &chosen);

    let final_look = world.look();
    world.stop();
    drop(world);

    let saved = save_dir
        .load(&content)
        .expect("the save loads")
        .expect("the world was saved");
    assert_eq!(saved.players["host"].look, Some(final_look.0.clone()));
    assert_eq!(saved.players["host"].name.as_deref(), Some("Wren"));
    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    let mut resumed = HostedWorld::with_world(content.clone(), setup);
    resumed.run(2);
    assert_eq!(resumed.look(), final_look);
    assert_eq!(resumed.heard().newcomers, 0, "nobody is asked twice");
}

#[test]
fn a_player_who_never_introduced_themselves_is_asked_again() {
    let content = load_content().expect("the shipped content is valid");
    let save_dir = scratch_folder("unintroduced");
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut world = HostedWorld::with_world(content.clone(), setup);
    world.run(2);
    assert_eq!(world.heard().newcomers, 1);
    world.stop();
    drop(world);

    let saved = save_dir
        .load(&content)
        .expect("the save loads")
        .expect("the world was saved");
    assert_eq!(saved.players["host"].look, None);
    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    let mut resumed = HostedWorld::with_world(content, setup);
    resumed.run(2);
    assert_eq!(resumed.heard().newcomers, 1);
}

/// At the wardrobe, a tunic replaces the overalls, which go into the
/// backpack; a hat goes on, and comes off again.
fn change_clothes_at_home(world: &mut HostedWorld, content: &Catalog) {
    world.settle_on_a_lot(content);
    build_cabin(world, content);
    let wardrobe =
        structure(world, content, Purpose::Wardrobe).expect("the cabin comes with a wardrobe");
    world.teleport(wardrobe.to_world(Vec3::new(0.0, 0.0, -1.2)));
    world.give(content, "green_tunic", 1);
    world.give(content, "straw_hat", 1);
    let tunic = content.id("green_tunic").expect("tunics exist");
    let overalls = content.id("overalls").expect("overalls exist");
    let straw_hat = content.id("straw_hat").expect("straw hats exist");
    let slot = world.slot_of(tunic).expect("the tunic is carried");
    world.send(ChangeClothes { slot: Some(slot) });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.look().0.outfit, "outfit-tunic.png");
    assert_eq!(world.carried(tunic), 0);
    assert_eq!(world.carried(overalls), 1, "the old clothes are kept");
    let slot = world.slot_of(straw_hat).expect("the hat is carried");
    world.send(ChangeClothes { slot: Some(slot) });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.look().0.hat.as_deref(), Some("hat-straw.png"));
    world.send(ChangeClothes { slot: None });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.look().0.hat, None);
    assert_eq!(world.carried(straw_hat), 1);
    world.send(ChangeClothes { slot: None });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.heard().notices.last(), Some(&Notice::NothingToWear));
}

/// The barber restyles for the village's price, in opening hours, and
/// does nothing the wardrobe does not offer.
fn visit_the_barber(world: &mut HostedWorld, content: &Catalog, chosen: &Look) {
    let wardrobe = &content.characters().wardrobe;
    let price = content.village().haircut.price;
    world.go_to_stall(content, "barber");
    world.set_time("10:00");
    let barber = content.shop(content.village().haircut.shop);
    assert!(barber.is_open(world.clock()), "the first day is a Monday");
    let coins = world.coins();
    let restyled = Restyle {
        hair: "hair-braid.png".to_owned(),
        hair_color: wardrobe.hair_colors[0],
    };
    world.send(Restyle {
        hair: "hair-mohawk.png".to_owned(),
        hair_color: wardrobe.hair_colors[0],
    });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(
        world.look().0.hair,
        chosen.hair,
        "the barber does not do that"
    );
    world.send(restyled.clone());
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.look().0.hair, "hair-braid.png");
    assert_eq!(world.look().0.hair_color, wardrobe.hair_colors[0]);
    assert_eq!(world.coins(), coins - price);
    world.send(restyled);
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.coins(), coins - price, "the same look costs nothing");
}

impl HostedWorld {
    /// How the host's character looks, and what it is called.
    fn look(&mut self) -> (Look, String) {
        let world = self.world();
        let (appearance, name) = world
            .query_filtered::<(&Appearance, &PlayerName), With<InputMarker<PlayerInput>>>()
            .single(world)
            .expect("the host has a character");
        (appearance.0.clone(), name.0.clone())
    }
}

/// Builds the host's cabin with its front 3.5 m ahead of them.
fn build_cabin(world: &mut HostedWorld, content: &Catalog) {
    let (feet, _) = world.character().expect("the host has a character");
    let ahead = feet + Vec3::new(0.0, 0.0, -3.5);
    let height = world
        .world()
        .resource::<Terrain>()
        .surface_below(ahead.with_y(60.0), 120.0)
        .expect("ground there");
    let deed = world.slot_holding(content, "cabin_deed");
    world.use_item(deed, ahead.with_y(height));
    assert!(
        structure(world, content, Purpose::Wardrobe).is_some(),
        "the cabin was built; the last notice was {:?}",
        world.heard().notices.last()
    );
}

fn structure(world: &mut HostedWorld, content: &Catalog, purpose: Purpose) -> Option<Structure> {
    let world = world.world();
    world
        .query::<&Structure>()
        .iter(world)
        .find(|structure| content.structure(structure.kind).purpose == purpose)
        .copied()
}

fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
