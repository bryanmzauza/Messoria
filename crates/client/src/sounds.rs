//! What the player hears: happenings in the world where they happen,
//! footsteps on the ground walked on, and the interface.
//!
//! World sounds are spatial, heard from the camera. Each sound picks one of
//! a few recordings at a slightly varied pitch, so repeats do not sound
//! mechanical.

use std::collections::HashMap;

use bevy::{
    audio::{DefaultSpatialScale, SpatialScale, Volume},
    prelude::*,
};
use messoria_calendar::Season;
use messoria_content::Tool;
use messoria_shared::{
    protocol::{Happened, PlayerId},
    terrain::Terrain,
    village,
};
use messoria_voxel::Material;

use crate::{
    art::DrawnSeason,
    avatars::AvatarSystems,
    feedback::{Told, Witnessed},
    panels::OpenPanel,
};

/// Spatial sound fades with the square of the distance from one unit away.
/// Scaling meters down keeps sounds a few meters off at full volume.
const METERS_PER_UNIT: f32 = 4.0;
/// Distance walked between two footsteps, in meters.
const STRIDE: f32 = 1.6;
/// Moves longer than this in one frame are jumps in position, not steps.
const MAX_STEP: f32 = 2.0;
/// How far below the feet the ground may be for a step to sound.
const GROUND_REACH: f32 = 0.2;
const FOOTSTEP_VOLUME: f32 = 0.35;
const INTERFACE_VOLUME: f32 = 0.5;
/// Largest change of pitch, up or down, between repeats of a sound.
const PITCH_SPREAD: f32 = 0.08;

pub(crate) struct SoundsPlugin;

impl Plugin for SoundsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DefaultSpatialScale(SpatialScale::new(
            1.0 / METERS_PER_UNIT,
        )))
        .add_systems(Startup, load_sounds)
        .add_systems(Update, (sound_happenings, sound_notices, sound_interface))
        .add_systems(PostUpdate, sound_footsteps.after(AvatarSystems));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Cue {
    Footstep(Ground),
    Dig,
    DigStone,
    Chop,
    Build,
    Raise,
    Till,
    Water,
    Plant,
    Fertilize,
    Harvest,
    Coins,
    Eat,
    Refused,
    Click,
    Open,
    Close,
    Backpack,
}

/// What a footstep sounds like it lands on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Ground {
    Grass,
    Soil,
    Hard,
    Snow,
}

impl Cue {
    const ALL: [Self; 21] = [
        Self::Footstep(Ground::Grass),
        Self::Footstep(Ground::Soil),
        Self::Footstep(Ground::Hard),
        Self::Footstep(Ground::Snow),
        Self::Dig,
        Self::DigStone,
        Self::Chop,
        Self::Build,
        Self::Raise,
        Self::Till,
        Self::Water,
        Self::Plant,
        Self::Fertilize,
        Self::Harvest,
        Self::Coins,
        Self::Eat,
        Self::Refused,
        Self::Click,
        Self::Open,
        Self::Close,
        Self::Backpack,
    ];

    /// Recordings to choose from, within the sounds folder.
    fn files(self) -> Vec<String> {
        let numbered = |name: &str| -> Vec<String> {
            (0..5)
                .map(|number| format!("impact/{name}_{number:03}.ogg"))
                .collect()
        };
        let listed = |names: &[&str]| names.iter().map(|&name| name.to_owned()).collect();
        match self {
            Self::Footstep(Ground::Grass) => numbered("footstep_grass"),
            Self::Footstep(Ground::Soil) => numbered("footstep_carpet"),
            Self::Footstep(Ground::Hard) => numbered("footstep_concrete"),
            Self::Footstep(Ground::Snow) => numbered("footstep_snow"),
            Self::Dig => numbered("impactSoft_medium"),
            Self::DigStone => numbered("impactMining"),
            Self::Build => numbered("impactPlank_medium"),
            Self::Chop => {
                let mut recordings = numbered("impactWood_medium");
                recordings.push("rpg/chop.ogg".to_owned());
                recordings
            }
            Self::Raise | Self::Till => numbered("impactSoft_heavy"),
            Self::Water => listed(&[
                "interface/drop_001.ogg",
                "interface/drop_002.ogg",
                "interface/drop_003.ogg",
                "interface/drop_004.ogg",
            ]),
            Self::Plant => listed(&["rpg/handleSmallLeather.ogg", "rpg/handleSmallLeather2.ogg"]),
            Self::Fertilize => listed(&["rpg/dropLeather.ogg"]),
            Self::Harvest => listed(&["interface/pluck_001.ogg", "interface/pluck_002.ogg"]),
            Self::Coins => listed(&["rpg/handleCoins.ogg", "rpg/handleCoins2.ogg"]),
            Self::Eat => listed(&["rpg/knifeSlice.ogg", "rpg/knifeSlice2.ogg"]),
            Self::Refused => listed(&["interface/error_004.ogg"]),
            Self::Click => listed(&["interface/click_001.ogg"]),
            Self::Open => listed(&["interface/open_001.ogg"]),
            Self::Close => listed(&["interface/close_001.ogg"]),
            Self::Backpack => listed(&["rpg/cloth1.ogg", "rpg/cloth2.ogg"]),
        }
    }

