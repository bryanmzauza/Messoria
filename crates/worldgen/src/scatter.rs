//! Scenery: where trees, bushes, rocks and logs stand, grown from the seed.
//!
//! Every kind of prop is scattered over a grid of its own across the world.
//! Each cell holds one candidate, at a spot of its own, which may grow if a
//! roll against the kind's density (raised in groves, lowered in clearings,
//! and thinner across the farmland than in the wilds) succeeds and the
//! ground there suits the kind. Where two such candidates
//! would crowd each other, the one first in order (by kind, then row, then
//! column) grows and the other does not. Every decision looks only at
//! candidates within reach, so the props of a column come out the same
//! whichever other columns are worked out, and in whatever order: servers
//! and clients each grow the scenery near their players by themselves.
//!
//! The village, the roads, the river, the edge of the world and the
//! clearing where players arrive stay free.

use glam::{FloatExt, IVec2, Vec2, Vec3, Vec3Swizzles};
use messoria_content::{Catalog, PropDef, PropId};
use messoria_voxel::CHUNK_SIZE;
use rand::{RngExt, SeedableRng, rngs::SmallRng};
use serde::{Deserialize, Serialize};

use crate::{
    landscape::{HALF_WIDTH, Landscape, VILLAGE_CENTER, VILLAGE_RADIUS, column_of},
    noise::{mix, unit},
    river::RIVER_HALF_WIDTH,
    roads::ROAD_HALF_WIDTH,
};

/// Radius of the clearing around the world's origin, where players arrive.
const ARRIVAL_CLEARING: f32 = 12.0;
/// Room kept free around the village, the roads, the river and the edge of
/// the world.
const VILLAGE_MARGIN: f32 = 4.0;
const ROAD_MARGIN: f32 = 1.0;
const RIVER_MARGIN: f32 = 2.0;
const EDGE_MARGIN: f32 = 8.0;
/// Size of the groves and clearings, in meters.
const GROVE_SIZE: f32 = 28.0;
/// How thick scenery grows across the farmland, and in the wilds, as a
/// share of its kind's density.
const FARMLAND_GROWTH: f32 = 0.3;
const WILDS_GROWTH: f32 = 1.3;
/// Room kept between the footprints of two props.
const GAP: f32 = 0.8;

/// Names a prop: its kind, and the cell of its kind's grid it grows in,
/// which holds at most one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PropKey {
    pub kind: PropId,
    pub cell: [u16; 2],
}

/// A prop the seed grows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedProp {
    pub key: PropKey,
    /// Which of its kind's models it is drawn with.
    pub model: u8,
    /// The ground it stands on.
    pub position: Vec3,
    /// Rotation around the vertical axis, in radians.
    pub turn: f32,
    pub scale: f32,
}

/// Every prop growing in chunk column `column`.
pub fn props_in_column(landscape: &Landscape, catalog: &Catalog, column: IVec2) -> Vec<PlacedProp> {
    #[expect(clippy::cast_precision_loss, reason = "the chunk size is small")]
    let size = CHUNK_SIZE as f32;
    let low = column.as_vec2() * size;
    let high = low + Vec2::splat(size);
    // A prop inside the column can be crowded out by a candidate at most
    // this far outside it.
    let reach = 2.0 * largest_footprint(catalog) + GAP;
    let candidates = candidates_between(landscape, catalog, low - reach, high + reach);
    candidates
        .iter()
        .filter(|candidate| column_of(candidate.prop.position.xz()) == column)
        .filter(|candidate| {
            !candidates.iter().any(|other| {
                other.order < candidate.order
                    && other
                        .prop
                        .position
                        .xz()
                        .distance(candidate.prop.position.xz())
                        < other.footprint + candidate.footprint + GAP
            })
        })
        .map(|candidate| candidate.prop)
        .collect()
}

/// A candidate that may grow, as far as its own spot goes.
struct Candidate {
    prop: PlacedProp,
    footprint: f32,
    /// Which of two crowding candidates grows: the lower.
    order: (usize, u16, u16),
}

