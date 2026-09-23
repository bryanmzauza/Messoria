//! Saving the world: as soon as a new world is created, every few minutes,
//! at each dawn, and when the server shuts down.
//!
//! A save writes the world file, the terrain that changed since the world
//! was generated (only if more of it changed since the last save), every
//! player in the world, and every player who left since the last save.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use messoria_save::{FieldState, SaveDir, SaveError, WorldState};
use messoria_shared::{
    content::Content,
    protocol::{Crop, Fertilized, Field, MarketState, Watered, WorldClock},
    terrain::Terrain,
};

use crate::{
    Beginning, WorldSeed, WorldStart,
    day_cycle::DayStarted,
    players::{AbsentPlayers, CharacterState, player_key, state_of},
    terrain::EditedChunks,
};

/// Real time between two saves, besides the one at every dawn.
const AUTOSAVE_INTERVAL: Duration = Duration::from_mins(5);

pub(crate) struct SavingPlugin {
    pub save_dir: Option<SaveDir>,
    /// Whether the world is new, and so has never been saved.
    pub new_world: bool,
}

impl Plugin for SavingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, report_lost_content);
        let Some(save_dir) = self.save_dir.clone() else {
            return;
        };
        info!("saving the world in {}", save_dir.root().display());
        app.insert_resource(Saving {
            save_dir,
            next_autosave: Timer::new(AUTOSAVE_INTERVAL, TimerMode::Repeating),
            never_saved: self.new_world,
        })
        // Last, so that an exit requested anywhere during the frame is seen.
        .add_systems(Last, save_world);
    }
}

#[derive(Resource)]
struct Saving {
    save_dir: SaveDir,
    next_autosave: Timer,
    never_saved: bool,
}

/// Tells the operator about anything the save referred to that the content
/// no longer defines, which was left out of the world.
fn report_lost_content(beginning: Res<Beginning>) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        for lost in &saved.lost {
            warn!("{lost}");
        }
    }
}

fn save_world(
    real_time: Res<Time<Real>>,
    mut saving: ResMut<Saving>,
    mut dawns: MessageReader<DayStarted>,
    mut exits: MessageReader<AppExit>,
    content: Res<Content>,
    seed: Res<WorldSeed>,
    clock: Single<&WorldClock>,
    market: Single<&MarketState>,
    fields: Query<(&Field, Has<Watered>, Has<Fertilized>, Option<&Crop>)>,
    terrain: Res<Terrain>,
    mut edited: ResMut<EditedChunks>,
    characters: Query<CharacterState<'_>>,
    mut absent: ResMut<AbsentPlayers>,
) {
    let autosave = saving.next_autosave.tick(real_time.delta()).just_finished();
    let dawn = dawns.read().count() > 0;
    let exiting = exits.read().count() > 0;
    if !(saving.never_saved || autosave || dawn || exiting) {
        return;
    }
    saving.never_saved = false;

    let started = Instant::now();
    let world = WorldState {
        seed: seed.0,
        clock: clock.0,
        market: market.0.clone(),
        fields: fields
            .iter()
            .map(|(field, watered, fertilized, crop)| FieldState {
                tile: field.tile,
                height: field.height,
                watered,
                fertilized,
                crop: crop.map(|crop| crop.0),
            })
            .collect(),
    };
    let today = clock.0.day();
    let save_dir = &saving.save_dir;
    let mut result = save_dir.save_world(&content, &world);
    if edited.unsaved {
        result = result.and_then(|()| {
            save_dir.save_terrain(
                edited
                    .chunks
                    .iter()
                    .filter_map(|&position| Some((position, terrain.get(position)?))),
            )
        });
        edited.unsaved = result.is_err();
    }
    for character in &characters {
        if let Some(key) = player_key(character.0.0) {
            result = result
                .and_then(|()| save_dir.save_player(&content, &key, &state_of(character, today)));
        }
    }
    let unsaved: Vec<String> = absent.unsaved.drain().collect();
    for key in unsaved {
        let saved = absent.players.get(&key).map_or(Ok(()), |player| {
            save_dir.save_player(&content, &key, player)
        });
        if saved.is_err() {
            absent.unsaved.insert(key);
        }
        result = result.and(saved);
    }

    match result {
        Ok(()) => info!("saved the world in {:?}", started.elapsed()),
        Err(SaveError { file, problem }) => {
            error!("could not save the world: {}: {problem}", file.display());
        }
    }
}
