//! The passing of days: advancing the clock, putting characters to sleep,
//! starting a new day when enough of them are asleep or at 02:00 regardless,
//! and choosing each day's weather.

use std::time::Duration;

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_calendar::{SleepRule, Weather, WorldTime};
use messoria_shared::{
    energy::{Energy, Rest},
    protocol::{Asleep, CurrentWeather, PlayerId, SleepRequest, SleepTally, WorldClock},
    tick,
};

use crate::players::ControlledCharacter;

/// Seeds the weather. Fixed until worlds are saved and each gets its own.
const WORLD_SEED: u64 = 0x6d65_7373_6f72_6961;

pub(crate) struct DayCyclePlugin {
    pub sleep_rule: SleepRule,
    pub start_time: WorldTime,
    pub minute_length: Duration,
}

impl Plugin for DayCyclePlugin {
    fn build(&self, app: &mut App) {
        let start_time = self.start_time;
        app.insert_resource(DayRules {
            sleep_rule: self.sleep_rule,
            ticks_per_minute: tick::ticks_in(self.minute_length).max(1),
        })
        .add_message::<DayStarted>()
        .add_systems(Startup, move |commands: Commands| {
            start_clock(commands, start_time);
        })
        .add_systems(
            PreUpdate,
            handle_sleep_requests.after(MessageSystems::Receive),
        )
        .add_systems(FixedUpdate, run_clock.in_set(ClockSystems));
    }
}

#[derive(Resource)]
struct DayRules {
    sleep_rule: SleepRule,
    ticks_per_minute: u32,
}

/// Advances the world clock and ends days.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ClockSystems;

/// A new day began at dawn; carries its day number. Its weather is already
/// set on the clock.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct DayStarted(pub u32);

fn start_clock(mut commands: Commands, start_time: WorldTime) {
    commands.spawn((
        Name::new("World clock"),
        WorldClock(start_time),
        CurrentWeather(Weather::on(WORLD_SEED, start_time.day())),
        SleepTally::default(),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn handle_sleep_requests(
    clock: Single<&WorldClock>,
    mut clients: Query<(&mut MessageReceiver<SleepRequest>, &ControlledCharacter)>,
    mut commands: Commands,
) {
    for (mut requests, character) in &mut clients {
        for request in requests.receive() {
            match request {
                SleepRequest::Sleep if clock.0.is_bedtime() => {
                    commands.entity(character.0).insert(Asleep);
                }
                SleepRequest::Sleep => debug!("rejected sleep request before bedtime"),
                SleepRequest::Wake => {
                    commands.entity(character.0).remove::<Asleep>();
                }
            }
        }
    }
}

/// Advances the clock one game minute at a time and ends the day when the
/// sleep rule is met or the day runs out.
fn run_clock(
    rules: Res<DayRules>,
    clock: Single<(&mut WorldClock, &mut CurrentWeather, &mut SleepTally)>,
    mut characters: Query<(Entity, &mut Energy, Has<Asleep>), With<PlayerId>>,
    mut ticks_this_minute: Local<u32>,
    mut days: MessageWriter<DayStarted>,
    mut commands: Commands,
) {
    let (mut clock, mut weather, mut tally) = clock.into_inner();

    let online = u32::try_from(characters.iter().count()).unwrap_or(u32::MAX);
    let asleep = u32::try_from(characters.iter().filter(|(_, _, asleep)| *asleep).count())
        .unwrap_or(u32::MAX);
    tally.set_if_neq(SleepTally {
        asleep,
        required: rules.sleep_rule.required(online),
    });

    let mut out_of_time = false;
    *ticks_this_minute += 1;
    if *ticks_this_minute >= rules.ticks_per_minute {
        *ticks_this_minute = 0;
        match clock.0.next_minute() {
            Some(next) => clock.0 = next,
            None => out_of_time = true,
        }
    }
    if !out_of_time && !rules.sleep_rule.ends_day(asleep, online) {
        return;
    }

    // Whoever is still awake when the day runs out collapses where they stand.
    for (character, mut energy, asleep) in &mut characters {
        if asleep {
            *energy = energy.after(Rest::Slept);
            commands.entity(character).remove::<Asleep>();
        } else if out_of_time {
            *energy = energy.after(Rest::PassedOut);
        }
    }
    clock.0 = clock.0.next_dawn();
    weather.set_if_neq(CurrentWeather(Weather::on(WORLD_SEED, clock.0.day())));
    *ticks_this_minute = 0;
    days.write(DayStarted(clock.0.day()));
    info!("{} begins, {:?}", clock.0.date(), weather.0);
}
