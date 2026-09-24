//! Things characters cannot walk through: tree trunks, rocks, stalls, and
//! the walls and furniture of what players build.
//!
//! Round things are upright cylinders, built things upright boxes. The
//! server and every client build the same set from the same replicated
//! props, stalls and structures, so movement predicted by a client collides
//! exactly as the server's does. A prop that leaves nothing standing once
//! gathered stops being an obstacle then.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    content::Content,
    protocol::{Gathered, Prop, Shopfront, Structure},
};

/// Size of the cells obstacles are indexed by, in meters.
const CELL: f32 = 4.0;
/// How far below and above its base an obstacle reaches.
const REACH_DOWN: f32 = 0.5;
const REACH_UP: f32 = 3.0;
/// A stall's counter, as cylinders along its front: their offsets across the
/// counter and their radius.
const COUNTER_POSTS: [f32; 3] = [-0.8, 0.0, 0.8];
const COUNTER_RADIUS: f32 = 0.45;

pub(crate) struct ObstaclesPlugin;

impl Plugin for ObstaclesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Obstacles>()
            .add_observer(block_with_prop)
            .add_observer(clear_prop)
            .add_observer(clear_gathered)
            .add_observer(block_with_regrown)
            .add_observer(block_with_stall)
            .add_observer(block_with_structure)
            .add_observer(clear_structure);
    }
}

/// Something upright nothing walks through.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Obstacle {
    center: Vec2,
    shape: Shape,
    bottom: f32,
    top: f32,
    /// The entity it belongs to, so it can go when the entity does.
    owner: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Cylinder {
        radius: f32,
    },
    /// Turned by `turn` around the vertical axis, as a `Heading` is.
    Box {
        half_size: Vec2,
        turn: f32,
    },
}

impl Obstacle {
    /// Distance from the middle to the farthest point of the footprint.
    fn reach(&self) -> f32 {
        match self.shape {
            Shape::Cylinder { radius } => radius,
            Shape::Box { half_size, .. } => half_size.length(),
        }
    }

    /// Where a disc of `radius` centered on `point` ends up once pushed out
    /// of the footprint, if it overlaps it.
    fn push_disc(&self, point: Vec2, radius: f32) -> Option<Vec2> {
        match self.shape {
            Shape::Cylinder { radius: own } => {
                let offset = point - self.center;
                let clearance = own + radius;
                // Straight out from the middle; a disc exactly on the axis
                // is pushed along x.
                (offset.length_squared() < clearance * clearance)
                    .then(|| self.center + offset.try_normalize().unwrap_or(Vec2::X) * clearance)
            }
            Shape::Box { half_size, turn } => {
                let local = Vec2::from_angle(turn).rotate(point - self.center);
                let nearest = local.clamp(-half_size, half_size);
                let offset = local - nearest;
                let pushed = if offset == Vec2::ZERO {
                    // Inside: out through the nearest side.
                    let room = half_size - local.abs();
                    if room.x < room.y {
                        Vec2::new(local.x.signum() * (half_size.x + radius), local.y)
                    } else {
                        Vec2::new(local.x, local.y.signum() * (half_size.y + radius))
                    }
                } else if offset.length_squared() < radius * radius {
                    nearest + offset.normalize() * radius
                } else {
                    return None;
                };
                Some(self.center + Vec2::from_angle(-turn).rotate(pushed))
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct Obstacles {
    cells: HashMap<IVec2, Vec<Obstacle>>,
    largest_reach: f32,
}

impl Obstacles {
    /// Where a body of `radius` and `height` standing at `feet` ends up once
    /// pushed out of every obstacle it overlaps.
    pub fn push_out(&self, feet: Vec3, radius: f32, height: f32) -> Vec3 {
        let mut position = feet;
        let reach = Vec2::splat(radius + self.largest_reach);
        let (min, max) = (cell_of(feet.xz() - reach), cell_of(feet.xz() + reach));
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                for obstacle in self.cells.get(&IVec2::new(x, z)).into_iter().flatten() {
                    let overlaps_height =
                        position.y < obstacle.top && position.y + height > obstacle.bottom;
                    if !overlaps_height {
                        continue;
                    }
                    if let Some(out) = obstacle.push_disc(position.xz(), radius) {
                        position.x = out.x;
                        position.z = out.y;
                    }
                }
            }
        }
        position
    }

    /// The first obstacle a ray from `origin` along the unit vector
    /// `direction` meets within `max_distance`: the entity it belongs to,
    /// and the distance to it.
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(Entity, f32)> {
        let end = origin + direction * max_distance;
        let reach = Vec2::splat(self.largest_reach);
        let (min, max) = (
            cell_of(origin.xz().min(end.xz()) - reach),
            cell_of(origin.xz().max(end.xz()) + reach),
        );
        let mut nearest: Option<(Entity, f32)> = None;
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                for obstacle in self.cells.get(&IVec2::new(x, z)).into_iter().flatten() {
                    let Some(distance) = obstacle.hit_by(origin, direction) else {
                        continue;
                    };
                    if distance <= max_distance
                        && nearest.is_none_or(|(_, nearest)| distance < nearest)
                    {
                        nearest = Some((obstacle.owner, distance));
                    }
                }
            }
        }
        nearest
    }

