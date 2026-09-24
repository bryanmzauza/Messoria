//! Scenery: trees, bushes and rocks scattered across the valley.
//!
//! The same seed always scatters the same scenery, over the valley as it was
//! generated, so a world does not save it; it saves only which props players
//! gathered. Each kind of prop is scattered over a grid of its own: every
//! cell has a chance of one prop, higher in groves and lower in clearings,
//! placed at a random spot in the cell if the ground there suits it and
//! nothing else stands too close. The village and the clearing where players
//! arrive stay free. A kind's cell and the kind name a prop in the save.
//!
//! Nobody can dig, raise or till the ground a prop stands on, until it is
//! gathered and leaves nothing standing.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::{Catalog, PropId};
use messoria_shared::{
    content::Content,
    protocol::{Gathered, Prop},
    terrain::Terrain,
    village,
};
use messoria_voxel::ChunkMap;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::{
    Beginning, WorldSeed, WorldStart,
    terrain::{WorldBuilding, ground_height, half_width},
};

/// Radius of the clearing around the world's origin, where players arrive.
const ARRIVAL_CLEARING: f32 = 12.0;
/// Space kept free around the village's protected ground.
const VILLAGE_MARGIN: f32 = 4.0;
/// Size of the groves and clearings, in meters.
const GROVE_SIZE: f32 = 28.0;
/// Space kept between the footprints of two props.
const GAP: f32 = 0.8;
/// Size of the cells the placed props are indexed by.
const INDEX_CELL: f32 = 4.0;

pub(crate) struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scenery>().add_systems(
            Startup,
            grow_scenery
                .after(WorldBuilding::Generate)
                .before(WorldBuilding::Restore),
        );
    }
}

/// The cell of its kind's scattering grid a prop grew in.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneryCell(pub [u16; 2]);

/// Where props stand, to keep the ground under them as it is and to find the
/// prop a player works on.
#[derive(Resource, Default)]
pub(crate) struct Scenery {
    /// Footprints, indexed by the cell their center is in.
    footprints: HashMap<IVec2, Vec<Footprint>>,
    largest_footprint: f32,
}

#[derive(Clone, Copy, Debug)]
struct Footprint {
    center: Vec2,
    radius: f32,
    prop: Entity,
}

impl Scenery {
    /// Whether a disc of `radius` around `point` reaches the ground under a
    /// prop.
    pub(crate) fn blocks(&self, point: Vec3, radius: f32) -> bool {
        self.near(point.xz(), radius).next().is_some()
    }

    /// The prop whose footprint is nearest `point`, if one is within `slack`
    /// of it.
    pub(crate) fn prop_at(&self, point: Vec3, slack: f32) -> Option<Entity> {
        let center = point.xz();
        self.near(center, slack)
            .min_by(|a, b| {
                let gap =
                    |footprint: &Footprint| footprint.center.distance(center) - footprint.radius;
                gap(a).total_cmp(&gap(b))
            })
            .map(|footprint| footprint.prop)
    }

    /// Frees the ground under `prop`, which no longer stands there.
    pub(crate) fn clear(&mut self, prop: Entity) {
        for footprints in self.footprints.values_mut() {
            footprints.retain(|footprint| footprint.prop != prop);
        }
    }

    /// Footprints a disc of `radius` around `center` reaches.
    fn near(&self, center: Vec2, radius: f32) -> impl Iterator<Item = &Footprint> {
        let reach = radius + self.largest_footprint;
        let (min, max) = (cell_of(center - reach), cell_of(center + reach));
        (min.y..=max.y)
            .flat_map(move |z| (min.x..=max.x).map(move |x| IVec2::new(x, z)))
            .filter_map(|cell| self.footprints.get(&cell))
            .flatten()
            .filter(move |footprint| footprint.center.distance(center) < radius + footprint.radius)
    }

    /// Whether a prop with `footprint` at `center` would come too close to
    /// one already placed.
    fn crowds(&self, center: Vec2, footprint: f32) -> bool {
        self.near(center, footprint + GAP).next().is_some()
    }

