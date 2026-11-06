//! What the valley sounds like: wind, birds by day, crickets by night,
//! rain when it rains, the river near its banks, and all of it muffled
//! indoors.
//!
//! Every layer loops all the time, in a plain and a muffled recording,
//! at volumes that follow the hour, the season, the weather and where the
//! camera is; changes are eased so nothing cuts in.

use bevy::{audio::Volume, prelude::*};
use messoria_calendar::{Season, Weather};
use messoria_shared::{
    content::Content,
    protocol::{CurrentWeather, Structure},
    valley::Valley,
};

use crate::{art::DrawnSeason, camera::WorldCamera, clock::LocalClock};

/// How fast a layer moves toward its volume, in full volume per second.
const EASE_RATE: f32 = 0.5;
/// Hours between which birds sing, and from which crickets chirp.
const BIRDS_FROM: f32 = 6.0;
const BIRDS_UNTIL: f32 = 19.5;
const CRICKETS_FROM: f32 = 20.0;
/// How far from the river it is heard, in meters.
const RIVER_HEARD_FROM: f32 = 60.0;
const WIND_VOLUME: f32 = 0.3;
const WEATHER_WIND: f32 = 0.25;
const BIRDS_VOLUME: f32 = 0.45;
const CRICKETS_VOLUME: f32 = 0.35;
const RAIN_VOLUME: f32 = 0.8;
const RIVER_VOLUME: f32 = 0.7;

pub(crate) struct AmbiencePlugin;

impl Plugin for AmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_layers)
            .add_systems(Update, follow_the_valley);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Wind,
    Birds,
    Crickets,
    Rain,
    River,
}

impl Layer {
    const ALL: [Self; 5] = [
        Self::Wind,
        Self::Birds,
        Self::Crickets,
        Self::Rain,
        Self::River,
    ];

    fn file(self, inside: bool) -> String {
        let name = match self {
            Self::Wind => "wind",
            Self::Birds => "birds",
            Self::Crickets => "crickets",
            Self::Rain => "rain",
            Self::River => "river",
        };
        let variant = if inside { "_inside" } else { "" };
        format!("sounds/ambience/{name}{variant}.ogg")
    }
}

/// One looping recording: a layer, plain or muffled.
#[derive(Component)]
struct AmbienceLayer {
    layer: Layer,
    inside: bool,
}

fn start_layers(assets: Res<AssetServer>, mut commands: Commands) {
    for layer in Layer::ALL {
        for inside in [false, true] {
            commands.spawn((
                Name::new(format!("{layer:?} ambience")),
                AudioPlayer::new(assets.load(layer.file(inside))),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
                AmbienceLayer { layer, inside },
            ));
        }
    }
}

/// What the valley sounds like at one moment and place.
struct Soundscape {
    wind: f32,
    birds: f32,
    crickets: f32,
    rain: f32,
    river: f32,
    inside: bool,
}

impl Soundscape {
    fn of(hour: f32, season: Season, weather: Weather, river: f32, inside: bool) -> Self {
        let hour = hour.rem_euclid(24.0);
        let daytime = (BIRDS_FROM..BIRDS_UNTIL).contains(&hour);
        let night = !(BIRDS_FROM..CRICKETS_FROM).contains(&hour);
        let wintry = season == Season::Winter;
        Self {
            wind: WIND_VOLUME
                + if weather == Weather::Clear {
                    0.0
                } else {
                    WEATHER_WIND
                },
            birds: if daytime && !wintry && weather == Weather::Clear {
                BIRDS_VOLUME
            } else {
                0.0
            },
            crickets: if night && !wintry {
                CRICKETS_VOLUME
            } else {
                0.0
            },
            rain: if weather == Weather::Rain {
                RAIN_VOLUME
            } else {
                0.0
            },
            river: (1.0 - river / RIVER_HEARD_FROM).clamp(0.0, 1.0) * RIVER_VOLUME,
            inside,
        }
    }

    fn volume_of(&self, layer: &AmbienceLayer) -> f32 {
        if layer.inside != self.inside {
            return 0.0;
        }
        match layer.layer {
            Layer::Wind => self.wind,
            Layer::Birds => self.birds,
            Layer::Crickets => self.crickets,
            Layer::Rain => self.rain,
            Layer::River => self.river,
        }
    }
}

fn follow_the_valley(
    time: Res<Time>,
    clock: Res<LocalClock>,
    season: Res<DrawnSeason>,
    content: Res<Content>,
    valley: Option<Res<Valley>>,
    weather: Query<&CurrentWeather>,
    structures: Query<&Structure>,
    camera: Single<&GlobalTransform, With<WorldCamera>>,
    mut layers: Query<(&AmbienceLayer, &mut AudioSink)>,
) {
    // Silence until the world's clock is known.
    let hour = clock.hours().unwrap_or(f32::NAN);
    let at = camera.translation().xz();
    let inside = structures.iter().any(|structure| {
        let definition = content.structure(structure.kind);
        (definition.door.is_some() || definition.home) && structure.covers(definition, at, 0.0)
    });
    let river = valley.map_or(f32::INFINITY, |valley| valley.0.river_distance(at));
    let weather = weather.single().map_or(Weather::Clear, |weather| weather.0);
    let soundscape = Soundscape::of(hour, season.0, weather, river, inside);

    let step = EASE_RATE * time.delta_secs();
    for (layer, mut sink) in &mut layers {
        let target = if hour.is_nan() {
            0.0
        } else {
            soundscape.volume_of(layer)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn birds_sing_by_day_and_crickets_by_night_but_not_in_winter() {
        let noon = Soundscape::of(12.0, Season::Spring, Weather::Clear, 200.0, false);
        assert!(noon.birds > 0.0 && noon.crickets == 0.0);
        let night = Soundscape::of(23.0, Season::Summer, Weather::Clear, 200.0, false);
        assert!(night.birds == 0.0 && night.crickets > 0.0);
        let late = Soundscape::of(25.0, Season::Summer, Weather::Clear, 200.0, false);
        assert!(late.crickets > 0.0, "past midnight is still night");
        let winter = Soundscape::of(12.0, Season::Winter, Weather::Clear, 200.0, false);
        assert!(winter.birds == 0.0 && winter.crickets == 0.0);
    }

    #[test]
    fn rain_silences_the_birds_and_the_river_is_heard_near_its_banks() {
        let rain = Soundscape::of(12.0, Season::Spring, Weather::Rain, 10.0, false);
        assert!(rain.rain > 0.0 && rain.birds == 0.0 && rain.wind > WIND_VOLUME);
        assert!(rain.river > 0.5);
        let far = Soundscape::of(12.0, Season::Spring, Weather::Clear, 500.0, true);
        assert!(far.river.abs() < f32::EPSILON);
        let plain = AmbienceLayer {
            layer: Layer::Wind,
            inside: false,
        };
        assert!(
            far.volume_of(&plain) == 0.0,
            "indoors, only muffled layers play"
        );
    }
}
