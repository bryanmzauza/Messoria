//! Music for the season: one piece each, looping, crossfaded into the
//! next as the season turns, at the volume the player chose. The title
//! screen plays spring.

use bevy::{audio::Volume, prelude::*};
use messoria_calendar::Season;

use crate::{art::DrawnSeason, settings::Preferences};

/// How fast a piece fades in or out, in full volume per second.
const FADE_RATE: f32 = 0.4;

pub(crate) struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (play_for_season, fade_tracks).chain());
    }
}

/// A piece playing, or fading out once another took its place.
#[derive(Component)]
struct MusicTrack {
    season: Season,
    playing: bool,
}

fn file_of(season: Season) -> String {
    format!("music/{}.ogg", season.to_string().to_lowercase())
}

fn play_for_season(
    assets: Res<AssetServer>,
    season: Option<Res<DrawnSeason>>,
    mut tracks: Query<&mut MusicTrack>,
    mut commands: Commands,
) {
    let wanted = season.map_or(Season::Spring, |season| season.0);
    if tracks
        .iter()
        .any(|track| track.playing && track.season == wanted)
    {
        return;
    }
    for mut track in &mut tracks {
        track.playing = false;
    }
    commands.spawn((
        Name::new(format!("{wanted} music")),
        AudioPlayer::new(assets.load(file_of(wanted))),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        MusicTrack {
            season: wanted,
            playing: true,
        },
    ));
}

fn fade_tracks(
    time: Res<Time>,
    preferences: Res<Preferences>,
    mut tracks: Query<(Entity, &MusicTrack, &mut AudioSink)>,
    mut commands: Commands,
) {
    let step = FADE_RATE * time.delta_secs();
    for (entity, track, mut sink) in &mut tracks {
        let target = if track.playing {
            preferences.0.music_volume
        } else {
            0.0
        };
        let now = sink.volume().to_linear();
        let next = if now < target {
            (now + step).min(target)
        } else {
            (now - step).max(target)
        };
        if (next - now).abs() > f32::EPSILON {
            sink.set_volume(Volume::Linear(next));
        }
        if !track.playing && next <= f32::EPSILON {
            commands.entity(entity).despawn();
        }
    }
}