    fn insert(&mut self, footprint: Footprint) {
        self.largest_footprint = self.largest_footprint.max(footprint.radius);
        self.footprints
            .entry(cell_of(footprint.center))
            .or_default()
            .push(footprint);
    }
}

fn cell_of(point: Vec2) -> IVec2 {
    (point / INDEX_CELL).floor().as_ivec2()
}

/// A prop the seed grew, and where in its kind's grid.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grown {
    entity: Entity,
    prop: Prop,
    cell: [u16; 2],
}

fn grow_scenery(
    content: Res<Content>,
    seed: Res<WorldSeed>,
    beginning: Res<Beginning>,
    terrain: Res<Terrain>,
    mut scenery: ResMut<Scenery>,
    mut commands: Commands,
) {
    let grown = scatter(seed.0, &content, &terrain, free, &mut scenery, || {
        commands.spawn_empty().id()
    });
    let gathered: HashMap<(PropId, [u16; 2]), u32> = match &beginning.0 {
        WorldStart::New { .. } => HashMap::new(),
        WorldStart::Resume(saved) => saved
            .world
            .gathered
            .iter()
            .map(|gathered| ((gathered.kind, gathered.cell), gathered.day))
            .collect(),
    };
    info!(
        "grew {} props of scenery, {} of them gathered",
        grown.len(),
        gathered.len()
    );
    for Grown { entity, prop, cell } in grown {
        let mut spawned = commands.entity(entity);
        spawned.insert((
            prop,
            SceneryCell(cell),
            Replicate::to_clients(NetworkTarget::All),
        ));
        if let Some(&day) = gathered.get(&(prop.kind, cell)) {
            spawned.insert(Gathered { day });
            if !content.prop(prop.kind).stands_when_gathered() {
                scenery.clear(entity);
            }
        }
    }
}

/// Where the seed may grow scenery: away from the arrival clearing and the
/// village.
fn free(point: Vec2) -> bool {
    point.length() >= ARRIVAL_CLEARING && !village::reaches(point.extend(0.0).xzy(), VILLAGE_MARGIN)
}

/// Scatters every kind of prop over the valley, where `free` allows,
/// recording their footprints in `scenery`. Each prop grown gets an entity
/// from `entity`.
fn scatter(
    seed: u64,
    content: &Catalog,
    terrain: &ChunkMap,
    free: impl Fn(Vec2) -> bool,
    scenery: &mut Scenery,
    mut entity: impl FnMut() -> Entity,
) -> Vec<Grown> {
    let half_width = half_width();
    let mut grown = Vec::new();
    for (index, (kind, definition)) in content.props().enumerate() {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the world is a few hundred cells across"
        )]
        let cells = (2.0 * half_width / definition.spacing).ceil() as u16;
        for z in 0..cells {
            for x in 0..cells {
                let mut rng =
                    SmallRng::seed_from_u64(mix(&[seed, index as u64, u64::from(x), u64::from(z)]));
                let corner =
                    Vec2::new(f32::from(x), f32::from(z)) * definition.spacing - half_width;
                let spot = corner + Vec2::new(rng.random(), rng.random()) * definition.spacing;
                let grove = groves(seed, spot);
                let chance = definition.density
                    * ((1.0 - definition.clustering) + definition.clustering * grove);
                let scale = rng.random_range(definition.scale.0..=definition.scale.1);
                let footprint = definition.radius * scale;
                let model = rng.random_range(0..definition.models.len());
                let turn = rng.random_range(0.0..std::f32::consts::TAU);
                if rng.random::<f32>() >= chance || !free(spot) || scenery.crowds(spot, footprint) {
                    continue;
                }
                let Some(ground) = ground_height(terrain, spot.x, spot.y) else {
                    continue;
                };
                let point = spot.extend(ground).xzy();
                let suits = terrain.normal(point).is_some_and(|normal| {
                    normal.y > 0.0 && normal.xz().length() / normal.y <= definition.max_slope
                }) && terrain
                    .surface_material(point)
                    .is_some_and(|material| definition.grows_on.contains(&material));
                if !suits {
                    continue;
                }
                let prop_entity = entity();
                scenery.insert(Footprint {
                    center: spot,
                    radius: footprint,
                    prop: prop_entity,
                });
                grown.push(Grown {
                    entity: prop_entity,
                    prop: Prop {
                        kind,
                        model: u8::try_from(model).unwrap_or(u8::MAX),
                        position: point,
                        turn,
                        scale,
                    },
                    cell: [x, z],
                });
            }
        }
    }
    grown
}

