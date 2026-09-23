//! Sunlight and the colors of the sky, following the time of day.
//!
//! One directional light plays the sun by day and the moon by night. Its
//! color and strength, the sky's colors at the horizon and overhead, the fog
//! and the ambient light blend between keyframes through the day. The sky
//! itself is drawn by `sky`, from the [`Sky`] this module keeps.

use std::f32::consts::PI;

use bevy::{light::CascadeShadowConfigBuilder, pbr::DistanceFog, prelude::*};
use messoria_calendar::Weather;
use messoria_shared::protocol::CurrentWeather;

use crate::clock::LocalClock;

/// Hour of day shown before the server's clock arrives.
const DEFAULT_HOUR: f32 = 12.0;

/// The look of the sky at one hour of the day.
struct SkyKey {
    hour: f32,
    /// Color at the horizon, also used for the fog and ambient light.
    horizon: Color,
    /// Color straight overhead.
    zenith: Color,
    light: Color,
    illuminance: f32,
    ambient_brightness: f32,
}

const NIGHT_HORIZON: Color = Color::srgb(0.07, 0.09, 0.18);
const NIGHT_ZENITH: Color = Color::srgb(0.01, 0.02, 0.06);
const MOONLIGHT: Color = Color::srgb(0.55, 0.65, 1.0);
const DAY_HORIZON: Color = Color::srgb(0.7, 0.82, 0.9);
const DAY_ZENITH: Color = Color::srgb(0.3, 0.52, 0.84);
const SUNLIGHT: Color = Color::srgb(1.0, 0.95, 0.85);

/// Keyframes in increasing hour order, covering the whole day so that
/// midnight blends into itself.
const SKY_KEYS: [SkyKey; 8] = [
    SkyKey {
        hour: 0.0,
        horizon: NIGHT_HORIZON,
        zenith: NIGHT_ZENITH,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 5.0,
        horizon: NIGHT_HORIZON,
        zenith: NIGHT_ZENITH,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 6.5,
        horizon: Color::srgb(0.95, 0.64, 0.45),
        zenith: Color::srgb(0.42, 0.5, 0.74),
        light: Color::srgb(1.0, 0.72, 0.5),
        illuminance: 4_000.0,
        ambient_brightness: 220.0,
    },
    SkyKey {
        hour: 8.5,
        horizon: DAY_HORIZON,
        zenith: DAY_ZENITH,
        light: SUNLIGHT,
        illuminance: 12_000.0,
        ambient_brightness: 400.0,
    },
    SkyKey {
        hour: 17.5,
        horizon: DAY_HORIZON,
        zenith: DAY_ZENITH,
        light: SUNLIGHT,
        illuminance: 12_000.0,
        ambient_brightness: 400.0,
    },
    SkyKey {
        hour: 19.5,
        horizon: Color::srgb(0.95, 0.56, 0.38),
        zenith: Color::srgb(0.34, 0.36, 0.62),
        light: Color::srgb(1.0, 0.6, 0.38),
        illuminance: 3_000.0,
        ambient_brightness: 200.0,
    },
    SkyKey {
        hour: 21.0,
        horizon: NIGHT_HORIZON,
        zenith: NIGHT_ZENITH,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 24.0,
        horizon: NIGHT_HORIZON,
        zenith: NIGHT_ZENITH,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
];

/// Overcast skies turn toward this gray and let through this share of light.
const OVERCAST: Color = Color::srgb(0.45, 0.48, 0.52);
const OVERCAST_BLEND: f32 = 0.6;
const OVERCAST_LIGHT: f32 = 0.35;

/// When the sun is up; the moon has the rest of the day.
const SUNRISE: f32 = 6.0;
const SUNSET: f32 = 20.0;
/// Highest the sun and the moon climb, in radians above the horizon.
const SUN_PEAK: f32 = 60.0 * PI / 180.0;
const MOON_PEAK: f32 = 40.0 * PI / 180.0;
/// Lowest the light gets, so shadows never stretch across the whole valley.
const MIN_ELEVATION: f32 = 10.0 * PI / 180.0;

pub(crate) struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(DAY_HORIZON))
            .insert_resource(GlobalAmbientLight {
                color: DAY_HORIZON,
                brightness: 400.0,
                ..default()
            })
            .init_resource::<Sky>()
            .add_systems(Startup, spawn_sky_light)
            .add_systems(Update, follow_time_of_day);
    }
}

/// How the sky looks now.
#[derive(Resource)]
pub(crate) struct Sky {
    pub horizon: Color,
    pub zenith: Color,
    /// Direction toward the sun by day, or the moon by night.
    pub toward_light: Vec3,
    pub sun_up: bool,
    pub overcast: bool,
    /// How bright it is, from 0 at night to 1 on a clear day.
    pub daylight: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            horizon: DAY_HORIZON,
            zenith: DAY_ZENITH,
            toward_light: toward_light(DEFAULT_HOUR),
            sun_up: true,
            overcast: false,
            daylight: 1.0,
        }
    }
}

