//! Players are told why the server refused what they asked for, and everyone
//! hears about what they did.

mod common;

use common::HostedWorld;
use messoria_shared::{
    content::load_content,
    protocol::{Happened, Notice, SleepRequest},
};

#[test]
fn refused_actions_tell_the_player_why() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());

    world.set_time("10:00");
    world.send(SleepRequest::Sleep);
    world.run_until("the refusal to arrive", |world| {
        !world.heard().notices.is_empty()
    });
    assert_eq!(world.heard().notices, [Notice::TooEarlyToSleep]);

    let target = world.ground_ahead();
    let (hoe, seeds) = (
        world.slot_holding(&content, "hoe"),
        world.slot_holding(&content, "turnip_seeds"),
    );
    world.use_item(hoe, target);
    world.use_item(seeds, target);
    world.harvest(target);
    world.run_until("the refusal to arrive", |world| {
        world.heard().notices.len() == 2
    });
    assert_eq!(world.heard().notices[1], Notice::NotRipe);
}

#[test]
fn work_on_a_field_is_shown_where_it_happened() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());

    let target = world.ground_ahead();
    let (hoe, seeds) = (
        world.slot_holding(&content, "hoe"),
        world.slot_holding(&content, "turnip_seeds"),
    );
    world.use_item(hoe, target);
    world.use_item(seeds, target);
    world.run_until("both happenings to arrive", |world| {
        world.heard().happenings.len() == 2
    });

    let heard = world.heard();
    let what: Vec<_> = heard
        .happenings
        .iter()
        .map(|happening| happening.what)
        .collect();
    assert_eq!(what, [Happened::Tilled, Happened::Planted]);
    for happening in &heard.happenings {
        assert!(
            happening.at.distance(target) < 1.0,
            "{happening:?} is shown at the field at {target}"
        );
    }
    assert!(heard.notices.is_empty());
}
