//! Roads: dirt tracks from the village square out across the farmland, one
//! toward each point of the compass, and a ring road around the village
//! joining them. Each is graded along its length, so its surface climbs
//! gently where the ground beside it rolls.

use std::collections::HashMap;

use glam::{IVec2, Vec2};

use crate::noise::{fractal, mix, smoothstep};

/// Half the width of a road's surface, in meters.
pub const ROAD_HALF_WIDTH: f32 = 2.0;
/// How far beyond its surface a road's grading reaches into the ground
/// beside it.
const SHOULDER: f32 = 3.5;
/// Distance between the points a road is laid through.
const STEP: f32 = 16.0;
/// Where the roads out of the village end, at the foot of the mountains.
const ROAD_LENGTH: f32 = 820.0;
/// Radius of the ring road around the village.
const RING_RADIUS: f32 = 280.0;
/// How far a road wanders from a straight line, and over what distance.
const WANDER: f32 = 18.0;
const WANDER_LENGTH: f32 = 140.0;
/// Points on each side averaged into a road's grade.
const GRADING: isize = 3;
/// Size of the cells roads are indexed by.
const CELL: f32 = 32.0;

/// The nearest road to a point, if one is close enough to shape the ground
/// there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NearestRoad {
    pub distance: f32,
    /// Height of the road's surface at the point nearest.
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    from: Vec2,
    to: Vec2,
    from_height: f32,
    to_height: f32,
}

pub(crate) struct Roads {
    segments: Vec<Segment>,
    /// Segments reaching into each cell, by index.
    cells: HashMap<IVec2, Vec<u32>>,
}

impl Roads {
    /// Lays the roads out from the village square at `village` (of radius
    /// `square`), over ground whose height is `ground`, meeting the square
    /// at `village_level`.
    pub(crate) fn lay(
        seed: u64,
        village: Vec2,
        square: f32,
        village_level: f32,
        ground: impl Fn(Vec2) -> f32,
    ) -> Self {
        let mut roads = Self {
            segments: Vec::new(),
            cells: HashMap::new(),
        };
        let directions = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X];
        for (index, direction) in directions.into_iter().enumerate() {
            let wander_seed = mix(&[seed, 0x0ad, index as u64]);
            let across = direction.perp();
            let mut points = Vec::new();
            let mut along = square;
            while along <= ROAD_LENGTH {
                // Straight out of the square, wandering once clear of it.
                let wander = fractal(wander_seed, Vec2::new(along / WANDER_LENGTH, 0.5), 2)
                    * WANDER
                    * smoothstep(40.0, 140.0, along);
                points.push(village + direction * along + across * wander);
                along += STEP;
            }
            let mut heights = graded(
                &points.iter().map(|&p| ground(p)).collect::<Vec<_>>(),
                false,
            );
            heights[0] = village_level;
            roads.add(&points, &heights, false);
        }

        let ring_seed = mix(&[seed, 0x21e6]);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few hundred points"
        )]
        let count = (std::f32::consts::TAU * RING_RADIUS / STEP).round() as usize;
        let points: Vec<Vec2> = (0..count)
            .map(|index| {
                #[expect(clippy::cast_precision_loss, reason = "a few hundred points")]
                let angle = std::f32::consts::TAU * index as f32 / count as f32;
                let around = Vec2::from_angle(angle);
                // Noise sampled on a circle closes the ring seamlessly.
                let radius = RING_RADIUS + fractal(ring_seed, around * 2.0, 2) * WANDER;
                village + around * radius
            })
            .collect();
        let heights = graded(&points.iter().map(|&p| ground(p)).collect::<Vec<_>>(), true);
        roads.add(&points, &heights, true);
        roads
    }

    fn add(&mut self, points: &[Vec2], heights: &[f32], closed: bool) {
        let count = if closed {
            points.len()
        } else {
            points.len() - 1
        };
        for index in 0..count {
            let next = (index + 1) % points.len();
            let segment = Segment {
                from: points[index],
                to: points[next],
                from_height: heights[index],
                to_height: heights[next],
            };
            let reach = Vec2::splat(ROAD_HALF_WIDTH + SHOULDER);
            let low = cell_of(segment.from.min(segment.to) - reach);
            let high = cell_of(segment.from.max(segment.to) + reach);
            let id = u32::try_from(self.segments.len()).expect("fewer than 4 billion segments");
            for z in low.y..=high.y {
                for x in low.x..=high.x {
                    self.cells.entry(IVec2::new(x, z)).or_default().push(id);
                }
            }
            self.segments.push(segment);
        }
    }

    /// The road nearest `point`, if `point` is on it or on its shoulder.
    pub(crate) fn nearest(&self, point: Vec2) -> Option<NearestRoad> {
        let mut nearest: Option<NearestRoad> = None;
        for &id in self.cells.get(&cell_of(point)).into_iter().flatten() {
            let segment = self.segments[id as usize];
            let span = segment.to - segment.from;
            let t = ((point - segment.from).dot(span) / span.length_squared()).clamp(0.0, 1.0);
            let distance = point.distance(segment.from + span * t);
            if distance <= ROAD_HALF_WIDTH + SHOULDER
                && nearest.is_none_or(|nearest| distance < nearest.distance)
            {
                nearest = Some(NearestRoad {
                    distance,
                    height: segment.from_height + (segment.to_height - segment.from_height) * t,
                });
            }
        }
        nearest
    }

    /// `height`, the ground at `point`, graded to the road it is near.
    pub(crate) fn grade(&self, point: Vec2, height: f32) -> f32 {
        match self.nearest(point) {
            Some(road) => {
                let weight =
                    1.0 - smoothstep(ROAD_HALF_WIDTH, ROAD_HALF_WIDTH + SHOULDER, road.distance);
                height + (road.height - height) * weight
            }
            None => height,
        }
    }

    /// Distance from `point` to the nearest road's middle, if it is within
    /// a road's reach.
    pub(crate) fn distance(&self, point: Vec2) -> Option<f32> {
        self.nearest(point).map(|road| road.distance)
    }
}

