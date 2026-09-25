//! Building structures: placing a cabin or a chest, checking the ground is
//! fit for it, levelling it, and keeping the ground of what stands.
//!
//! Using a structure's item on the ground builds it there, its front toward
//! whoever builds it. A structure that levels the ground needs open ground
//! that is not too uneven, clear of props, fields, other structures and
//! people, even around it, where the levelled ground eases back into the
//! valley. Smaller structures stand on nearly flat ground, outside or on
//! the floor of a structure that levelled its ground, such as a chest in a
//! cabin, but never on what stands there.

use bevy::{
    ecs::{message::Message, system::SystemParam},
    prelude::*,
};
use lightyear::prelude::*;
use messoria_content::{Catalog, Purpose, StructureDef};
use messoria_inventory::Inventory;
use messoria_shared::{
    content::Content,
    fields::tile_center,
    movement::{BODY_RADIUS, EYE_HEIGHT},
    obstacles::Obstacles,
    protocol::{
        Asleep, Belongings, Field, Happened, Heading, Notice, PlayerId, Position, Stored, Structure,
    },
    scenery::Scenery,
    structures,
    terrain::{ChunkChanged, Terrain},
    tools, village,
};
use messoria_voxel::{Levelling, Material};

use crate::{
    Beginning, WorldStart,
    feedback::{Show, Tell},
    inventory::ItemUseSystems,
    players::player_key,
    terrain::{Restoring, TerrainEdited, reshape},
};

/// Height between the levels ground is levelled to, as the shovel's.
const LEVEL_STEP: f32 = tools::SHOVEL_STEP;
/// Most the ground under a structure that levels it may rise and fall.
const MOST_LEVELLED: f32 = 1.5;
/// Most the ground under any other structure may rise and fall.
const MOST_UNEVEN: f32 = 0.35;
/// Width of the band around levelled ground that eases back to the valley.
const LEVELLING_MARGIN: f32 = 1.5;
/// Space between samples of the ground under a structure, in meters.
const SAMPLE_SPACING: f32 = 1.0;
/// Room kept around a structure for people standing near it.
const PERSONAL_SPACE: f32 = 0.5;

pub(crate) struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BuildUse>()
            .add_systems(Startup, rebuild_saved.after(Restoring))
            .add_systems(PreUpdate, build.after(ItemUseSystems));
    }
}

/// A character building the structure of the item in `slot` at `target`.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct BuildUse {
    pub character: Entity,
    pub slot: usize,
    pub target: Vec3,
}

/// The player whose home a structure is, by their key.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Home(pub String);

/// Everything built, to keep the ground under it and to find it.
#[derive(SystemParam)]
pub(crate) struct Built<'w, 's> {
    content: Res<'w, Content>,
    structures: Query<'w, 's, (Entity, &'static Structure)>,
}

impl Built<'_, '_> {
    /// Whether a disc of `radius` around `point` reaches the ground of
    /// something built.
    pub(crate) fn covers(&self, point: Vec3, radius: f32) -> bool {
        self.structures.iter().any(|(_, structure)| {
            structure.covers(self.content.structure(structure.kind), point.xz(), radius)
        })
    }

    /// The structure for `purpose` whose middle is nearest `point`, within
    /// `reach` of it.
    pub(crate) fn nearest(
        &self,
        purpose: Purpose,
        point: Vec3,
        reach: f32,
    ) -> Option<(Entity, Structure)> {
        self.structures
            .iter()
            .filter(|(_, structure)| self.content.structure(structure.kind).purpose == purpose)
            .map(|(entity, structure)| (entity, *structure, structure.position.distance(point)))
            .filter(|&(_, _, distance)| distance <= reach)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(entity, structure, _)| (entity, structure))
    }

    /// The structures standing on the ground of `container`.
    pub(crate) fn within<'a>(
        &'a self,
        container: &'a Structure,
    ) -> impl Iterator<Item = (Entity, Structure)> + 'a {
        let definition = self.content.structure(container.kind);
        self.structures
            .iter()
            .filter(move |(_, structure)| {
                *structure != container
                    && container.covers(definition, structure.position.xz(), 0.0)
            })
            .map(|(entity, structure)| (entity, *structure))
    }

    fn all(&self) -> impl Iterator<Item = Structure> + '_ {
        self.structures.iter().map(|(_, structure)| *structure)
    }
}