/// How much of a grove `point` is in, from 0 in a clearing to about 2 in the
/// thick of one, averaging about 1 over the valley. Smooth value noise over a
/// grid of `GROVE_SIZE` cells.
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
        #[expect(clippy::cast_precision_loss, reason = "only the top bits matter")]
        let value = (mix(&[seed, 0x9e37, x, z]) >> 40) as f32 / (1u64 << 24) as f32;
        value
    };
    let noise = corner(0.0, 0.0)
        .lerp(corner(1.0, 0.0), t.x)
        .lerp(corner(0.0, 1.0).lerp(corner(1.0, 1.0), t.x), t.y);
    // Sharpen the noise into distinct groves and clearings.
    let t = ((noise - 0.3) / 0.4).clamp(0.0, 1.0);
    2.0 * t * t * (3.0 - 2.0 * t)
}

/// Mixes numbers into one well-spread seed, with steps of `SplitMix64`.
fn mix(values: &[u64]) -> u64 {
    values.iter().fold(0x6d65_7373_6f72_6961, |state, &value| {
        let mut z = (state ^ value).wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

#[cfg(test)]
mod tests {
    use messoria_shared::content::load_content;

    use super::*;
    use crate::terrain::farm;

    fn grow(seed: u64) -> (Vec<Grown>, Scenery) {
        let content = load_content().expect("the shipped content is valid");
        let mut scenery = Scenery::default();
        let mut next = 0;
        let grown = scatter(seed, &content, &farm(), free, &mut scenery, || {
            next += 1;
            Entity::from_raw_u32(next).expect("a valid index")
        });
        (grown, scenery)
    }

    #[test]
    fn a_seed_always_grows_the_same_scenery() {
        let (first, _) = grow(7);
        let (again, _) = grow(7);
        let (other, _) = grow(8);
        assert_eq!(first, again);
        assert_ne!(first, other);
    }

    #[test]
    fn every_kind_grows_and_none_crowds_another() {
        let content = load_content().expect("the shipped content is valid");
        let (grown, _) = grow(7);
        assert!((500..5_000).contains(&grown.len()), "{} props", grown.len());
        for (kind, definition) in content.props() {
            assert!(
                grown.iter().any(|grown| grown.prop.kind == kind),
                "no {} grew",
                definition.name
            );
        }
        let footprint = |prop: &Prop| content.prop(prop.kind).radius * prop.scale;
        for (index, Grown { prop, .. }) in grown.iter().enumerate() {
            assert!(free(prop.position.xz()), "a prop grew at {}", prop.position);
            for Grown { prop: other, .. } in &grown[index + 1..] {
                let apart = prop.position.xz().distance(other.position.xz());
                assert!(
                    apart >= footprint(prop) + footprint(other),
                    "props overlap at {}",
                    prop.position
                );
            }
        }
    }

    #[test]
    fn a_kind_and_a_cell_name_one_prop() {
        let (grown, _) = grow(7);
        let mut named = std::collections::HashSet::new();
        for grown in &grown {
            assert!(named.insert((grown.prop.kind, grown.cell)));
        }
    }

    #[test]
    fn the_ground_under_props_is_kept_until_they_are_cleared() {
        let (grown, mut scenery) = grow(7);
        let Grown { entity, prop, .. } = grown[0];
        assert!(scenery.blocks(prop.position, 0.0));
        assert!(scenery.blocks(prop.position + Vec3::new(1.0, 0.0, 0.0), 1.5));
        assert_eq!(scenery.prop_at(prop.position, 0.5), Some(entity));
        assert!(
            !scenery.blocks(Vec3::ZERO, 1.5),
            "the arrival clearing is free"
        );

        scenery.clear(entity);
        assert!(!scenery.blocks(prop.position, 0.0));
        assert_eq!(scenery.prop_at(prop.position, 0.0), None);
    }
}