    fn of(happened: Happened) -> Self {
        match happened {
            Happened::Dug(Material::Stone) | Happened::Struck(Tool::Pickaxe) => Self::DigStone,
            Happened::Dug(Material::Grass | Material::Soil | Material::Sand)
            | Happened::Struck(Tool::Shovel | Tool::Hoe | Tool::WateringCan) => Self::Dig,
            Happened::Raised => Self::Raise,
            Happened::Tilled => Self::Till,
            Happened::Watered => Self::Water,
            Happened::Planted => Self::Plant,
            Happened::Fertilized => Self::Fertilize,
            Happened::Harvested => Self::Harvest,
            Happened::Struck(Tool::Axe) => Self::Chop,
            Happened::Traded => Self::Coins,
            Happened::Built => Self::Build,
            Happened::Ate => Self::Eat,
        }
    }
}

#[derive(Resource)]
struct Sounds(HashMap<Cue, Vec<Handle<AudioSource>>>);

impl Sounds {
    /// Plays `cue` at `volume`, from `at` in the world or, without a place,
    /// as part of the interface.
    fn play(&self, commands: &mut Commands, cue: Cue, at: Option<Vec3>, volume: f32) {
        let Some(recordings) = self.0.get(&cue).filter(|recordings| !recordings.is_empty()) else {
            return;
        };
        let recording = recordings[rand::random_range(0..recordings.len())].clone();
        let settings = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(volume))
            .with_speed(1.0 + rand::random_range(-PITCH_SPREAD..=PITCH_SPREAD))
            .with_spatial(at.is_some());
        commands.spawn((
            Name::new("Sound"),
            AudioPlayer(recording),
            settings,
            Transform::from_translation(at.unwrap_or_default()),
        ));
    }
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    let sounds = Cue::ALL
        .into_iter()
        .map(|cue| {
            let recordings = cue
                .files()
                .into_iter()
                .map(|file| assets.load(format!("sounds/{file}")))
                .collect();
            (cue, recordings)
        })
        .collect();
    commands.insert_resource(Sounds(sounds));
}

fn sound_happenings(
    sounds: Res<Sounds>,
    mut witnessed: MessageReader<Witnessed>,
    mut commands: Commands,
) {
    for Witnessed(happening) in witnessed.read() {
        sounds.play(
            &mut commands,
            Cue::of(happening.what),
            Some(happening.at),
            1.0,
        );
    }
}

fn sound_notices(sounds: Res<Sounds>, mut told: MessageReader<Told>, mut commands: Commands) {
    // Several refusals at once sound as one.
    if told.read().count() > 0 {
        sounds.play(&mut commands, Cue::Refused, None, INTERFACE_VOLUME);
    }
}

fn sound_interface(
    sounds: Res<Sounds>,
    panel: Res<OpenPanel>,
    buttons: Query<&Interaction, (With<Button>, Changed<Interaction>)>,
    mut was_open: Local<OpenPanel>,
    mut commands: Commands,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        sounds.play(&mut commands, Cue::Click, None, INTERFACE_VOLUME);
    }
    if *panel == *was_open {
        return;
    }
    let cue = match (*was_open, *panel) {
        (OpenPanel::Backpack, _) | (_, OpenPanel::Backpack) => Some(Cue::Backpack),
        // Moving between the menu and its options is a click already heard.
        (OpenPanel::Menu, OpenPanel::Options) | (OpenPanel::Options, OpenPanel::Menu) => None,
        (_, OpenPanel::None) => Some(Cue::Close),
        _ => Some(Cue::Open),
    };
    if let Some(cue) = cue {
        sounds.play(&mut commands, cue, None, INTERFACE_VOLUME);
    }
    *was_open = *panel;
}

/// How far a character has walked since its last footstep.
#[derive(Component)]
struct Stride {
    last: Vec3,
    walked: f32,
}

fn sound_footsteps(
    sounds: Res<Sounds>,
    terrain: Res<Terrain>,
    season: Res<DrawnSeason>,
    mut characters: Query<(Entity, &Transform, Option<&mut Stride>), With<PlayerId>>,
    mut commands: Commands,
) {
    for (character, transform, stride) in &mut characters {
        let feet = transform.translation;
        let Some(mut stride) = stride else {
            commands.entity(character).insert(Stride {
                last: feet,
                walked: 0.0,
            });
            continue;
        };
        let step = feet.xz().distance(stride.last.xz());
        stride.last = feet;
        let grounded = terrain
            .surface_below(feet + Vec3::Y * GROUND_REACH, 2.0 * GROUND_REACH)
            .is_some();
        if step > MAX_STEP || !grounded {
            continue;
        }
        stride.walked += step;
        if stride.walked < STRIDE {
            continue;
        }
        stride.walked = 0.0;
        let ground = ground_under(&terrain, feet, season.0);
        sounds.play(
            &mut commands,
            Cue::Footstep(ground),
            Some(feet),
            FOOTSTEP_VOLUME,
        );
    }
}

fn ground_under(terrain: &Terrain, feet: Vec3, season: Season) -> Ground {
    if village::reaches(feet, 0.0) {
        return Ground::Hard;
    }
    // Just below the surface: a point exactly on it may count as air.
    match terrain.surface_material(feet - Vec3::Y * 0.1) {
        Some(Material::Grass) if season == Season::Winter => Ground::Snow,
        Some(Material::Grass) | None => Ground::Grass,
        Some(Material::Soil | Material::Sand) => Ground::Soil,
        Some(Material::Stone) => Ground::Hard,
    }
}
