//! Farm lots: the first is free and the next cost the village's price; the
//! ground is worked only on a lot of one's own, never under a fence; two
//! players on neighboring lots cannot touch each other's land unless
//! invited; and lots and invitations survive a restart.

mod common;

use std::{
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Instant,
};

use bevy::prelude::*;
use common::{HostedWorld, client_app, free_udp_port, local_character_if_any, run_apps_until};
use lightyear::prelude::*;
use messoria_calendar::WorldTime;
use messoria_content::Purpose;
use messoria_save::SaveDir;
use messoria_server::WorldSetup;
use messoria_shared::{
    content::load_content,
    fields::tile_at,
    lots::{GATE_WIDTH, LotId, lot},
    players::player_key,
    protocol::{
        ActionChannel, ClaimLot, Field, Happened, InviteToLot, ItemAction, LotClaim, Notice,
        PlayerId, Position, Structure, Uninvite, UseItem,
    },
    tools,
    valley::Valley,
};

#[test]
fn the_first_lot_is_free_and_the_next_costs_the_village_price() {
    let content = load_content().expect("the shipped content is valid");
    let price = content.village().lots.price;
    let mut world = HostedWorld::new(content.clone());
    let coins = world.coins();

    world.claim(&content, LotId(3));
    assert_eq!(world.coins(), coins, "the first lot is free");
    assert_eq!(
        world
            .claims()
            .iter()
            .map(|claim| claim.lot)
            .collect::<Vec<_>>(),
        [LotId(3)]
    );
    assert_eq!(world.claims()[0].owner, "host");
    assert!(
        world
            .heard()
            .happenings
            .iter()
            .any(|happening| happening.what == Happened::ClaimedLot)
    );

    world.claim(&content, LotId(3));
    assert_eq!(world.heard().notices.last(), Some(&Notice::LotTaken));

    world.claim(&content, LotId(4));
    assert_eq!(world.heard().notices.last(), Some(&Notice::LotCosts(price)));
    assert_eq!(world.claims().len(), 1);

    let world_mut = world.world();
    world_mut
        .query_filtered::<&mut messoria_shared::protocol::Money, With<PlayerId>>()
        .single_mut(world_mut)
        .expect("the host")
        .0 = messoria_economy::Wallet::with(coins + price);
    world.claim(&content, LotId(4));
    assert_eq!(world.claims().len(), 2);
    assert_eq!(world.coins(), coins, "the second lot cost the price");

    // Claims are made at the board, nowhere else.
    world.teleport_to(lot(LotId(4)).center());
    world.send(ClaimLot { lot: LotId(5) });
    world.run(common::PAUSE_BETWEEN_USES);
    assert_eq!(world.claims().len(), 2);
}

#[test]
fn the_ground_is_worked_only_on_a_lot_of_ones_own() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let (hoe, shovel) = (
        world.slot_holding(&content, "hoe"),
        world.slot_holding(&content, "shovel"),
    );

    // The road south of the village is on no lot.
    world.teleport_to(Vec2::new(0.0, 60.0));
    let target = world.ground_ahead();
    world.use_item(hoe, target);
    assert_eq!(world.heard().notices.last(), Some(&Notice::OnlyOnYourLot));
    world.use_item(shovel, target);
    assert_eq!(world.heard().notices.last(), Some(&Notice::OnlyOnYourLot));
    assert!(world.field(tile_at(target)).is_none());

    let lot = world.settle_on_a_lot(&content);
    let target = world.ground_ahead();
    world.use_item(hoe, target);
    assert!(world.field(tile_at(target)).is_some(), "tilled on the lot");

    // The fence keeps its ground, on both sides; the gate is in the middle
    // of this side, so the fence is tried away from it.
    let fence = Vec2::new(lot.min.x, lot.center().y + 12.0);
    world.teleport_to(fence + Vec2::X * 1.5);
    let against = world.ground_at(fence + Vec2::X * 0.4);
    world.use_item(hoe, against);
    assert_eq!(world.heard().notices.last(), Some(&Notice::SceneryInTheWay));
    world.use_item(shovel, against);
    assert_eq!(world.heard().notices.last(), Some(&Notice::SceneryInTheWay));
    let outside = world.ground_at(fence - Vec2::X * 1.5);
    world.use_item(hoe, outside);
    assert_eq!(world.heard().notices.last(), Some(&Notice::OnlyOnYourLot));

    // The gate is open, and the fence stands in the way.
    let valley_height =
        |world: &mut HostedWorld, at: Vec2| world.world().resource::<Valley>().height(at);
    let inward = (lot.center() - lot.gate).normalize();
    let through_the_gate = lot.gate - inward * 1.0;
    let feet = Vec3::new(
        through_the_gate.x,
        valley_height(&mut world, through_the_gate),
        through_the_gate.y,
    );
    let push_out = |world: &mut HostedWorld, feet: Vec3| {
        world
            .world()
            .resource::<messoria_shared::obstacles::Obstacles>()
            .push_out(feet, 0.3, 1.8)
    };
    assert_eq!(push_out(&mut world, feet), feet, "the gateway is open");
    let into_the_fence = lot.gate + lot.along_road * (GATE_WIDTH / 2.0 + 1.5);
    let feet = Vec3::new(
        into_the_fence.x,
        valley_height(&mut world, into_the_fence),
        into_the_fence.y,
    );
    assert_ne!(push_out(&mut world, feet), feet, "the fence blocks");
}