/// The candidates of every kind whose cells overlap the box from `low` to
/// `high` and whose spots suit them.
fn candidates_between(
    landscape: &Landscape,
    catalog: &Catalog,
    low: Vec2,
    high: Vec2,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for (index, (kind, definition)) in catalog.props().enumerate() {
        let cells = cells_across(definition.spacing);
        let cell = |meters: f32| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to the grid, a few hundred cells across"
            )]
            let cell = ((meters + HALF_WIDTH) / definition.spacing)
                .floor()
                .clamp(0.0, f32::from(cells - 1)) as u16;
            cell
        };
        for z in cell(low.y)..=cell(high.y) {
            for x in cell(low.x)..=cell(high.x) {
                if let Some(candidate) = candidate(landscape, definition, index, kind, [x, z]) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}

/// The candidate in `cell` of the grid of kind `kind` (the `index`th kind),
/// if it grows as far as its own spot goes.
fn candidate(
    landscape: &Landscape,
    definition: &PropDef,
    index: usize,
    kind: PropId,
    cell: [u16; 2],
) -> Option<Candidate> {
    let seed = landscape.seed();
    let [x, z] = cell;
    let mut rng = SmallRng::seed_from_u64(mix(&[seed, index as u64, u64::from(x), u64::from(z)]));
    let corner = Vec2::new(f32::from(x), f32::from(z)) * definition.spacing - HALF_WIDTH;
    let spot = corner + Vec2::new(rng.random(), rng.random()) * definition.spacing;
    let growth = FARMLAND_GROWTH + (WILDS_GROWTH - FARMLAND_GROWTH) * landscape.wildness(spot);
    let chance = definition.density
        * growth
        * ((1.0 - definition.clustering) + definition.clustering * groves(seed, spot));
    let scale = rng.random_range(definition.scale.0..=definition.scale.1);
    let footprint = definition.radius * scale;
    let model = rng.random_range(0..definition.models.len());
    let turn = rng.random_range(0.0..std::f32::consts::TAU);
    if rng.random::<f32>() >= chance || !free(landscape, spot, footprint) {
        return None;
    }
    let normal = landscape.normal(spot);
    let slope = normal.xz().length() / normal.y;
    if slope > definition.max_slope
        || !definition
            .grows_on
            .contains(&landscape.material_at(spot, slope))
    {
        return None;
    }
    Some(Candidate {
        prop: PlacedProp {
            key: PropKey { kind, cell },
            model: u8::try_from(model).unwrap_or(u8::MAX),
            position: spot.extend(landscape.height(spot)).xzy(),
            turn,
            scale,
        },
        footprint,
        order: (index, z, x),
    })
}

/// Whether a prop with `footprint` may stand at `spot`, clear of the
/// village, the roads, the river, the arrival clearing and the edge.
fn free(landscape: &Landscape, spot: Vec2, footprint: f32) -> bool {
    spot.abs().max_element() < HALF_WIDTH - EDGE_MARGIN
        && spot.length() >= ARRIVAL_CLEARING
        && spot.distance(VILLAGE_CENTER) >= VILLAGE_RADIUS + VILLAGE_MARGIN + footprint
        && landscape
            .road_distance(spot)
            .is_none_or(|distance| distance >= ROAD_HALF_WIDTH + ROAD_MARGIN + footprint)
        && landscape.river_distance(spot) >= RIVER_HALF_WIDTH + RIVER_MARGIN + footprint
}

/// Cells across the world in a grid of `spacing`-meter cells.
fn cells_across(spacing: f32) -> u16 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the content keeps spacings to a few meters and more"
    )]
    let cells = (2.0 * HALF_WIDTH / spacing).ceil() as u16;
    cells
}

/// The largest footprint any prop may have.
fn largest_footprint(catalog: &Catalog) -> f32 {
    catalog
        .props()
        .map(|(_, definition)| definition.radius * definition.scale.1)
        .fold(0.0, f32::max)
}

