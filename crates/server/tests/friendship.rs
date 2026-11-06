//! Friendship is each player's own: two players befriend the grocer at
//! different rates, one by talking and one by talking and giving, and each
//! finds their own friendship in the save after a restart.

mod common;

use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use bevy::prelude::*;
use common::{Heard, client_app, free_udp_port, local_character_if_any, run_apps_until};
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_calendar::WorldTime;
use messoria_content::{Catalog, ItemId, Quality, Reaction, VillagerId};
use messoria_save::SaveDir;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    players::player_key,
    protocol::{
        ActionChannel, Belongings, Dialogue, GiveGift, PlayerId, PlayerInput, Position, TalkTo,
        Villager, WorldClock,
    },
    valley::Valley,
    village,
};

#[test]
fn two_players_befriend_the_grocer_at_their_own_pace_and_keep_it_after_a_restart() {
    let content = load_content().expect("the shipped content is valid");
    let marit = content.villager_id("marit").expect("Marit lives here");
    let tomato = content.id("tomato").expect("tomatoes exist");
    let save_dir = scratch_folder("friendship");
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut server = common::server_app(server_addr, setup);
    let (mut alice, mut bob) = (client_app(server_addr, 21), client_app(server_addr, 22));
    let (alice_id, bob_id) = (PeerId::Netcode(21), PeerId::Netcode(22));
    let both = &mut [&mut alice, &mut bob];
    run_apps_until(
        &mut server,
        both,
        "both clients to control a character",
        |_, clients| {
            clients
                .iter_mut()
                .all(|client| local_character_if_any(client).is_some())
        },
    );

    // Far from the village, villagers go straight to where their day has
    // them: Marit to her stall once the market opens.
    move_to(&mut server, both, 0, alice_id, Vec2::new(600.0, 600.0));
    move_to(&mut server, both, 1, bob_id, Vec2::new(600.0, 610.0));
    set_time(&mut server, "10:00");
    let stall = village::CENTER + Vec2::new(4.95, -5.15);
    run_apps_until(
        &mut server,
        both,
        "Marit to reach her stall",
        |server, _| villager_at(server, marit).xz().distance(stall) < 1.0,
    );
    let beside = stall + Vec2::new(0.0, 1.5);
    move_to(&mut server, both, 0, alice_id, beside);
    move_to(&mut server, both, 1, bob_id, beside + Vec2::new(1.0, 0.0));

    // Alice talks; Bob talks and gives her a tomato, which she loves.
    let answer = hear(&mut server, both, 0, TalkTo { villager: marit });
    assert_eq!(answer.points, 20, "a first talk earns twenty points");
    assert!(!answer.says.is_empty());

    give(&mut server, bob_id, &content, tomato);
    hear(&mut server, both, 1, TalkTo { villager: marit });
    let slot = slot_of(&mut server, both, 1, tomato);
    let thanks = hear(
        &mut server,
        both,
        1,
        GiveGift {
            villager: marit,
            slot,
        },
    );
    assert_eq!(thanks.reaction, Some(Reaction::Loved));
    assert_eq!(thanks.points, 100, "a talk and a loved gift make a heart");
    assert_eq!(
        thanks.given, None,
        "Marit gives her seeds at two hearts, not one"
    );

    // A second talk the same day warms nothing.
    let again = hear(&mut server, both, 0, TalkTo { villager: marit });
    assert_eq!(again.points, 20);
    let hearts = |client: &App| {
        client
            .world()
            .resource::<Heard>()
            .friendships
            .last()
            .cloned()
            .expect("friendships were sent")
    };
    assert_eq!(hearts(both[0]).0[0].points, 20);
    assert_eq!(hearts(both[1]).0[0].points, 100);
    assert!(hearts(both[1]).0[0].gave_today && hearts(both[1]).0[0].talked_today);

    // Everyone leaves, the world is saved, and each player's friendship is
    // their own.
    server.world_mut().write_message(AppExit::Success);
    server.update();
    drop(server);
    let saved = save_dir
        .load(&content)
        .expect("the save loads")
        .expect("the world was saved");
    let points = |peer: PeerId| {
        saved.players[&player_key(peer).expect("a key")]
            .friendships
            .iter()
            .find(|friendship| friendship.villager == marit)
            .map(|friendship| friendship.points)
    };
    assert_eq!(points(alice_id), Some(20));
    assert_eq!(points(bob_id), Some(100));
}

