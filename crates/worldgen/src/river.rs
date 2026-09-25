//! The river: it runs across the world from one edge to the other, well to
//! one side of the village, winding as it goes.
//!
//! Its water only ever runs downhill: the level at each point of its course
//! is the lowest the ground has been upstream, a little under it, so where
//! the ground rises across its path the river cuts a gorge rather than
//! climbing. Its bed and banks are carved into the ground around it, a
//! channel shallow enough to wade.

use glam::Vec2;

use crate::noise::{mix, unit};

/// Half the width of the channel, in meters.
pub const RIVER_HALF_WIDTH: f32 = 5.0;
/// How far the water lies below the ground along the river's course.
const BELOW_GROUND: f32 = 1.2;
/// Depth of the channel's middle, and of its edges, under the water.
const DEPTH: f32 = 1.1;
const EDGE_DEPTH: f32 = 0.15;
/// Rise of the banks, in meters per meter.
const BANK_SLOPE: f32 = 0.55;
/// Distance between the points the water level is worked out at.
const STEP: f32 = 4.0;
/// How far the river runs from the middle of the world, give or take.
const OFFSET: (f32, f32) = (300.0, 380.0);
/// Its bends: two waves across its course, of these sizes and lengths.
const BENDS: [(f32, f32); 2] = [(70.0, 170.0), (25.0, 65.0)];

/// A point on the river's course and the level of its water there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverPoint {
    pub position: Vec2,
    pub level: f32,
}

pub(crate) struct River {
    /// Its course's distance from the middle, signed by the side it is on.
    offset: f32,
    phases: [f32; 2],
    /// Where its course starts, along z.
    start: f32,
    /// The water level every `STEP` along z from `start`.
    levels: Vec<f32>,
}

impl River {
    /// Lays the river's course across a world `half_width` meters on each
    /// side of the middle, over ground whose height is `ground`.
    pub(crate) fn lay(seed: u64, half_width: f32, ground: impl Fn(Vec2) -> f32) -> Self {
        let side = if mix(&[seed, 0x71]) & 1 == 0 {
            1.0
        } else {
            -1.0
        };
        let offset = side * (OFFSET.0 + unit(mix(&[seed, 0x72])) * (OFFSET.1 - OFFSET.0));
        let phases = [0x73, 0x74].map(|salt| unit(mix(&[seed, salt])) * std::f32::consts::TAU);
        let mut river = Self {
            offset,
            phases,
            start: -half_width,
            levels: Vec::new(),
        };
        let mut level = f32::INFINITY;
        let mut z = -half_width;
        while z <= half_width {
            let here = Vec2::new(river.course_x(z), z);
            level = level.min(ground(here) - BELOW_GROUND);
            river.levels.push(level);
            z += STEP;
        }
        river
    }

    /// Where the middle of the river is, across x, at `z`.
    fn course_x(&self, z: f32) -> f32 {
        BENDS
            .iter()
            .zip(self.phases)
            .map(|(&(size, length), phase)| size * (z / length + phase).sin())
            .sum::<f32>()
            + self.offset
    }

    /// How fast the course moves across x as z grows.
    fn course_slope(&self, z: f32) -> f32 {
        BENDS
            .iter()
            .zip(self.phases)
            .map(|(&(size, length), phase)| size / length * (z / length + phase).cos())
            .sum()
    }

    /// Horizontal distance from `point` to the middle of the river.
    pub(crate) fn distance(&self, point: Vec2) -> f32 {
        // The course never turns far from running along z, so the distance
        // across x, foreshortened by the course's slant, is close enough.
        let across = point.x - self.course_x(point.y);
        across.abs() / (1.0 + self.course_slope(point.y).powi(2)).sqrt()
    }

    /// The water level of the river abreast of `z`.
    pub(crate) fn level(&self, z: f32) -> f32 {
        let at = ((z - self.start) / STEP).max(0.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative and within the course"
        )]
        let index = (at.floor() as usize).min(self.levels.len() - 1);
        let next = (index + 1).min(self.levels.len() - 1);
        let t = (at - at.floor()).min(1.0);
        self.levels[index] + (self.levels[next] - self.levels[index]) * t
    }

    /// `height`, the ground at `point`, with the river's channel and banks
    /// carved into it.
    pub(crate) fn carve(&self, point: Vec2, height: f32) -> f32 {
        let distance = self.distance(point);
        let level = self.level(point.y);
        let carved = if distance < RIVER_HALF_WIDTH {
            let middle = 1.0 - (distance / RIVER_HALF_WIDTH).powi(2);
            level - EDGE_DEPTH - (DEPTH - EDGE_DEPTH) * middle
        } else {
            level - EDGE_DEPTH + (distance - RIVER_HALF_WIDTH) * BANK_SLOPE
        };
        height.min(carved)
    }

    /// The river's course from start to end, with its water level.
    pub(crate) fn course(&self) -> impl Iterator<Item = RiverPoint> + '_ {
        self.levels.iter().enumerate().map(|(index, &level)| {
            #[expect(clippy::cast_precision_loss, reason = "a few hundred points")]
            let z = self.start + index as f32 * STEP;
            RiverPoint {
                position: Vec2::new(self.course_x(z), z),
                level,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground that rises in the middle of the river's course.
    fn humped(point: Vec2) -> f32 {
        10.0 + 8.0 * (-(point.y / 100.0).powi(2)).exp()
    }

    #[test]
    fn water_only_runs_downhill_and_cuts_through_rises() {
        let river = River::lay(5, 1024.0, humped);
        let levels: Vec<f32> = river.course().map(|point| point.level).collect();
        assert!(levels.windows(2).all(|pair| pair[1] <= pair[0]));
        // Where the ground rises across it, the river keeps its level.
        assert!(river.level(0.0) < humped(Vec2::ZERO) - 5.0);
    }

    #[test]
    fn the_channel_holds_the_water_and_the_banks_rise_from_it() {
        let river = River::lay(5, 1024.0, humped);
        let point = river.course().nth(40).expect("a point on the course");
        let at = point.position;
        let ground = humped(at);
        assert!(river.carve(at, ground) < point.level - DEPTH + 0.01);
        let beyond = at + Vec2::X * (RIVER_HALF_WIDTH + 30.0);
        assert!((river.carve(beyond, humped(beyond)) - humped(beyond)).abs() < 1e-4);
        assert!(river.distance(at) < 0.01);
    }
}
