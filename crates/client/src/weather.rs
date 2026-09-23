//! Falling rain and snow around the viewer.
//!
//! Drops are drawn as short gizmo lines in a box that follows the camera.
//! Each drop's place is fixed and only its height changes with time, so the
//! weather needs no state of its own.

use std::f32::consts::TAU;

use bevy::prelude::*;
use messoria_calendar::Weather;
use messoria_shared::protocol::CurrentWeather;

const DROPS: u32 = 500;
/// Half the width and the height of the box drops fall through, in meters.
const BOX_RADIUS: f32 = 14.0;
const BOX_HEIGHT: f32 = 12.0;

pub(crate) struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw_precipitation);
    }
}

/// How a kind of precipitation falls and looks.
struct Precipitation {
    speed: f32,
    length: f32,
    /// Horizontal drift per meter fallen.
    slant: Vec2,
    color: Color,
}

const RAIN: Precipitation = Precipitation {
    speed: 11.0,
    length: 0.45,
    slant: Vec2::new(0.08, 0.03),
    color: Color::srgba(0.75, 0.8, 0.9, 0.45),
};

const SNOW: Precipitation = Precipitation {
    speed: 1.5,
    length: 0.06,
    slant: Vec2::new(0.25, 0.1),
    color: Color::srgba(1.0, 1.0, 1.0, 0.8),
};

fn draw_precipitation(
    time: Res<Time>,
    weather: Query<&CurrentWeather>,
    camera: Single<&Transform, With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    let precipitation = match weather.single().map(|weather| weather.0) {
        Ok(Weather::Rain) => RAIN,
        Ok(Weather::Snow) => SNOW,
        Ok(Weather::Clear) | Err(_) => return,
    };
    let elapsed = time.elapsed_secs();
    for drop in 0..DROPS {
        let [spread, angle, phase] = [0, 1, 2].map(|channel| unit_noise(drop * 3 + channel));
        // Square root spreads drops evenly over the disc rather than bunching
        // them in the middle.
        let offset = Vec2::from_angle(angle * TAU) * spread.sqrt() * BOX_RADIUS;
        let fallen = (elapsed * precipitation.speed + phase * BOX_HEIGHT).rem_euclid(BOX_HEIGHT);
        let drift = precipitation.slant * fallen;
        let top = camera.translation + Vec3::new(offset.x, BOX_HEIGHT / 2.0, offset.y);
        let head = top + Vec3::new(drift.x, -fallen, drift.y);
        let tail = head
            + Vec3::new(precipitation.slant.x, 1.0, precipitation.slant.y) * precipitation.length;
        gizmos.line(head, tail, precipitation.color);
    }
}

/// A fixed pseudo-random number in `0..1` for `index`.
fn unit_noise(index: u32) -> f32 {
    let mut value = index.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    #[expect(clippy::cast_precision_loss, reason = "only the top bits matter")]
    let unit = (value >> 8) as f32 / (1 << 24) as f32;
    unit
}
