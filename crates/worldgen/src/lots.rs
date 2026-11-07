//! Farm lots: the fenced squares of land along the roads out of the
//! village, one of which every player farms.
//!
//! The lots are laid out the same in every world, three to a side along
//! each of the four roads leaving the village, each with its gate on the
//! road. The roads run straight through the farmland so the fences can lie
//! beside them. Who owns a lot is not decided here; this is only where the
//! lots and their fences are.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{landscape::VILLAGE_CENTER, roads::ROAD_HALF_WIDTH};

/// Side of a lot, in meters.
pub const LOT_SIZE: f32 = 64.0;
/// Width of the opening in the fence on the road side.
pub const GATE_WIDTH: f32 = 4.0;
/// Length of one section of fence, from one post to the next.
pub const FENCE_SECTION: f32 = 2.0;
/// Where along each road the lots start, from the middle of the village:
/// the first clear of the lots along the road across, the rest a lane apart.
const LOT_STARTS: [f32; 3] = [72.0, 144.0, 216.0];
/// How far from the middle of a road its lots' fences stand.
const SETBACK: f32 = ROAD_HALF_WIDTH + 1.5;
/// The roads run straight this far out, past the last lot.
pub(crate) const LOTS_REACH: f32 = 290.0;
/// The roads out of the village, in the order their lots are numbered.
const ROADS: [Vec2; 4] = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X];
/// Lots in the world: both sides of every road, with a lot at each start.
pub const LOT_COUNT: u16 = 4 * 2 * 3;

/// Names a lot, from 0 up to [`LOT_COUNT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LotId(pub u16);

impl LotId {
    /// The number players know the lot by, counting from one.
    pub fn number(self) -> u16 {
        self.0 + 1
    }
}

/// A lot: a square of land with a gate in the fence on its road side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lot {
    pub id: LotId,
    /// Its corners nearest and farthest from the world's origin.
    pub min: Vec2,
    pub max: Vec2,
    /// The middle of the opening in its fence, on the lot's edge.
    pub gate: Vec2,
    /// Which way the road runs past the gate.
    pub along_road: Vec2,
}

/// A straight stretch of fence between two points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FenceRun {
    pub from: Vec2,
    pub to: Vec2,
}

impl FenceRun {
    pub fn length(&self) -> f32 {
        self.from.distance(self.to)
    }

    /// Distance from `point` to the fence.
    pub fn distance(&self, point: Vec2) -> f32 {
        let span = self.to - self.from;
        let t = ((point - self.from).dot(span) / span.length_squared()).clamp(0.0, 1.0);
        point.distance(self.from + span * t)
    }

    /// The sections of the run, from one post to the next, as the point
    /// each starts at and the direction it runs in.
    pub fn sections(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        let direction = (self.to - self.from).normalize();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "runs are a few dozen meters"
        )]
        let count = (self.length() / FENCE_SECTION).round() as u32;
        (0..count).map(move |index| {
            #[expect(clippy::cast_precision_loss, reason = "a few dozen sections")]
            let along = index as f32 * FENCE_SECTION;
            (self.from + direction * along, direction)
        })
    }
}

impl Lot {
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.cmpge(self.min).all() && point.cmplt(self.max).all()
    }

    /// Distance from `point` to the lot's ground, zero inside it.
    pub fn distance(&self, point: Vec2) -> f32 {
        (self.min - point)
            .max(point - self.max)
            .max(Vec2::ZERO)
            .length()
    }

    /// The fence around the lot: a run along each side, the road side in
    /// two with the gate between them.
    pub fn fences(&self) -> Vec<FenceRun> {
        let (min, max) = (self.min, self.max);
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        let mut runs = Vec::with_capacity(5);
        for index in 0..4 {
            let (from, to) = (corners[index], corners[(index + 1) % 4]);
            let run = FenceRun { from, to };
            if run.distance(self.gate) < 0.01 {
                let half_gate = (to - from).normalize() * (GATE_WIDTH / 2.0);
                runs.push(FenceRun {
                    from,
                    to: self.gate - half_gate,
                });
                runs.push(FenceRun {
                    from: self.gate + half_gate,
                    to,
                });
            } else {
                runs.push(run);
            }
        }
        runs
    }

    /// Where the two gateposts stand.
    pub fn gateposts(&self) -> [Vec2; 2] {
        let half = self.along_road * (GATE_WIDTH / 2.0);
        [self.gate - half, self.gate + half]
    }
}

/// Every lot in the world, in order of their ids.
pub fn lots() -> impl Iterator<Item = Lot> {
    (0..LOT_COUNT).map(LotId).map(lot)
}

