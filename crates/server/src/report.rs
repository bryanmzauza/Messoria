//! How the server keeps up: every minute it logs how long its updates took,
//! on average and at worst, against the length of a tick, and how much of
//! the world it has loaded for how many players.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use messoria_shared::{protocol::PlayerId, tick::tick_duration};

use crate::terrain::LoadedColumns;

/// Real time between two reports.
const REPORT_INTERVAL: Duration = Duration::from_mins(1);

pub(crate) struct ReportPlugin;

impl Plugin for ReportPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UpdateTimes::new())
            .add_systems(First, start_timing)
            .add_systems(Last, stop_timing);
    }
}

#[derive(Resource)]
struct UpdateTimes {
    /// When the update under way started.
    started: Instant,
    count: u32,
    total: Duration,
    worst: Duration,
    next_report: Instant,
}

impl UpdateTimes {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            count: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
            next_report: now + REPORT_INTERVAL,
        }
    }
}

fn start_timing(mut times: ResMut<UpdateTimes>) {
    times.started = Instant::now();
}

fn stop_timing(
    mut times: ResMut<UpdateTimes>,
    loaded: Res<LoadedColumns>,
    players: Query<(), With<PlayerId>>,
) {
    let now = Instant::now();
    let took = now - times.started;
    times.count += 1;
    times.total += took;
    times.worst = times.worst.max(took);
    if now < times.next_report {
        return;
    }
    let mean = times.total / times.count.max(1);
    info!(
        "updates took {:.1} ms on average and {:.1} ms at worst, of a {:.1} ms tick; \
         {} columns of the valley are loaded for {} players",
        mean.as_secs_f64() * 1000.0,
        times.worst.as_secs_f64() * 1000.0,
        tick_duration().as_secs_f64() * 1000.0,
        loaded.len(),
        players.iter().count(),
    );
    *times = UpdateTimes::new();
}
