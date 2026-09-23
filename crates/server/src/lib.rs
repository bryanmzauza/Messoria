//! The authoritative game server.
//!
//! [`ServerPlugin`] holds game logic only. It never adds runtime plugins such
//! as a window, a schedule runner or logging, so the same plugin can run in the
//! dedicated server binary or inside the game client when a player opens their
//! world to friends.

use bevy::prelude::*;
use messoria_shared::{SharedPlugin, tick::TICK_RATE_HZ};

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<SharedPlugin>() {
            app.add_plugins(SharedPlugin);
        }

        app.init_resource::<ServerTick>()
            .add_systems(Startup, log_startup)
            .add_systems(FixedUpdate, advance_tick);
    }
}

/// Number of simulation ticks the server has run since it started.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerTick(pub u64);

fn log_startup() {
    info!("server simulation running at {TICK_RATE_HZ} Hz");
}

fn advance_tick(mut tick: ResMut<ServerTick>) {
    tick.0 += 1;
}

#[cfg(test)]
mod tests {
    use bevy::time::TimeUpdateStrategy;

    use super::*;

    #[test]
    fn server_advances_one_tick_per_fixed_timestep() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ServerPlugin))
            .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));

        // The first update only initializes the clock; no time elapses in it.
        app.update();
        let start = *app.world().resource::<ServerTick>();

        for _ in 0..10 {
            app.update();
        }

        let end = *app.world().resource::<ServerTick>();
        assert_eq!(end.0 - start.0, 10);
    }
}
