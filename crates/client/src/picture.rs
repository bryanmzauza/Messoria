//! The picture the world camera makes: rendered in high dynamic range, then
//! finished like film, graded to the time of day and the weather.
//!
//! Light is kept in physical-like units until the end, where tonemapping
//! rolls highlights off softly instead of clipping them, keeping the colors
//! the art is painted in everywhere else; bright things such as lamps and
//! the sun bloom. Ambient occlusion darkens where surfaces meet, temporal
//! anti-aliasing smooths edges and the noise of the occlusion and of soft
//! shadow edges. The grade then warms dawn and dusk, cools and lifts the
//! night, and greys an overcast day. A haze hangs in the air around the
//! camera, thick enough at dawn and dusk for the low sun to cast beams
//! through it.

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    camera::Hdr,
    core_pipeline::tonemapping::Tonemapping,
    light::{DirectionalLightShadowMap, FogVolume, ShadowFilteringMethod, VolumetricFog},
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    post_process::bloom::Bloom,
    prelude::*,
    render::view::ColorGrading,
};

use messoria_save::Graphics;

use crate::{
    camera::{CameraPlacement, WorldCamera},
    clock::LocalClock,
    environment::Sky,
    settings::Preferences,
};

/// Width and height of each shadow cascade, in texels, at each graphics
/// quality.
const SHADOW_MAP_SIZES: [usize; 3] = [1024, 2048, 4096];

/// How the picture is graded at one hour of the day.
struct GradeKey {
    hour: f32,
    /// Shift toward red (positive) or blue (negative).
    temperature: f32,
    saturation: f32,
    /// Exposure added, in stops.
    exposure: f32,
    /// How thick the haze is.
    haze: f32,
}

/// Keyframes in increasing hour order, covering the whole day so that
/// midnight blends into itself.
const GRADE_KEYS: [GradeKey; 8] = [
    GradeKey {
        hour: 0.0,
        temperature: -0.04,
        saturation: 0.8,
        exposure: 0.6,
        haze: 0.004,
    },
    GradeKey {
        hour: 5.0,
        temperature: -0.04,
        saturation: 0.8,
        exposure: 0.6,
        haze: 0.012,
    },
    GradeKey {
        hour: 6.5,
        temperature: 0.03,
        saturation: 1.0,
        exposure: 0.0,
        haze: 0.03,
    },
    GradeKey {
        hour: 9.0,
        temperature: 0.0,
        saturation: 0.95,
        exposure: -0.9,
        haze: 0.004,
    },
    GradeKey {
        hour: 17.0,
        temperature: 0.0,
        saturation: 0.95,
        exposure: -0.9,
        haze: 0.004,
    },
    GradeKey {
        hour: 19.5,
        temperature: 0.03,
        saturation: 1.0,
        exposure: -0.1,
        haze: 0.028,
    },
    GradeKey {
        hour: 21.0,
        temperature: -0.04,
        saturation: 0.8,
        exposure: 0.6,
        haze: 0.006,
    },
    GradeKey {
        hour: 24.0,
        temperature: -0.04,
        saturation: 0.8,
        exposure: 0.6,
        haze: 0.004,
    },
];

/// Size of the box of haze kept around the camera, in meters.
const HAZE_BOX: Vec3 = Vec3::new(160.0, 60.0, 160.0);
/// Steps taken through the haze for each pixel.
const HAZE_STEPS: u32 = 32;

/// An overcast day loses this share of its color, and is this much cooler.
const OVERCAST_SATURATION: f32 = 0.8;
const OVERCAST_TEMPERATURE: f32 = -0.03;
/// Hour graded before the server's clock arrives.
const DEFAULT_HOUR: f32 = 12.0;

pub(crate) struct PicturePlugin;

impl Plugin for PicturePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(develop)
            .add_systems(Startup, spawn_haze)
            .add_systems(Update, (grade, match_quality))
            .add_systems(PostUpdate, follow_with_haze.after(CameraPlacement));
    }
}

/// Gives the world camera what the picture is made with at any quality;
/// `match_quality` adds the rest.
fn develop(trigger: On<Add, WorldCamera>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        Hdr,
        Tonemapping::KhronosPbrNeutral,
        Bloom::NATURAL,
        // Temporal anti-aliasing takes the place of multisampling.
        Msaa::Off,
        TemporalAntiAliasing::default(),
        ShadowFilteringMethod::Temporal,
        ColorGrading::default(),
    ));
}

