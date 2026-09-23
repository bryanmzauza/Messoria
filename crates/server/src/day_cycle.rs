//! The passing of days: advancing the clock, putting characters to sleep,
//! starting a new day when enough of them are asleep or at 02:00 regardless,
//! and choosing each day's weather.

use std::time::Duration;

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_calendar::{SleepRule, Weather};
use messoria_shared::{
    energy::{Energy, Rest},
    protocol::{Asleep, CurrentWeather, Notice, PlayerId, SleepRequest, SleepTally, WorldClock},
    tick,
};

use crate::{Beginning, WorldSeed, WorldStart, feedback::Tell, players::ControlledCharacter};

pub(crate) struct DayCyclePlugin {
    pub sleep_rule: SleepRule,
    pub minute_length: Duration,
}

impl Plugin for DayCyclePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DayRules {
            sleep_rule: self.sleep_rule,
            ticks_per_minute: tick::ticks_in(self.minute_length).max(1),
        })
        .add_message::<DayStarted>()
        .add_systems(Startup, start_clock)
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

fn start_clock(beginning: Res<Beginning>, seed: Res<WorldSeed>, mut commands: Commands) {
    let (now, how) = match &beginning.0 {
        WorldStart::New { start_time, .. } => (*start_time, "a new world starts"),
        WorldStart::Resume(saved) => (saved.world.clock, "the world resumes"),
    };
    info!("{how} on {} at {}", now.date(), now.clock());
    commands.spawn((
        Name::new("World clock"),
        WorldClock(now),
        CurrentWeather(Weather::on(seed.0, now.day())),
        SleepTally::default(),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn handle_sleep_requests(
    clock: Single<&WorldClock>,
    mut clients: Query<(&mut MessageReceiver<SleepRequest>, &ControlledCharacter)>,
    mut tell: MessageWriter<Tell>,
    mut commands: Commands,
) {
    for (mut requests, character) in &mut clients {
        for request in requests.receive() {
            match request {
                SleepRequest::Sleep if clock.0.is_bedtime() => {
                    commands.entity(character.0).insert(Asleep);
                }
                SleepRequest::Sleep => {
                    tell.write(Tell {
                        character: character.0,
                        notice: Notice::TooEarlyToSleep,
                    });
                }
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
    seed: Res<WorldSeed>,
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
    weather.set_if_neq(CurrentWeather(Weather::on(seed.0, clock.0.day())));
    *ticks_this_minute = 0;
    days.write(DayStarted(clock.0.day()));
    info!("{} begins, {:?}", clock.0.date(), weather.0);
}