/// The lot `id` names.
///
/// # Panics
///
/// If `id` is not below [`LOT_COUNT`].
pub fn lot(id: LotId) -> Lot {
    assert!(id.0 < LOT_COUNT, "lot {} does not exist", id.0);
    let slots = LOT_STARTS.len();
    let (road, side, slot) = (
        usize::from(id.0) / (2 * slots),
        (usize::from(id.0) / slots) % 2,
        usize::from(id.0) % slots,
    );
    let direction = ROADS[road];
    // The right-hand side of the road, going out, comes first.
    let across = if side == 0 {
        direction.perp()
    } else {
        -direction.perp()
    };
    let start = LOT_STARTS[slot];
    let middle = VILLAGE_CENTER + direction * (start + LOT_SIZE / 2.0);
    let center = middle + across * (SETBACK + LOT_SIZE / 2.0);
    let half = Vec2::splat(LOT_SIZE / 2.0);
    Lot {
        id,
        min: center - half,
        max: center + half,
        gate: middle + across * SETBACK,
        along_road: direction,
    }
}

/// The lot `point` lies in, if any.
pub fn lot_at(point: Vec2) -> Option<LotId> {
    lots().find(|lot| lot.contains(point)).map(|lot| lot.id)
}

/// Distance from `point` to the nearest fence.
pub fn fence_distance(point: Vec2) -> f32 {
    lots()
        .filter(|lot| lot.distance(point) < LOT_SIZE)
        .flat_map(|lot| lot.fences())
        .map(|run| run.distance(point))
        .fold(f32::INFINITY, f32::min)
}

/// Distance from `point` to the nearest gate's middle.
pub fn gate_distance(point: Vec2) -> f32 {
    lots()
        .map(|lot| lot.gate.distance(point))
        .fold(f32::INFINITY, f32::min)
}

/// Distance from `point` to the nearest lot's middle, where its owner's
/// cabin has room.
pub fn homestead_distance(point: Vec2) -> f32 {
    lots()
        .map(|lot| lot.center().distance(point))
        .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Landscape, landscape::VILLAGE_RADIUS};

    #[test]
    fn lots_are_numbered_squares_that_do_not_overlap() {
        let all: Vec<Lot> = lots().collect();
        assert_eq!(all.len(), usize::from(LOT_COUNT));
        assert_eq!(all.len(), ROADS.len() * 2 * LOT_STARTS.len());
        for (index, lot) in all.iter().enumerate() {
            assert_eq!(lot.id, LotId(u16::try_from(index).expect("few lots")));
            assert_eq!(lot.max - lot.min, Vec2::splat(LOT_SIZE));
            assert!(lot.contains(lot.center()));
            assert_eq!(lot_at(lot.center()), Some(lot.id));
            for other in &all[index + 1..] {
                assert!(
                    lot.distance(other.center()) > LOT_SIZE / 2.0,
                    "lots {} and {} overlap",
                    lot.id.number(),
                    other.id.number()
                );
            }
        }
        assert_eq!(lot_at(VILLAGE_CENTER), None);
        assert_eq!(lot_at(Vec2::ZERO), None);
    }

    #[test]
    fn gates_open_onto_the_roads_and_fences_keep_off_them() {
        let landscape = Landscape::new(7);
        for lot in lots() {
            let just_outside = lot.gate - (lot.center() - lot.gate).normalize() * 2.0;
            assert!(
                landscape
                    .road_distance(just_outside)
                    .is_some_and(|distance| distance < ROAD_HALF_WIDTH),
                "lot {} has no road at its gate",
                lot.id.number()
            );
            assert!(lot.distance(VILLAGE_CENTER) > VILLAGE_RADIUS);
            for run in lot.fences() {
                for (start, _) in run.sections() {
                    assert!(
                        landscape
                            .road_distance(start)
                            .is_none_or(|distance| distance >= ROAD_HALF_WIDTH),
                        "a fence of lot {} stands on a road at {start}",
                        lot.id.number()
                    );
                }
            }
        }
    }

    #[test]
    fn a_fence_has_five_runs_with_whole_sections_and_a_gap_for_the_gate() {
        let lot = lot(LotId(5));
        let runs = lot.fences();
        assert_eq!(runs.len(), 5);
        let total: f32 = runs.iter().map(FenceRun::length).sum();
        assert!((total - (4.0 * LOT_SIZE - GATE_WIDTH)).abs() < 1e-3);
        for run in &runs {
            let sections = run.sections().count();
            #[expect(clippy::cast_precision_loss, reason = "a few dozen sections")]
            let covered = sections as f32 * FENCE_SECTION;
            assert!((covered - run.length()).abs() < 1e-3, "{run:?}");
        }
        assert!(fence_distance(lot.gate) > GATE_WIDTH / 2.0 - 0.01);
        assert!(fence_distance(lot.min) < 0.01);
        assert!(fence_distance(lot.center()) > LOT_SIZE / 2.0 - 0.01);
        assert!(gate_distance(lot.gate) < 1e-4);
    }
}