/// Draws as much of the picture's finish as the graphics quality asks for:
/// ambient occlusion from medium up, and the haze's sunbeams only at high,
/// with sharper shadows at each step.
fn match_quality(
    preferences: Res<Preferences>,
    camera: Single<(Entity, Ref<WorldCamera>)>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut commands: Commands,
) {
    let (camera, added) = camera.into_inner();
    if !preferences.is_changed() && !added.is_added() {
        return;
    }
    let graphics = preferences.0.graphics;
    let mut camera = commands.entity(camera);
    match graphics {
        Graphics::Low => {
            camera.remove::<ScreenSpaceAmbientOcclusion>();
        }
        Graphics::Medium | Graphics::High => {
            camera.insert(ScreenSpaceAmbientOcclusion {
                quality_level: if graphics == Graphics::High {
                    ScreenSpaceAmbientOcclusionQualityLevel::High
                } else {
                    ScreenSpaceAmbientOcclusionQualityLevel::Low
                },
                ..default()
            });
        }
    }
    if graphics == Graphics::High {
        camera.insert(VolumetricFog {
            step_count: HAZE_STEPS,
            // Temporal anti-aliasing smooths out the jittered steps.
            jitter: 0.5,
            ambient_intensity: 0.0,
            ..default()
        });
    } else {
        camera.remove::<VolumetricFog>();
    }
    let size = SHADOW_MAP_SIZES[graphics as usize];
    if shadow_map.size != size {
        shadow_map.size = size;
    }
}

#[derive(Component)]
struct Haze;

fn spawn_haze(mut commands: Commands) {
    commands.spawn((
        Name::new("Haze"),
        Haze,
        FogVolume {
            density_factor: 0.0,
            ..default()
        },
        Transform::from_scale(HAZE_BOX),
    ));
}

/// Keeps the haze around the camera, wherever it goes.
fn follow_with_haze(
    camera: Single<&Transform, (With<WorldCamera>, Without<Haze>)>,
    mut haze: Single<&mut Transform, With<Haze>>,
) {
    haze.translation = camera.translation;
}

fn grade(
    clock: Res<LocalClock>,
    sky: Res<Sky>,
    mut grading: Single<&mut ColorGrading, With<WorldCamera>>,
    mut haze: Single<&mut FogVolume, With<Haze>>,
) {
    let hour = clock.hours().unwrap_or(DEFAULT_HOUR).rem_euclid(24.0);
    let mut key = grade_at(hour);
    if sky.overcast {
        key.saturation *= OVERCAST_SATURATION;
        key.temperature += OVERCAST_TEMPERATURE;
    }
    let global = &mut grading.global;
    global.temperature = key.temperature;
    global.post_saturation = key.saturation;
    global.exposure = key.exposure;
    haze.density_factor = key.haze;
    haze.fog_color = sky.horizon;
}

/// Blends the keyframes around `hour`, which must be within `0..24`.
fn grade_at(hour: f32) -> GradeKey {
    let next = GRADE_KEYS
        .iter()
        .position(|key| key.hour > hour)
        .unwrap_or(GRADE_KEYS.len() - 1);
    let (from, to) = (&GRADE_KEYS[next - 1], &GRADE_KEYS[next]);
    let t = ((hour - from.hour) / (to.hour - from.hour)).clamp(0.0, 1.0);
    GradeKey {
        hour,
        temperature: from.temperature.lerp(to.temperature, t),
        saturation: from.saturation.lerp(to.saturation, t),
        exposure: from.exposure.lerp(to.exposure, t),
        haze: from.haze.lerp(to.haze, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grade_blends_smoothly_through_midnight() {
        let (before, after) = (grade_at(23.99), grade_at(0.0));
        assert!((before.temperature - after.temperature).abs() < 0.001);
        assert!((before.exposure - after.exposure).abs() < 0.001);
    }

    #[test]
    fn dusk_is_warmer_than_noon() {
        assert!(grade_at(19.5).temperature > grade_at(12.0).temperature);
    }
}