/// Illuminance of a clear day, which counts as full daylight.
const FULL_DAYLIGHT: f32 = 12_000.0;

#[derive(Component)]
struct SkyLight;

fn spawn_sky_light(mut commands: Commands) {
    commands.spawn((
        Name::new("Sky light"),
        SkyLight,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            maximum_distance: 120.0,
            ..default()
        }
        .build(),
    ));
}

fn follow_time_of_day(
    clock: Res<LocalClock>,
    weather: Query<&CurrentWeather>,
    mut sky: ResMut<Sky>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    light: Single<(&mut DirectionalLight, &mut Transform), With<SkyLight>>,
    mut fog: Query<&mut DistanceFog>,
) {
    let hour = clock.hours().unwrap_or(DEFAULT_HOUR).rem_euclid(24.0);
    let mut look = sky_at(hour);
    let overcast = weather
        .single()
        .is_ok_and(|weather| weather.0 != Weather::Clear);
    if overcast {
        look.horizon = look.horizon.mix(&OVERCAST, OVERCAST_BLEND);
        look.zenith = look.zenith.mix(&OVERCAST, OVERCAST_BLEND);
        look.illuminance *= OVERCAST_LIGHT;
        look.ambient_brightness *= 1.0 - OVERCAST_BLEND / 2.0;
    }

    *sky = Sky {
        horizon: look.horizon,
        zenith: look.zenith,
        toward_light: toward_light(hour),
        sun_up: (SUNRISE..SUNSET).contains(&hour),
        overcast,
        daylight: (look.illuminance / FULL_DAYLIGHT).clamp(0.0, 1.0),
    };
    clear_color.0 = look.horizon;
    ambient.color = look.horizon;
    ambient.brightness = look.ambient_brightness;
    for mut fog in &mut fog {
        fog.color = look.horizon;
    }

    let (mut light, mut transform) = light.into_inner();
    light.color = look.light;
    light.illuminance = look.illuminance;
    *transform = Transform::default().looking_to(-sky.toward_light, Vec3::Y);
}

/// The sky's colors and light at one hour.
struct SkyLook {
    horizon: Color,
    zenith: Color,
    light: Color,
    illuminance: f32,
    ambient_brightness: f32,
}

/// Blends the keyframes around `hour`, which must be within `0..24`.
fn sky_at(hour: f32) -> SkyLook {
    let next = SKY_KEYS
        .iter()
        .position(|key| key.hour > hour)
        .unwrap_or(SKY_KEYS.len() - 1);
    let (from, to) = (&SKY_KEYS[next - 1], &SKY_KEYS[next]);
    let t = ((hour - from.hour) / (to.hour - from.hour)).clamp(0.0, 1.0);
    SkyLook {
        horizon: from.horizon.mix(&to.horizon, t),
        zenith: from.zenith.mix(&to.zenith, t),
        light: from.light.mix(&to.light, t),
        illuminance: from.illuminance.lerp(to.illuminance, t),
        ambient_brightness: from.ambient_brightness.lerp(to.ambient_brightness, t),
    }
}

/// Direction from the ground toward the sun by day or the moon by night.
/// Both rise in the east (+x), pass to the south (+z) and set in the west.
fn toward_light(hour: f32) -> Vec3 {
    let (progress, peak) = if (SUNRISE..SUNSET).contains(&hour) {
        ((hour - SUNRISE) / (SUNSET - SUNRISE), SUN_PEAK)
    } else {
        let night = 24.0 - SUNSET + SUNRISE;
        ((hour - SUNSET).rem_euclid(24.0) / night, MOON_PEAK)
    };
    let arc = progress * PI;
    let elevation = (arc.sin() * peak).max(MIN_ELEVATION);
    Vec3::new(
        arc.cos() * elevation.cos(),
        elevation.sin(),
        0.35 * elevation.cos(),
    )
    .normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_is_bright_and_midnight_is_dark() {
        assert!(sky_at(12.0).illuminance > sky_at(0.0).illuminance * 5.0);
    }

    #[test]
    fn the_sky_blends_smoothly_through_midnight() {
        let (before, after) = (sky_at(23.99), sky_at(0.0));
        for (before, after) in [
            (before.horizon, after.horizon),
            (before.zenith, after.zenith),
        ] {
            let gap = before.to_linear().to_vec4() - after.to_linear().to_vec4();
            assert!(gap.length() < 0.01);
        }
    }

    #[test]
    fn the_sun_rises_in_the_east_and_sets_in_the_west() {
        assert!(toward_light(7.0).x > 0.5);
        assert!(toward_light(19.0).x < -0.5);
        assert!(toward_light(13.0).y > toward_light(7.0).y);
    }
}
