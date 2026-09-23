//! Things characters cannot walk through: tree trunks, rocks and stalls.
//!
//! Each obstacle is an upright cylinder. The server and every client build
//! the same set from the same replicated props and stalls, so movement
//! predicted by a client collides exactly as the server's does.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    content::Content,
    protocol::{Prop, Shopfront},
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
            .add_observer(block_with_stall);
    }
}

/// An upright cylinder nothing walks through.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Obstacle {
    center: Vec2,
    radius: f32,
    bottom: f32,
    top: f32,
    /// The entity it belongs to, so it can go when the entity does.
    owner: Entity,
}

#[derive(Resource, Default)]
pub struct Obstacles {
    cells: HashMap<IVec2, Vec<Obstacle>>,
    largest_radius: f32,
}

impl Obstacles {
    /// Where a body of `radius` and `height` standing at `feet` ends up once
    /// pushed out of every obstacle it overlaps.
    pub fn push_out(&self, feet: Vec3, radius: f32, height: f32) -> Vec3 {
        let mut position = feet;
        let reach = Vec2::splat(radius + self.largest_radius);
        let (min, max) = (cell_of(feet.xz() - reach), cell_of(feet.xz() + reach));
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                for obstacle in self.cells.get(&IVec2::new(x, z)).into_iter().flatten() {
                    let overlaps_height =
                        position.y < obstacle.top && position.y + height > obstacle.bottom;
                    let offset = position.xz() - obstacle.center;
                    let clearance = obstacle.radius + radius;
                    if overlaps_height && offset.length_squared() < clearance * clearance {
                        // Straight out from the middle; a body exactly on the
                        // axis is pushed along x.
                        let away = offset.try_normalize().unwrap_or(Vec2::X);
                        let out = obstacle.center + away * clearance;
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
        let reach = Vec2::splat(self.largest_radius);
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
        self.largest_radius = self.largest_radius.max(obstacle.radius);
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
    /// Distance along a ray to where it enters this cylinder's side, or its
    /// start if it starts inside. Rays that pass above or below miss.
    fn hit_by(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let offset = origin.xz() - self.center;
        let across = direction.xz();
        let (a, b, c) = (
            across.length_squared(),
            offset.dot(across),
            offset.length_squared() - self.radius * self.radius,
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
            radius,
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
    props: Query<&Prop>,
    content: Res<Content>,
    mut obstacles: ResMut<Obstacles>,
) {
    let Ok(prop) = props.get(trigger.entity) else {
        return;
    };
    let definition = content.prop(prop.kind);
    if definition.blocks {
        obstacles.insert(Obstacle {
            center: prop.position.xz(),
            radius: definition.radius * prop.scale,
            bottom: prop.position.y - REACH_DOWN,
            top: prop.position.y + REACH_UP,
            owner: trigger.entity,
        });
    }
}

fn clear_prop(trigger: On<Remove, Prop>, mut obstacles: ResMut<Obstacles>) {
    obstacles.remove_owned_by(trigger.entity);
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
            radius: COUNTER_RADIUS,
            bottom: stall.position.y - REACH_DOWN,
            top: stall.position.y + REACH_UP,
            owner: trigger.entity,
        });
    }
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
            radius: 0.5,
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

    #[test]
    fn bodies_clear_of_obstacles_stay_put() {
        let obstacles = trunk_at(Vec2::new(10.0, 10.0));
        let beside = Vec3::new(11.0, 0.0, 10.0);
        assert_eq!(obstacles.push_out(beside, 0.3, 1.8), beside);
        let above = Vec3::new(10.2, 3.5, 10.0);
        assert_eq!(obstacles.push_out(above, 0.3, 1.8), above);
    }
}
