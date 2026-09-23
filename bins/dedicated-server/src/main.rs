use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use messoria_server::ServerPlugin;
use messoria_shared::tick::tick_duration;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_duration())),
            LogPlugin::default(),
            bevy::app::TerminalCtrlCHandlerPlugin,
            ServerPlugin,
        ))
        .run()
}