/// Heights averaged over `GRADING` points on each side, so a road rises and
/// falls more gently than the ground it crosses. A closed road wraps around.
fn graded(heights: &[f32], closed: bool) -> Vec<f32> {
    let count = heights.len();
    (0..count)
        .map(|index| {
            let (mut sum, mut weight) = (0.0, 0.0);
            for offset in -GRADING..=GRADING {
                let at = index.cast_signed() + offset;
                let at = if closed {
                    at.rem_euclid(count.cast_signed())
                } else if (0..count.cast_signed()).contains(&at) {
                    at
                } else {
                    continue;
                };
                sum += heights[at.cast_unsigned()];
                weight += 1.0;
            }
            sum / weight
        })
        .collect()
}

fn cell_of(point: Vec2) -> IVec2 {
    (point / CELL).floor().as_ivec2()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roads() -> Roads {
        Roads::lay(3, Vec2::ZERO, 9.0, 10.0, |point| 10.0 + point.x * 0.05)
    }

    #[test]
    fn roads_leave_the_square_level_with_it_and_run_straight_out() {
        let roads = roads();
        let start = roads
            .nearest(Vec2::new(0.0, 12.0))
            .expect("the southern road");
        assert!(start.distance < 0.01);
        let out = roads.nearest(Vec2::new(0.0, 30.0)).expect("still straight");
        assert!(out.distance < 0.01);
    }

    /// A point on the road crossing the line from `from` to `to`.
    fn crossing(roads: &Roads, from: Vec2, to: Vec2) -> Vec2 {
        (0..=100)
            .map(|step| {
                #[expect(clippy::cast_precision_loss, reason = "a small count")]
                let t = step as f32 / 100.0;
                from.lerp(to, t)
            })
            .find(|&point| roads.distance(point).is_some_and(|d| d < ROAD_HALF_WIDTH))
            .expect("a road crosses the line")
    }

    #[test]
    fn roads_are_graded_between_their_points() {
        let roads = roads();
        let on_road = crossing(&roads, Vec2::new(200.0, -30.0), Vec2::new(200.0, 30.0));
        let road = roads.nearest(on_road).expect("on the eastern road");
        assert!((road.height - 20.0).abs() < 2.0, "{}", road.height);
        // The ground beside a road is pulled toward it, and left alone past
        // its shoulder.
        assert!((roads.grade(on_road, 50.0) - road.height).abs() < 0.01);
        assert!(roads.nearest(Vec2::new(150.0, 150.0)).is_none());
    }

    #[test]
    fn the_ring_road_closes() {
        let roads = roads();
        for angle in [0.1_f32, 1.7, 3.3, 5.9] {
            let around = Vec2::from_angle(angle);
            let inner = around * (RING_RADIUS - WANDER - 1.0);
            crossing(&roads, inner, around * (RING_RADIUS + WANDER + 1.0));
        }
    }
}