/// Spawns `structure`, keeping `stored` in it if it keeps anything.
fn raise(
    commands: &mut Commands,
    content: &Catalog,
    structure: Structure,
    stored: Option<Inventory>,
) -> Entity {
    let definition = content.structure(structure.kind);
    let mut spawned = commands.spawn((
        Name::new(definition.name.clone()),
        structure,
        Replicate::to_clients(NetworkTarget::All),
    ));
    if definition.purpose == Purpose::Storage {
        spawned.insert(Stored(stored.unwrap_or_default()));
    }
    spawned.id()
}

/// Raises everything the save says was built.
fn rebuild_saved(beginning: Res<Beginning>, content: Res<Content>, mut commands: Commands) {
    let WorldStart::Resume(saved) = &beginning.0 else {
        return;
    };
    for state in &saved.world.structures {
        let structure = Structure {
            kind: state.kind,
            position: state.position,
            facing: state.facing,
        };
        let entity = raise(&mut commands, &content, structure, state.stored.clone());
        if let Some(owner) = &state.home_of {
            commands.entity(entity).insert(Home(owner.clone()));
        }
    }
}

fn build(
    content: Res<Content>,
    mut uses: MessageReader<BuildUse>,
    characters: Query<(&Position, &Heading, &PlayerId)>,
    mut workers: Query<&mut Belongings, Without<Asleep>>,
    homes: Query<&Home>,
    scenery: Res<Scenery>,
    obstacles: Res<Obstacles>,
    fields: Query<&Field>,
    built: Built,
    mut terrain: ResMut<Terrain>,
    mut edited: MessageWriter<TerrainEdited>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut commands: Commands,
) {
    for work in uses.read() {
        let (Ok((feet, heading, player)), Ok(mut belongings)) = (
            characters.get(work.character),
            workers.get_mut(work.character),
        ) else {
            continue;
        };
        if !work.target.is_finite() || !tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, work.target)
        {
            continue;
        }
        let Some(kind) = belongings
            .0
            .slot(work.slot)
            .and_then(|stack| content.structure_built_from(stack.item))
        else {
            continue;
        };
        let definition = content.structure(kind);
        let owner = player_key(player.0);
        if definition.home
            && homes
                .iter()
                .any(|home| owner.as_ref().is_some_and(|owner| *owner == home.0))
        {
            continue;
        }

        let plan = structures::planned(kind, definition, work.target, heading.0);
        let site = Site {
            content: &content,
            terrain: &terrain,
            scenery: &scenery,
            obstacles: &obstacles,
            built: &built,
        };
        let people = characters.iter().map(|(position, ..)| position.0);
        let fields = fields.iter().map(|field| tile_center(field.tile));
        let height = match site.check(definition, plan, people, fields) {
            Ok(height) => height,
            Err(notice) => {
                tell.write(Tell {
                    character: work.character,
                    notice,
                });
                continue;
            }
        };

        belongings.0.take_one(work.slot);
        let structure = Structure {
            position: plan.position.with_y(height),
            ..plan
        };
        if definition.levels_ground {
            let levelling = Levelling {
                center: structure.position.xz(),
                half_size: half_size(definition),
                turn: structure.facing,
                height,
                margin: LEVELLING_MARGIN,
                surface: Material::Soil,
            };
            reshape(&mut terrain, &levelling, &mut edited, &mut chunk_changed);
        }
        let entity = raise(&mut commands, &content, structure, None);
        if definition.home
            && let Some(owner) = owner
        {
            commands.entity(entity).insert(Home(owner));
        }
        for inside in structure.contained(definition) {
            raise(&mut commands, &content, inside, None);
        }
        show.write(Show::at(
            Happened::Built,
            structure.position,
            work.character,
        ));
    }
}

fn half_size(definition: &StructureDef) -> Vec2 {
    Vec2::new(definition.size.0, definition.size.1) / 2.0
}

/// What decides whether a structure can be built somewhere.
struct Site<'a> {
    content: &'a Catalog,
    terrain: &'a Terrain,
    scenery: &'a Scenery,
    obstacles: &'a Obstacles,
    built: &'a Built<'a, 'a>,
}