/// Has the `who`th client send `message` to the villager it stands by and
/// waits for what the villager says back.
fn hear<M: lightyear::prelude::Message>(
    server: &mut App,
    clients: &mut [&mut App],
    who: usize,
    message: M,
) -> Dialogue {
    let before = clients[who].world().resource::<Heard>().dialogues.len();
    send(clients[who], message);
    run_apps_until(server, clients, "the villager to answer", |_, clients| {
        clients[who].world().resource::<Heard>().dialogues.len() > before
    });
    clients[who].world().resource::<Heard>().dialogues[before].clone()
}

fn villager_at(server: &mut App, id: VillagerId) -> Vec3 {
    let world = server.world_mut();
    world
        .query::<(&Villager, &Position)>()
        .iter(world)
        .find(|(villager, _)| villager.0 == id)
        .map(|(_, position)| position.0)
        .expect("the villager is in the world")
}

fn set_time(server: &mut App, clock: &str) {
    let world = server.world_mut();
    let mut time = world
        .query::<&mut WorldClock>()
        .single_mut(world)
        .expect("the world has a clock");
    let today = time.0.day();
    time.0 = WorldTime::at(today, clock.parse().expect("valid time")).expect("within the day");
}

/// Moves the character of `peer`, the `who`th client's, onto the ground at
/// `place` and waits for that client to see it there. Every client keeps
/// updating meanwhile, or its connection would time out.
fn move_to(server: &mut App, clients: &mut [&mut App], who: usize, peer: PeerId, place: Vec2) {
    let height = server.world().resource::<Valley>().height(place);
    let world = server.world_mut();
    for (player, mut position) in world.query::<(&PlayerId, &mut Position)>().iter_mut(world) {
        if player.0 == peer {
            position.0 = Vec3::new(place.x, height, place.y);
        }
    }
    run_apps_until(server, clients, "the client to arrive", |_, clients| {
        local_character_if_any(clients[who]).is_some_and(|at| at.xz().distance(place) < 1.0)
    });
}

/// Puts one `item` in the hands of the character of `peer`.
fn give(server: &mut App, peer: PeerId, content: &Catalog, item: ItemId) {
    let world = server.world_mut();
    for (player, mut belongings) in world
        .query::<(&PlayerId, &mut Belongings)>()
        .iter_mut(world)
    {
        if player.0 == peer {
            belongings.0.add(content, item, Quality::Normal, 1, 0);
        }
    }
}

/// The slot the `who`th client sees `item` in, once it arrives.
fn slot_of(server: &mut App, clients: &mut [&mut App], who: usize, item: ItemId) -> u8 {
    let mut found = None;
    run_apps_until(
        server,
        clients,
        "the item to reach the client",
        |_, clients| {
            let world = clients[who].world_mut();
            found = world
                .query_filtered::<&Belongings, With<InputMarker<PlayerInput>>>()
                .iter(world)
                .next()
                .and_then(|belongings| {
                    (0..messoria_inventory::SLOTS).find(|&slot| {
                        belongings
                            .0
                            .slot(slot)
                            .is_some_and(|stack| stack.item == item)
                    })
                });
            found.is_some()
        },
    );
    u8::try_from(found.expect("found above")).expect("slots fit in u8")
}

fn send<M: lightyear::prelude::Message>(client: &mut App, message: M) {
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<M>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(message);
}

fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