#[test]
fn neighbors_work_each_others_lots_only_when_invited_and_that_outlasts_a_restart() {
    let content = load_content().expect("the shipped content is valid");
    let save_dir = scratch_folder("neighbors");
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_udp_port()));
    let setup = WorldSetup::open(save_dir.clone(), &content, WorldTime::FIRST_DAWN)
        .expect("an empty folder opens a new world");
    let mut server = common::server_app(server_addr, setup);
    let (mut alice, mut bob) = (client_app(server_addr, 11), client_app(server_addr, 12));
    let (alice_id, bob_id) = (PeerId::Netcode(11), PeerId::Netcode(12));
    let (alice_key, bob_key) = (
        player_key(alice_id).expect("a key"),
        player_key(bob_id).expect("a key"),
    );
    run_apps_until(
        &mut server,
        &mut [&mut alice, &mut bob],
        "both clients to control a character",
        |_, clients| {
            clients
                .iter_mut()
                .all(|client| local_character_if_any(client).is_some())
        },
    );

    // Neighbors across the lane: lots 1 and 2 lie along the same road.
    claim(&mut server, &mut alice, alice_id, LotId(0));
    claim(&mut server, &mut bob, bob_id, LotId(1));
    let hoe = 1;

    // Bob tries Alice's lot, and is refused until invited.
    let on_alices = lot(LotId(0)).center();
    move_to(&mut server, &mut bob, bob_id, on_alices);
    let field = ground_at(&mut server, on_alices + Vec2::new(0.0, -2.0));
    let refusal = till_and_hear(&mut server, &mut bob, hoe, field);
    assert_eq!(refusal, Notice::NotYourLot);

    invite(&mut server, &mut alice, bob_id, &bob_key);
    till(&mut server, &mut bob, hoe, field);
    run_apps_until(
        &mut server,
        &mut [&mut alice, &mut bob],
        "Bob to till on Alice's lot",
        |server, _| has_field(server, field),
    );

    // Alice, who was not invited back, cannot touch Bob's lot.
    let on_bobs = lot(LotId(1)).center();
    move_to(&mut server, &mut alice, alice_id, on_bobs);
    let bobs_field = ground_at(&mut server, on_bobs + Vec2::new(0.0, -2.0));
    let refusal = till_and_hear(&mut server, &mut alice, hoe, bobs_field);
    assert_eq!(refusal, Notice::NotYourLot);
    assert!(!has_field(&mut server, bobs_field));

    // Taking the invitation back takes the right away.
    send(
        &mut alice,
        Uninvite {
            lot: LotId(0),
            player: bob_key.clone(),
        },
    );
    run_apps_until(
        &mut server,
        &mut [&mut alice, &mut bob],
        "the invitation to be withdrawn",
        |server, _| claim_on(server, LotId(0)).invited.is_empty(),
    );
    let farther = ground_at(&mut server, on_alices + Vec2::new(0.0, -4.0));
    let refusal = till_and_hear(&mut server, &mut bob, hoe, farther);
    assert_eq!(refusal, Notice::NotYourLot);

    // Invited again, then everyone leaves and the world is saved.
    invite(&mut server, &mut alice, bob_id, &bob_key);
    server.world_mut().write_message(AppExit::Success);
    server.update();
    drop(server);

    let setup =
        WorldSetup::open(save_dir, &content, WorldTime::FIRST_DAWN).expect("the saved world loads");
    let mut resumed = common::server_app(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), setup);
    resumed.update();
    let mut claims: Vec<LotClaim> = {
        let world = resumed.world_mut();
        world.query::<&LotClaim>().iter(world).cloned().collect()
    };
    claims.sort_by_key(|claim| claim.lot);
    assert_eq!(
        claims,
        [
            LotClaim {
                lot: LotId(0),
                owner: alice_key,
                invited: vec![bob_key.clone()],
            },
            LotClaim {
                lot: LotId(1),
                owner: bob_key,
                invited: Vec::new(),
            },
        ]
    );
}