impl Site<'_> {
    /// The height `plan` stands at if it can be built, or why it cannot.
    /// `people` are where characters stand, and `fields` the middles of
    /// tilled squares.
    fn check(
        &self,
        definition: &StructureDef,
        plan: Structure,
        people: impl Iterator<Item = Vec3>,
        fields: impl Iterator<Item = Vec2>,
    ) -> Result<f32, Notice> {
        let half = half_size(definition);
        let margin = if definition.levels_ground {
            LEVELLING_MARGIN
        } else {
            0.0
        };
        if village::reaches(plan.position, half.length() + margin) {
            return Err(Notice::ProtectedGround);
        }

        let samples = footprint_samples(half).map(|local| plan.to_world(local.extend(0.0).xzy()));
        let heights = samples
            .map(|point| self.terrain.surface_below(point + Vec3::Y * 4.0, 8.0))
            .collect::<Option<Vec<f32>>>()
            .ok_or(Notice::GroundTooUneven)?;
        let (lowest, highest) = heights
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), &h| {
                (low.min(h), high.max(h))
            });
        let most = if definition.levels_ground {
            MOST_LEVELLED
        } else {
            MOST_UNEVEN
        };
        if highest - lowest > most {
            return Err(Notice::GroundTooUneven);
        }
        #[expect(clippy::cast_precision_loss, reason = "a few dozen samples")]
        let height = if definition.levels_ground {
            let mean = heights.iter().sum::<f32>() / heights.len() as f32;
            (mean / LEVEL_STEP).round() * LEVEL_STEP
        } else {
            heights[0]
        };

        let reach = half.length() + margin;
        let near = |point: Vec2, radius: f32| plan.covers(definition, point, radius + margin);
        let floor = self.floor_under(definition, plan);
        if floor.is_none()
            && self
                .scenery
                .any_near(plan.position.xz(), reach, |center, radius| {
                    near(center, radius)
                })
        {
            return Err(Notice::NoRoomToBuild);
        }
        if fields.into_iter().any(|tile| near(tile, 0.71)) {
            return Err(Notice::NoRoomToBuild);
        }
        if !self.clear_of_structures(definition, plan, floor, margin) {
            return Err(Notice::NoRoomToBuild);
        }
        let blocked = footprint_samples(half).any(|local| {
            let point = plan.to_world(local.extend(0.0).xzy()).with_y(height);
            self.obstacles.push_out(point, 0.05, 1.0) != point
        });
        if blocked {
            return Err(Notice::NoRoomToBuild);
        }
        if people
            .into_iter()
            .any(|feet| near(feet.xz(), BODY_RADIUS + PERSONAL_SPACE))
        {
            return Err(Notice::WouldBuryPlayer);
        }
        Ok(height)
    }

    /// The structure that levelled the ground `plan` would stand on, if it
    /// stands wholly on levelled ground.
    fn floor_under(&self, definition: &StructureDef, plan: Structure) -> Option<Structure> {
        if definition.levels_ground {
            return None;
        }
        let corners =
            corners(half_size(definition)).map(|local| plan.to_world(local.extend(0.0).xzy()).xz());
        self.built.all().find(|structure| {
            let under = self.content.structure(structure.kind);
            under.levels_ground
                && corners
                    .iter()
                    .all(|&corner| structure.covers(under, corner, 0.0))
        })
    }

    /// Whether `plan` keeps clear of the ground of everything built, but for
    /// the floor it stands on.
    fn clear_of_structures(
        &self,
        definition: &StructureDef,
        plan: Structure,
        floor: Option<Structure>,
        margin: f32,
    ) -> bool {
        let own =
            corners(half_size(definition)).map(|local| plan.to_world(local.extend(0.0).xzy()).xz());
        self.built.all().all(|other| {
            if Some(other) == floor {
                return true;
            }
            let theirs = self.content.structure(other.kind);
            let their_corners = corners(half_size(theirs))
                .map(|local| other.to_world(local.extend(0.0).xzy()).xz());
            !own.iter()
                .any(|&corner| other.covers(theirs, corner, margin))
                && !their_corners
                    .iter()
                    .any(|&corner| plan.covers(definition, corner, margin))
                && !plan.covers(definition, other.position.xz(), margin)
        })
    }
}

fn corners(half: Vec2) -> [Vec2; 4] {
    [
        Vec2::new(-half.x, -half.y),
        Vec2::new(half.x, -half.y),
        Vec2::new(-half.x, half.y),
        Vec2::new(half.x, half.y),
    ]
}

/// Points spread over a footprint of half-size `half`, in its own frame:
/// its middle first, then a grid reaching its edges.
fn footprint_samples(half: Vec2) -> impl Iterator<Item = Vec2> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "structures are a few meters across"
    )]
    let steps = |extent: f32| ((2.0 * extent / SAMPLE_SPACING).ceil() as u32).max(1);
    let (across, along) = (steps(half.x), steps(half.y));
    let grid = (0..=along).flat_map(move |z| {
        (0..=across).map(move |x| {
            #[expect(clippy::cast_precision_loss, reason = "a few samples")]
            let t = Vec2::new(x as f32 / across as f32, z as f32 / along as f32);
            -half + 2.0 * half * t
        })
    });
    std::iter::once(Vec2::ZERO).chain(grid)
}
