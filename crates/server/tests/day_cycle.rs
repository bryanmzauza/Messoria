//! The day advances correctly under each sleep rule.
//!
//! These drive the server's clock directly, one simulation tick per update,
//! with characters standing in for connected players.

use std::net::{Ipv4Addr, SocketAddr};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use lightyear::prelude::PeerId;
use messoria_calendar::{GAME_MINUTE, MINUTES_PER_DAY, SleepRule, WorldTime};
use messoria_content::Quality;
use messoria_inventory::Inventory;
use messoria_server::ServerPlugin;
use messoria_shared::{
    SharedPlugin,
    content::load_content,
    energy::{Energy, MAX_ENERGY},
    network::NetworkRole,
    protocol::{Asleep, Belongings, PlayerId, WorldClock},
    tick::ticks_in,
};

/// 22:00 on the first day.
const LATE_EVENING: u16 = 16 * 60;

#[test]
fn a_hosted_world_waits_for_everyone_to_sleep() {
    let mut world = World::new(SleepRule::Everyone);
    world.set_clock(0, LATE_EVENING);
    let early_sleeper = world.add_player(Some(Rest::Asleep), 30);
    let night_owl = world.add_player(None, 30);

    world.advance_minutes(5);
    assert_eq!(world.clock().day(), 0, "the day ended with a player awake");

    world.app.world_mut().entity_mut(night_owl).insert(Asleep);
    world.advance_minutes(1);
    assert_eq!(world.clock().day(), 1);
    assert!(world.clock().minutes_since_dawn() <= 1);
    for player in [early_sleeper, night_owl] {
        assert_eq!(world.energy(player), MAX_ENERGY);
        assert!(!world.is_asleep(player));
    }
}

#[test]
fn a_dedicated_server_needs_only_a_share_of_sleepers() {
    let mut world = World::new(SleepRule::Share { percent: 50 });
    world.set_clock(0, LATE_EVENING);
    let sleepers = [
        world.add_player(Some(Rest::Asleep), 40),
        world.add_player(Some(Rest::Asleep), 40),
    ];
    let night_owl = world.add_player(None, 40);

    world.advance_minutes(1);
    assert_eq!(world.clock().day(), 1);
    for sleeper in sleepers {
        assert_eq!(world.energy(sleeper), MAX_ENERGY);
    }
    assert_eq!(
        world.energy(night_owl),
        MAX_ENERGY - 40,
        "staying up is not rest"
    );
}

#[test]
fn players_still_awake_at_two_pass_out() {
    let mut world = World::new(SleepRule::Everyone);
    world.set_clock(0, MINUTES_PER_DAY - 2);
    let worn_out = world.add_player(None, 80);

    world.advance_minutes(3);
    assert_eq!(world.clock().day(), 1);
    assert_eq!(world.energy(worn_out), MAX_ENERGY / 2);
}

#[test]
fn food_spoils_into_compost_at_dawn() {
    let content = load_content().expect("the shipped content is valid");
    let berries = content.id("wild_berries").expect("berries exist");
    let compost = content.item(berries).spoils_into.expect("berries spoil");
    let shelf_life = u32::from(content.item(berries).shelf_life.expect("berries spoil"));

    let mut world = World::new(SleepRule::Everyone);
    let mut basket = Inventory::default();
    basket.add(&content, berries, Quality::Normal, 4, 0);
    let player = world.add_player(None, 0);
    world
        .app
        .world_mut()
        .entity_mut(player)
        .insert(Belongings(basket));

    world.set_clock(shelf_life - 1, MINUTES_PER_DAY - 1);
    world.advance_minutes(1);

    assert_eq!(world.clock().day(), shelf_life);
    let basket = &world
        .app
        .world()
        .get::<Belongings>(player)
        .expect("still carried")
        .0;
    assert_eq!(basket.count(berries), 0);
    assert_eq!(basket.count(compost), 4);
}

#[test]
fn time_runs_one_game_minute_per_real_second() {
    let mut world = World::new(SleepRule::Everyone);
    world.add_player(None, 0);

    world.advance_minutes(90);
    assert_eq!(world.clock(), WorldTime::new(0, 90).unwrap());
}

enum Rest {
    Asleep,
}

struct World {
    app: App,
    players: u64,
    ticks_per_minute: u32,
}

impl World {
    fn new(sleep_rule: SleepRule) -> Self {
        let ticks_per_minute = ticks_in(GAME_MINUTE);
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            SharedPlugin {
                role: NetworkRole::Server,
                content: load_content().expect("the shipped content is valid"),
            },
            ServerPlugin {
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                sleep_rule,
                start_time: WorldTime::FIRST_DAWN,
                minute_length: GAME_MINUTE,
            },
        ))
        .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        app.finish();
        app.cleanup();
        // The first update only starts the clock; no time passes in it.
        app.update();
        Self {
            app,
            players: 0,
            ticks_per_minute,
        }
    }

    fn add_player(&mut self, rest: Option<Rest>, spent_energy: u16) -> Entity {
        self.players += 1;
        let mut energy = Energy::FULL;
        assert!(energy.try_spend(spent_energy));
        let mut player = self
            .app
            .world_mut()
            .spawn((PlayerId(PeerId::Local(self.players)), energy));
        if let Some(Rest::Asleep) = rest {
            player.insert(Asleep);
        }
        player.id()
    }

    fn set_clock(&mut self, day: u32, minute: u16) {
        let time = WorldTime::new(day, minute).expect("a time within the day");
        self.clock_mut().0 = time;
    }

    fn clock(&mut self) -> WorldTime {
        self.clock_mut().0
    }

    fn clock_mut(&mut self) -> Mut<'_, WorldClock> {
        self.app
            .world_mut()
            .query::<&mut WorldClock>()
            .single_mut(self.app.world_mut())
            .expect("the server keeps one clock")
    }

    fn advance_minutes(&mut self, minutes: u32) {
        for _ in 0..minutes * self.ticks_per_minute {
            self.app.update();
        }
    }

    fn energy(&self, player: Entity) -> u16 {
        self.app
            .world()
            .get::<Energy>(player)
            .expect("players have energy")
            .current()
    }

    fn is_asleep(&self, player: Entity) -> bool {
        self.app.world().get::<Asleep>(player).is_some()
    }
}