/// How much of a grove `point` is in, from 0 in a clearing to about 2 in the
/// thick of one, averaging about 1 over the world. Smooth value noise over
/// a grid of `GROVE_SIZE` cells, sharpened into groves and clearings.
fn groves(seed: u64, point: Vec2) -> f32 {
    let grid = point / GROVE_SIZE;
    let cell = grid.floor();
    let t = grid - cell;
    let t = t * t * (Vec2::splat(3.0) - 2.0 * t);
    let corner = |dx: f32, dz: f32| {
        let at = (cell + Vec2::new(dx, dz)).as_ivec2();
        let [x, z] = at
            .to_array()
            .map(|coordinate| u64::from(coordinate.cast_unsigned()));
        unit(mix(&[seed, 0x9e37, x, z]))
    };
    let noise = corner(0.0, 0.0)
        .lerp(corner(1.0, 0.0), t.x)
        .lerp(corner(0.0, 1.0).lerp(corner(1.0, 1.0), t.x), t.y);
    let t = ((noise - 0.3) / 0.4).clamp(0.0, 1.0);
    2.0 * t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn catalog() -> Catalog {
        let data = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data");
        Catalog::load(std::path::Path::new(data)).expect("the shipped content is valid")
    }

    /// The props of the columns in a square `radius` around `center`.
    fn props_around(
        landscape: &Landscape,
        catalog: &Catalog,
        center: IVec2,
        radius: i32,
    ) -> Vec<PlacedProp> {
        (-radius..=radius)
            .flat_map(|z| (-radius..=radius).map(move |x| center + IVec2::new(x, z)))
            .flat_map(|column| props_in_column(landscape, catalog, column))
            .collect()
    }

    #[test]
    fn a_seed_always_grows_the_same_scenery() {
        let catalog = catalog();
        let (a, b, c) = (Landscape::new(7), Landscape::new(7), Landscape::new(8));
        let column = IVec2::new(9, 4);
        assert_eq!(
            props_in_column(&a, &catalog, column),
            props_in_column(&b, &catalog, column)
        );
        assert_ne!(
            props_in_column(&a, &catalog, column),
            props_in_column(&c, &catalog, column)
        );
    }

    #[test]
    fn props_never_crowd_each_other_even_across_columns() {
        let catalog = catalog();
        let landscape = Landscape::new(7);
        let props = props_around(&landscape, &catalog, IVec2::new(8, 5), 2);
        assert!(props.len() > 40, "only {} props", props.len());
        let footprint = |prop: &PlacedProp| catalog.prop(prop.key.kind).radius * prop.scale;
        for (index, prop) in props.iter().enumerate() {
            for other in &props[index + 1..] {
                assert!(
                    prop.position.xz().distance(other.position.xz())
                        >= footprint(prop) + footprint(other),
                    "props overlap at {}",
                    prop.position
                );
            }
        }
        let named: HashSet<PropKey> = props.iter().map(|prop| prop.key).collect();
        assert_eq!(named.len(), props.len(), "a key names one prop");
    }

    #[test]
    fn every_kind_grows_and_the_open_places_stay_open() {
        let catalog = catalog();
        let landscape = Landscape::new(7);
        let mut props = props_around(&landscape, &catalog, IVec2::new(0, 0), 4);
        props.extend(props_around(&landscape, &catalog, IVec2::new(-14, 12), 4));
        for (kind, definition) in catalog.props() {
            assert!(
                props.iter().any(|prop| prop.key.kind == kind),
                "no {} grew",
                definition.name
            );
        }
        for prop in &props {
            let spot = prop.position.xz();
            assert!(spot.length() >= ARRIVAL_CLEARING);
            assert!(spot.distance(VILLAGE_CENTER) >= VILLAGE_RADIUS);
            assert!(
                landscape
                    .road_distance(spot)
                    .is_none_or(|d| d > ROAD_HALF_WIDTH)
            );
            assert!((prop.position.y - landscape.height(spot)).abs() < 1e-4);
        }
    }
}
