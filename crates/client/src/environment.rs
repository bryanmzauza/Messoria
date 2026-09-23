//! Sky, sunlight and atmosphere, following the time of day.
//!
//! One directional light plays the sun by day and the moon by night. Its
//! color and strength, the sky color, the fog and the ambient light blend
//! between keyframes through the day.

use std::f32::consts::PI;

use bevy::{light::CascadeShadowConfigBuilder, pbr::DistanceFog, prelude::*};

use crate::clock::LocalClock;

/// Hour of day shown before the server's clock arrives.
const DEFAULT_HOUR: f32 = 12.0;

/// The look of the sky at one hour of the day.
struct SkyKey {
    hour: f32,
    /// Horizon color, used for the clear color, the fog and ambient light.
    sky: Color,
    light: Color,
    illuminance: f32,
    ambient_brightness: f32,
}

const NIGHT_SKY: Color = Color::srgb(0.05, 0.07, 0.15);
const MOONLIGHT: Color = Color::srgb(0.55, 0.65, 1.0);
const DAY_SKY: Color = Color::srgb(0.62, 0.76, 0.88);
const SUNLIGHT: Color = Color::srgb(1.0, 0.95, 0.85);

/// Keyframes in increasing hour order, covering the whole day so that
/// midnight blends into itself.
const SKY_KEYS: [SkyKey; 8] = [
    SkyKey {
        hour: 0.0,
        sky: NIGHT_SKY,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 5.0,
        sky: NIGHT_SKY,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 6.5,
        sky: Color::srgb(0.92, 0.62, 0.45),
        light: Color::srgb(1.0, 0.72, 0.5),
        illuminance: 4_000.0,
        ambient_brightness: 220.0,
    },
    SkyKey {
        hour: 8.5,
        sky: DAY_SKY,
        light: SUNLIGHT,
        illuminance: 12_000.0,
        ambient_brightness: 400.0,
    },
    SkyKey {
        hour: 17.5,
        sky: DAY_SKY,
        light: SUNLIGHT,
        illuminance: 12_000.0,
        ambient_brightness: 400.0,
    },
    SkyKey {
        hour: 19.5,
        sky: Color::srgb(0.9, 0.55, 0.38),
        light: Color::srgb(1.0, 0.6, 0.38),
        illuminance: 3_000.0,
        ambient_brightness: 200.0,
    },
    SkyKey {
        hour: 21.0,
        sky: NIGHT_SKY,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
    SkyKey {
        hour: 24.0,
        sky: NIGHT_SKY,
        light: MOONLIGHT,
        illuminance: 1_200.0,
        ambient_brightness: 120.0,
    },
];

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
        app.insert_resource(ClearColor(DAY_SKY))
            .insert_resource(GlobalAmbientLight {
                color: DAY_SKY,
                brightness: 400.0,
                ..default()
            })
            .add_systems(Startup, spawn_sky_light)
            .add_systems(Update, follow_time_of_day);
    }
}

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
    mut clear_color: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    light: Single<(&mut DirectionalLight, &mut Transform), With<SkyLight>>,
    mut fog: Query<&mut DistanceFog>,
) {
    let hour = clock.hours().unwrap_or(DEFAULT_HOUR).rem_euclid(24.0);
    let (sky, light_color, illuminance, ambient_brightness) = sky_at(hour);

    clear_color.0 = sky;
    ambient.color = sky;
    ambient.brightness = ambient_brightness;
    for mut fog in &mut fog {
        fog.color = sky;
    }

    let (mut light, mut transform) = light.into_inner();
    light.color = light_color;
    light.illuminance = illuminance;
    *transform = Transform::default().looking_to(-toward_light(hour), Vec3::Y);
}

/// Blends the keyframes around `hour`, which must be within `0..24`.
fn sky_at(hour: f32) -> (Color, Color, f32, f32) {
    let next = SKY_KEYS
        .iter()
        .position(|key| key.hour > hour)
        .unwrap_or(SKY_KEYS.len() - 1);
    let (from, to) = (&SKY_KEYS[next - 1], &SKY_KEYS[next]);
    let t = ((hour - from.hour) / (to.hour - from.hour)).clamp(0.0, 1.0);
    (
        from.sky.mix(&to.sky, t),
        from.light.mix(&to.light, t),
        from.illuminance.lerp(to.illuminance, t),
        from.ambient_brightness.lerp(to.ambient_brightness, t),
    )
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
        let (_, _, noon, _) = sky_at(12.0);
        let (_, _, midnight, _) = sky_at(0.0);
        assert!(noon > midnight * 5.0);
    }

    #[test]
    fn the_sky_blends_smoothly_through_midnight() {
        let (before, ..) = sky_at(23.99);
        let (after, ..) = sky_at(0.0);
        let gap = before.to_linear().to_vec4() - after.to_linear().to_vec4();
        assert!(gap.length() < 0.01);
    }

    #[test]
    fn the_sun_rises_in_the_east_and_sets_in_the_west() {
        assert!(toward_light(7.0).x > 0.5);
        assert!(toward_light(19.0).x < -0.5);
        assert!(toward_light(13.0).y > toward_light(7.0).y);
    }
}
