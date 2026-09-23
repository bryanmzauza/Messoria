//! The world time as this client sees it.
//!
//! The server moves its clock once per game minute. Between updates the
//! client advances its own copy with real time, capped at one minute, so the
//! sun and the clock move smoothly instead of in steps.

use std::time::Duration;

use bevy::prelude::*;
use messoria_calendar::{GAME_MINUTE, WorldTime};
use messoria_shared::protocol::WorldClock;

pub(crate) struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalClock>()
            .add_systems(PreUpdate, follow_world_clock);
    }
}

#[derive(Resource, Default)]
pub(crate) struct LocalClock {
    /// The last time the server reported, once it has.
    time: Option<WorldTime>,
    since_update: Duration,
}

impl LocalClock {
    pub(crate) fn time(&self) -> Option<WorldTime> {
        self.time
    }

    /// Hours since midnight at the start of the day, including the fraction
    /// of the current game minute. Continues past 24 after midnight.
    pub(crate) fn hours(&self) -> Option<f32> {
        let elapsed = (self.since_update.as_secs_f32() / GAME_MINUTE.as_secs_f32()).min(1.0);
        self.time.map(|time| time.hours() + elapsed / 60.0)
    }
}

fn follow_world_clock(
    time: Res<Time>,
    clock: Query<&WorldClock, Changed<WorldClock>>,
    mut local: ResMut<LocalClock>,
) {
    if let Ok(clock) = clock.single() {
        local.time = Some(clock.0);
        local.since_update = Duration::ZERO;
    } else {
        local.since_update += time.delta();
    }
}