    fn insert(&mut self, obstacle: Obstacle) {
        self.largest_reach = self.largest_reach.max(obstacle.reach());
        self.cells
            .entry(cell_of(obstacle.center))
            .or_default()
            .push(obstacle);
    }

    fn remove_owned_by(&mut self, owner: Entity) {
        for obstacles in self.cells.values_mut() {
            obstacles.retain(|obstacle| obstacle.owner != owner);
        }
    }
}

impl Obstacle {
    /// Distance along a ray to where it enters the obstacle, or its start
    /// if it starts inside. Rays may enter a box through its top, but only
    /// the side of a cylinder, which stands taller than what it stands for.
    fn hit_by(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        match self.shape {
            Shape::Cylinder { radius } => self.hit_cylinder(radius, origin, direction),
            Shape::Box { half_size, turn } => self.hit_box(half_size, turn, origin, direction),
        }
    }

    fn hit_box(&self, half_size: Vec2, turn: f32, origin: Vec3, direction: Vec3) -> Option<f32> {
        let rotate = |v: Vec2| Vec2::from_angle(turn).rotate(v);
        let start = rotate(origin.xz() - self.center);
        let across = rotate(direction.xz());
        let middle = f32::midpoint(self.bottom, self.top);
        let (start, heading, half) = (
            Vec3::new(start.x, origin.y - middle, start.y),
            Vec3::new(across.x, direction.y, across.y),
            Vec3::new(half_size.x, (self.top - self.bottom) / 2.0, half_size.y),
        );
        // The slabs between each pair of opposite faces.
        let (mut enter, mut leave) = (0.0_f32, f32::INFINITY);
        for axis in 0..3 {
            if heading[axis].abs() < f32::EPSILON {
                if start[axis].abs() > half[axis] {
                    return None;
                }
                continue;
            }
            let (a, b) = (
                (-half[axis] - start[axis]) / heading[axis],
                (half[axis] - start[axis]) / heading[axis],
            );
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
        (enter <= leave).then_some(enter)
    }

    fn hit_cylinder(&self, radius: f32, origin: Vec3, direction: Vec3) -> Option<f32> {
        let offset = origin.xz() - self.center;
        let across = direction.xz();
        let (a, b, c) = (
            across.length_squared(),
            offset.dot(across),
            offset.length_squared() - radius * radius,
        );
        let distance = if c <= 0.0 {
            0.0
        } else {
            // Nearest root of |offset + across * t| = radius.
            let discriminant = b * b - a * c;
            if a <= f32::EPSILON || b >= 0.0 || discriminant < 0.0 {
                return None;
            }
            (-b - discriminant.sqrt()) / a
        };
        let height = origin.y + direction.y * distance;
        (self.bottom..=self.top)
            .contains(&height)
            .then_some(distance)
    }
}

#[cfg(test)]
impl Obstacles {
    /// A single trunk of `radius`, 3 m tall, on ground at height zero.
    pub(crate) fn one_trunk(center: Vec2, radius: f32) -> Self {
        let mut obstacles = Self::default();
        obstacles.insert(Obstacle {
            center,
            shape: Shape::Cylinder { radius },
            bottom: 0.0,
            top: 3.0,
            owner: Entity::PLACEHOLDER,
        });
        obstacles
    }
}

fn cell_of(point: Vec2) -> IVec2 {
    (point / CELL).floor().as_ivec2()
}

fn block_with_prop(
    trigger: On<Add, Prop>,
    props: Query<(&Prop, Has<Gathered>)>,
    content: Res<Content>,
    mut obstacles: ResMut<Obstacles>,
) {
    if let Ok((prop, gathered)) = props.get(trigger.entity) {
        block_with(&mut obstacles, &content, trigger.entity, prop, gathered);
    }
}

fn clear_prop(trigger: On<Remove, Prop>, mut obstacles: ResMut<Obstacles>) {
    obstacles.remove_owned_by(trigger.entity);
}

fn clear_gathered(
    trigger: On<Add, Gathered>,
    props: Query<&Prop>,
    content: Res<Content>,
    mut obstacles: ResMut<Obstacles>,
) {
    if let Ok(prop) = props.get(trigger.entity)
        && !content.prop(prop.kind).stands_when_gathered()
    {
        obstacles.remove_owned_by(trigger.entity);
    }
}

fn block_with_regrown(
    trigger: On<Remove, Gathered>,
    props: Query<&Prop>,
    content: Res<Content>,
    mut obstacles: ResMut<Obstacles>,
) {
    if let Ok(prop) = props.get(trigger.entity)
        && !content.prop(prop.kind).stands_when_gathered()
    {
        block_with(&mut obstacles, &content, trigger.entity, prop, false);
    }
}

/// Makes `prop` an obstacle, if it stands in the way.
fn block_with(
    obstacles: &mut Obstacles,
    content: &Content,
    owner: Entity,
    prop: &Prop,
    gathered: bool,
) {
    let definition = content.prop(prop.kind);
    let standing = !gathered || definition.stands_when_gathered();
    if definition.blocks && standing {
        obstacles.insert(Obstacle {
            center: prop.position.xz(),
            shape: Shape::Cylinder {
                radius: definition.radius * prop.scale,
            },
            bottom: prop.position.y - REACH_DOWN,
            top: prop.position.y + REACH_UP,
            owner,
        });
    }
}

fn block_with_stall(
    trigger: On<Add, Shopfront>,
    stalls: Query<&Shopfront>,
    mut obstacles: ResMut<Obstacles>,
) {
    let Ok(stall) = stalls.get(trigger.entity) else {
        return;
    };
    let across = Quat::from_rotation_y(stall.facing) * Vec3::X;
    for offset in COUNTER_POSTS {
        let center = stall.position + across * offset;
        obstacles.insert(Obstacle {
            center: center.xz(),
            shape: Shape::Cylinder {
                radius: COUNTER_RADIUS,
            },
            bottom: stall.position.y - REACH_DOWN,
            top: stall.position.y + REACH_UP,
            owner: trigger.entity,
        });
    }
}

fn block_with_structure(
    trigger: On<Add, Structure>,
    structures: Query<&Structure>,
    content: Res<Content>,
    mut obstacles: ResMut<Obstacles>,
) {
    let Ok(structure) = structures.get(trigger.entity) else {
        return;
    };
    for solid in &content.structure(structure.kind).solids {
        let center = structure.to_world(Vec3::new(solid.at.0, 0.0, solid.at.1));
        obstacles.insert(Obstacle {
            center: center.xz(),
            shape: Shape::Box {
                half_size: Vec2::new(solid.size.0, solid.size.1) / 2.0,
                turn: structure.facing,
            },
            bottom: structure.position.y,
            top: structure.position.y + solid.height,
            owner: trigger.entity,
        });
    }
}

fn clear_structure(trigger: On<Remove, Structure>, mut obstacles: ResMut<Obstacles>) {
    obstacles.remove_owned_by(trigger.entity);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trunk_at(center: Vec2) -> Obstacles {
        Obstacles::one_trunk(center, 0.5)
    }

    #[test]
    fn bodies_are_pushed_out_of_obstacles() {
        let obstacles = trunk_at(Vec2::new(10.0, 10.0));
        let pushed = obstacles.push_out(Vec3::new(10.4, 0.0, 10.0), 0.3, 1.8);
        assert!((pushed.x - 10.8).abs() < 1e-5, "pushed to {pushed}");
        assert!((pushed.z - 10.0).abs() < 1e-5);
    }

    #[test]
    fn rays_find_the_nearest_obstacle_in_front() {
        let mut obstacles = trunk_at(Vec2::new(10.0, 0.0));
        obstacles.insert(Obstacle {
            center: Vec2::new(5.0, 0.0),
            shape: Shape::Cylinder { radius: 0.5 },
            bottom: 0.0,
            top: 3.0,
            owner: Entity::from_raw_u32(7).expect("a valid index"),
        });
        let eyes = Vec3::new(0.0, 1.6, 0.0);
        let (owner, distance) = obstacles
            .raycast(eyes, Vec3::X, 20.0)
            .expect("the ray meets a trunk");
        assert_eq!(owner, Entity::from_raw_u32(7).expect("a valid index"));
        assert!((distance - 4.5).abs() < 1e-4, "hit at {distance}");

        assert_eq!(obstacles.raycast(eyes, Vec3::NEG_X, 20.0), None, "behind");
        assert_eq!(obstacles.raycast(eyes, Vec3::X, 4.0), None, "too far");
        let upward = Vec3::new(1.0, 1.0, 0.0).normalize();
        assert_eq!(obstacles.raycast(eyes, upward, 20.0), None, "over the top");
    }

    /// A box 2 m by 1 m, 1 m tall, turned a quarter, owned by entity 9.
    fn table_at(center: Vec2) -> Obstacles {
        let mut obstacles = Obstacles::default();
        obstacles.insert(Obstacle {
            center,
            shape: Shape::Box {
                half_size: Vec2::new(1.0, 0.5),
                turn: std::f32::consts::FRAC_PI_2,
            },
            bottom: 0.0,
            top: 1.0,
            owner: Entity::from_raw_u32(9).expect("a valid index"),
        });
        obstacles
    }

    #[test]
    fn bodies_are_pushed_out_of_turned_boxes() {
        // Turned a quarter, the box is 1 m across x and 2 m along z.
        let obstacles = table_at(Vec2::ZERO);
        let pushed = obstacles.push_out(Vec3::new(0.6, 0.0, 0.8), 0.3, 1.8);
        assert!((pushed.x - 0.8).abs() < 1e-4, "pushed to {pushed}");
        assert!((pushed.z - 0.8).abs() < 1e-4);
        let beside = Vec3::new(0.9, 0.0, 0.0);
        assert_eq!(obstacles.push_out(beside, 0.3, 1.8), beside);
        let inside = obstacles.push_out(Vec3::new(0.1, 0.0, 0.0), 0.3, 1.8);
        assert!((inside.x - 0.8).abs() < 1e-4, "pushed to {inside}");
    }

    #[test]
    fn rays_find_boxes_through_their_top() {
        let obstacles = table_at(Vec2::new(3.0, 0.0));
        let eyes = Vec3::new(0.0, 1.6, 0.0);
        let down = (Vec3::new(3.0, 1.0, 0.0) - eyes).normalize();
        let (owner, distance) = obstacles
            .raycast(eyes, down, 5.0)
            .expect("the ray meets the box");
        assert_eq!(owner, Entity::from_raw_u32(9).expect("a valid index"));
        let hit = eyes + down * distance;
        assert!((hit.y - 1.0).abs() < 1e-3, "hit the top at {hit}");
        assert_eq!(
            obstacles
                .raycast(eyes, Vec3::X, 5.0)
                .map(|(owner, _)| owner),
            None,
            "over it"
        );
    }

    #[test]
    fn bodies_clear_of_obstacles_stay_put() {
        let obstacles = trunk_at(Vec2::new(10.0, 10.0));
        let beside = Vec3::new(11.0, 0.0, 10.0);
        assert_eq!(obstacles.push_out(beside, 0.3, 1.8), beside);
        let above = Vec3::new(10.2, 3.5, 10.0);
        assert_eq!(obstacles.push_out(above, 0.3, 1.8), above);
    }
}
