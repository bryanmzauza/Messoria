//! The authoritative game server.
//!
//! [`ServerPlugin`] holds game logic only. It never adds runtime plugins such
//! as a window, a schedule runner or logging, so the same plugin runs in the
//! dedicated server binary and inside the game client when a player hosts
//! their own world. It expects `SharedPlugin` with the `Server` or `Host` role.
//!
//! A world either starts new or resumes from its save, which the executable
//! reads before building the app, as it does content, so a damaged save stops
//! the program instead of being overwritten.

mod connections;
mod day_cycle;
mod feedback;
mod fields;
mod inventory;
mod market;
mod players;
mod saving;
mod scenery;
mod terrain;
mod village;

use std::{net::SocketAddr, time::Duration};

use bevy::prelude::*;
use messoria_calendar::{SleepRule, WorldTime};
use messoria_content::Catalog;
use messoria_save::{SaveDir, SaveError, SavedWorld};

pub struct ServerPlugin {
    /// Address the server listens on for clients.
    pub bind_addr: SocketAddr,
    /// How many players must sleep to end the day.
    pub sleep_rule: SleepRule,
    /// Real time one game minute lasts; `messoria_calendar::GAME_MINUTE` for
    /// normal play, shorter to watch days go by quickly.
    pub minute_length: Duration,
    pub world: WorldSetup,
}

/// The world a server runs, and where it keeps it.
#[derive(Clone, Debug)]
pub struct WorldSetup {
    pub start: WorldStart,
    /// Folder the world is saved in; `None` runs a world that is never
    /// saved, for tests.
    pub save_dir: Option<SaveDir>,
}

/// Where a world comes from.
#[derive(Clone, Debug)]
pub enum WorldStart {
    /// A new world, with the seed of everything that varies between worlds
    /// and the time its clock starts at.
    New { seed: u64, start_time: WorldTime },
    /// A world read back from its save.
    Resume(Box<SavedWorld>),
}

impl WorldSetup {
    /// A new world that is never saved, as tests run.
    pub fn fresh(seed: u64, start_time: WorldTime) -> Self {
        Self {
            start: WorldStart::New { seed, start_time },
            save_dir: None,
        }
    }

    /// The world saved in `save_dir`, or a new world with a random seed, to
    /// be saved there, if nothing has been saved yet.
    ///
    /// # Errors
    ///
    /// If a save exists but cannot be read.
    pub fn open(
        save_dir: SaveDir,
        content: &Catalog,
        new_start_time: WorldTime,
    ) -> Result<Self, SaveError> {
        let start = match save_dir.load(content)? {
            Some(saved) => WorldStart::Resume(Box::new(saved)),
            None => WorldStart::New {
                seed: rand::random(),
                start_time: new_start_time,
            },
        };
        Ok(Self {
            start,
            save_dir: Some(save_dir),
        })
    }
}

impl WorldStart {
    fn seed(&self) -> u64 {
        match self {
            Self::New { seed, .. } => *seed,
            Self::Resume(saved) => saved.world.seed,
        }
    }
}

/// How the world began. Each part of the server sets up its share of the
/// world from it at startup; it is dropped afterwards.
#[derive(Resource)]
struct Beginning(WorldStart);

/// Seeds everything that varies from world to world, such as the weather.
#[derive(Resource, Clone, Copy)]
struct WorldSeed(u64);

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldSeed(self.world.start.seed()))
            .insert_resource(Beginning(self.world.start.clone()))
            .add_systems(PostStartup, forget_beginning)
            .add_plugins((
                connections::ConnectionsPlugin {
                    bind_addr: self.bind_addr,
                },
                players::PlayersPlugin,
                feedback::FeedbackPlugin,
                inventory::InventoryPlugin,
                fields::FieldsPlugin,
                market::MarketPlugin,
                village::VillagePlugin,
                scenery::SceneryPlugin,
                terrain::TerrainPlugin,
                day_cycle::DayCyclePlugin {
                    sleep_rule: self.sleep_rule,
                    minute_length: self.minute_length,
                },
                saving::SavingPlugin {
                    save_dir: self.world.save_dir.clone(),
                    new_world: matches!(self.world.start, WorldStart::New { .. }),
                },
            ));
    }
}

fn forget_beginning(mut commands: Commands) {
    commands.remove_resource::<Beginning>();
}