/// Has `owner` invite `guest` to lot 1 and waits for the server to note it.
fn invite(server: &mut App, owner: &mut App, guest: PeerId, guest_key: &str) {
    send(
        owner,
        InviteToLot {
            lot: LotId(0),
            player: guest,
        },
    );
    run_apps_until(
        server,
        &mut [owner],
        "the invitation to be noted",
        |server, _| claim_on(server, LotId(0)).invited == [guest_key.to_owned()],
    );
}

/// Has `client` use the hoe at `target` and waits for the server's notice
/// about it.
fn till_and_hear(server: &mut App, client: &mut App, hoe: u8, target: Vec3) -> Notice {
    let heard = |client: &App| client.world().resource::<common::Heard>().notices.len();
    let before = heard(client);
    till(server, client, hoe, target);
    run_apps_until(
        server,
        &mut [client],
        "the server's notice",
        |_, clients| heard(clients[0]) > before,
    );
    *client
        .world()
        .resource::<common::Heard>()
        .notices
        .last()
        .expect("a notice arrived")
}

/// Has `client`, playing as `peer`, claim `lot` at the notice board.
fn claim(server: &mut App, client: &mut App, peer: PeerId, lot: LotId) {
    let content = load_content().expect("the shipped content is valid");
    let board = {
        let world = server.world_mut();
        *world
            .query::<&Structure>()
            .iter(world)
            .find(|structure| content.structure(structure.kind).purpose == Purpose::NoticeBoard)
            .expect("the village has a notice board")
    };
    let front = board.to_world(Vec3::new(0.0, 0.0, -1.3));
    move_to(server, client, peer, front.xz());
    send(client, ClaimLot { lot });
    run_apps_until(
        server,
        &mut [client],
        "the lot to be claimed",
        |server, _| {
            server
                .world_mut()
                .query::<&LotClaim>()
                .iter(server.world())
                .any(|claim| claim.lot == lot)
        },
    );
}

/// Moves the character of `peer` onto the ground at `place` and waits for
/// `client` to see it there.
fn move_to(server: &mut App, client: &mut App, peer: PeerId, place: Vec2) {
    let feet = ground_at(server, place);
    let world = server.world_mut();
    for (player, mut position) in world.query::<(&PlayerId, &mut Position)>().iter_mut(world) {
        if player.0 == peer {
            position.0 = feet;
        }
    }
    run_apps_until(
        server,
        &mut [client],
        "the client to arrive",
        |_, clients| {
            local_character_if_any(clients[0]).is_some_and(|at| at.xz().distance(place) < 1.0)
        },
    );
}

fn ground_at(server: &mut App, place: Vec2) -> Vec3 {
    let height = server.world().resource::<Valley>().height(place);
    Vec3::new(place.x, height, place.y)
}

fn send<M: lightyear::prelude::Message>(client: &mut App, message: M) {
    client
        .world_mut()
        .query_filtered::<&mut MessageSender<M>, With<Client>>()
        .single_mut(client.world_mut())
        .expect("one client connection")
        .send::<ActionChannel>(message);
}

/// Uses the hoe at `target`, after the pause the server asks between uses.
fn till(server: &mut App, client: &mut App, hoe: u8, target: Vec3) {
    let rested = Instant::now() + tools::USE_INTERVAL;
    run_apps_until(server, &mut [client], "the use interval to pass", |_, _| {
        Instant::now() >= rested
    });
    send(
        client,
        UseItem {
            slot: hoe,
            action: ItemAction::Primary,
            target: Some(target),
        },
    );
}

fn has_field(server: &mut App, at: Vec3) -> bool {
    let tile = tile_at(at);
    let world = server.world_mut();
    world
        .query::<&Field>()
        .iter(world)
        .any(|field| field.tile == tile)
}

fn claim_on(server: &mut App, lot: LotId) -> LotClaim {
    let world = server.world_mut();
    world
        .query::<&LotClaim>()
        .iter(world)
        .find(|claim| claim.lot == lot)
        .cloned()
        .expect("the lot is claimed")
}

fn scratch_folder(name: &str) -> SaveDir {
    let root: PathBuf = std::env::temp_dir()
        .join(format!("messoria-server-tests-{}", std::process::id()))
        .join(name);
    let _ = fs::remove_dir_all(&root);
    SaveDir::new(root)
}
