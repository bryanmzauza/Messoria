//! Character movement.
//!
//! The server runs this for every player it owns, and each client runs it for
//! the character it predicts. Both must produce bit-identical results from the
//! same input and terrain, so it depends only on its arguments and the fixed
//! tick length.

use bevy::prelude::*;
use lightyear::prelude::{client::Remote, input::native::ActionState, *};
use messoria_voxel::ChunkMap;

use crate::{
    protocol::{Asleep, Heading, PlayerInput, Position, Velocity},
    terrain::Terrain,
    tick,
};

/// Horizontal speed at full input, in meters per second.
pub const WALK_SPEED: f32 = 4.5;
/// Vertical speed at the start of a jump, in meters per second.
pub const JUMP_SPEED: f32 = 6.0;
/// Downward acceleration, in meters per second squared.
pub const GRAVITY: f32 = 20.0;

/// Radius of the cylinder a character occupies, in meters.
pub const BODY_RADIUS: f32 = 0.3;
/// Height of a character from feet to the top of the head, in meters.
pub const BODY_HEIGHT: f32 = 1.8;
/// Height of the eyes above the feet, in meters.
pub const EYE_HEIGHT: f32 = 1.6;
/// Tallest ledge a character walks up without jumping.
const STEP_HEIGHT: f32 = 0.5;
/// Deepest drop a walking character follows without starting to fall.
const SNAP_DEPTH: f32 = 0.5;
/// How close to the ground feet must be to count as standing on it.
const GROUND_TOLERANCE: f32 = 0.05;

/// The part of a character's state that movement evolves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    pub position: Vec3,
    pub velocity: Vec3,
}

/// Advances `motion` by one step of `dt` seconds under `input`.
///
/// Input comes from clients and is sanitized here rather than trusted: the
/// movement vector is clamped to unit length and non-finite values are
/// treated as no input. A character whose surroundings are not loaded yet
/// stays where it is.
pub fn advance(motion: Motion, input: &PlayerInput, dt: f32, terrain: &ChunkMap) -> Motion {
    if terrain.distance(motion.position).is_none() {
        return Motion {
            position: motion.position,
            velocity: Vec3::ZERO,
        };
    }

    let grounded = motion.velocity.y <= 0.0
        && ground_below(terrain, motion.position, GROUND_TOLERANCE)
            .is_some_and(|ground| motion.position.y - ground <= GROUND_TOLERANCE);

    let mut position = walk(terrain, motion.position, horizontal_velocity(input) * dt);
    let mut velocity = (position - motion.position) / dt;

    if grounded && !input.jump {
        // Follow the ground up small steps and down gentle slopes; past that,
        // the character walked off a ledge and starts falling next step.
        if let Some(ground) = ground_below(terrain, position, SNAP_DEPTH) {
            position.y = ground;
        }
        velocity.y = 0.0;
    } else {
        let start = if grounded {
            JUMP_SPEED
        } else {
            motion.velocity.y
        };
        let end = start - GRAVITY * dt;
        // Averaging the vertical speed over the step integrates constant
        // gravity exactly, so jump height does not depend on the tick rate.
        let target_y = position.y + f32::midpoint(start, end) * dt;
        velocity.y = end;

        if target_y > position.y {
            if body_fits(terrain, position.with_y(target_y)) {
                position.y = target_y;
            } else {
                velocity.y = 0.0;
            }
        } else {
            let fall = position.y - target_y + GROUND_TOLERANCE;
            match ground_below(terrain, position, fall) {
                Some(ground) if ground + GROUND_TOLERANCE >= target_y => {
                    position.y = ground;
                    velocity.y = 0.0;
                }
                _ => position.y = target_y,
            }
        }
    }

    Motion { position, velocity }
}

fn horizontal_velocity(input: &PlayerInput) -> Vec3 {
    let wish = if input.movement.is_finite() {
        input.movement.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    let yaw = sanitize_yaw(input.yaw).unwrap_or(0.0);
    Quat::from_rotation_y(yaw) * Vec3::new(wish.x, 0.0, -wish.y) * WALK_SPEED
}

/// Moves `feet` by `step` if the body fits there, otherwise slides along
/// whichever axis is free.
fn walk(terrain: &ChunkMap, feet: Vec3, step: Vec3) -> Vec3 {
    [step, step.with_z(0.0), step.with_x(0.0)]
        .into_iter()
        .filter(|candidate| *candidate != Vec3::ZERO)
        .map(|candidate| feet + candidate)
        .find(|&candidate| body_fits(terrain, candidate))
        .unwrap_or(feet)
}

/// Whether a body standing at `feet` is clear of the ground, ignoring the part
/// below step height that walking up a ledge is allowed to overlap.
///
/// Probes the axis and the rim of the body's cylinder rather than trusting the
/// stored distance, which is only an estimate away from the surface.
fn body_fits(terrain: &ChunkMap, feet: Vec3) -> bool {
    const RIM: [Vec3; 5] = [
        Vec3::ZERO,
        Vec3::new(BODY_RADIUS, 0.0, 0.0),
        Vec3::new(-BODY_RADIUS, 0.0, 0.0),
        Vec3::new(0.0, 0.0, BODY_RADIUS),
        Vec3::new(0.0, 0.0, -BODY_RADIUS),
    ];
    [
        STEP_HEIGHT,
        f32::midpoint(STEP_HEIGHT, BODY_HEIGHT),
        BODY_HEIGHT,
    ]
    .into_iter()
    .flat_map(|height| RIM.map(|rim| feet + rim + Vec3::Y * height))
    .all(|probe| {
        terrain
            .distance(probe)
            .is_some_and(|distance| distance > 0.0)
    })
}

/// Height of the ground under `feet`, looking from step height above them down
/// to `depth` below them.
fn ground_below(terrain: &ChunkMap, feet: Vec3, depth: f32) -> Option<f32> {
    terrain.surface_below(feet + Vec3::Y * STEP_HEIGHT, STEP_HEIGHT + depth)
}

/// Returns `yaw` wrapped to `[-π, π)`, or `None` if it is not finite.
fn sanitize_yaw(yaw: f32) -> Option<f32> {
    use std::f32::consts::{PI, TAU};
    yaw.is_finite().then(|| (yaw + PI).rem_euclid(TAU) - PI)
}

pub(crate) struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, move_characters);
    }
}

/// Moves every character this app simulates: all local characters when
/// acting as a server, plus the one it predicts when acting as a client.
/// Remote characters that are only interpolated are left alone. Sleeping
/// characters ignore their input but still fall.
fn move_characters(
    terrain: Res<Terrain>,
    mut characters: Query<
        (
            &mut Position,
            &mut Velocity,
            &mut Heading,
            &ActionState<PlayerInput>,
            Has<Asleep>,
        ),
        Or<(With<Predicted>, Without<Remote>)>,
    >,
) {
    let dt = tick::tick_duration().as_secs_f32();
    for (mut position, mut velocity, mut heading, input, asleep) in &mut characters {
        let input = if asleep {
            &PlayerInput::default()
        } else {
            &input.0
        };
        let motion = advance(
            Motion {
                position: position.0,
                velocity: velocity.0,
            },
            input,
            dt,
            &terrain,
        );
        position.set_if_neq(Position(motion.position));
        velocity.set_if_neq(Velocity(motion.velocity));
        if !asleep && let Some(yaw) = sanitize_yaw(input.yaw) {
            heading.set_if_neq(Heading(yaw));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use messoria_voxel::{Chunk, ChunkPos, Material, Voxel};

    use super::*;

    const DT: f32 = 1.0 / 30.0;

    /// Terrain around the origin whose ground height at `(x, z)` is `height(x, z)`.
    fn terrain(height: impl Fn(f32, f32) -> f32) -> ChunkMap {
        let mut map = ChunkMap::default();
        for z in -1..=0 {
            for y in -1..=0 {
                for x in -1..=0 {
                    let chunk = ChunkPos(IVec3::new(x, y, z));
                    let origin = chunk.origin();
                    map.insert(
                        chunk,
                        Chunk::from_fn(|local| {
                            let voxel = (origin + local).as_vec3();
                            Voxel::new(voxel.y - height(voxel.x, voxel.z), Material::Grass)
                        }),
                    );
                }
            }
        }
        map
    }

    fn flat() -> ChunkMap {
        terrain(|_, _| 0.0)
    }

    fn walk_forward(yaw: f32) -> PlayerInput {
        PlayerInput {
            movement: Vec2::Y,
            yaw,
            jump: false,
        }
    }

    fn standing_at(position: Vec3) -> Motion {
        Motion {
            position,
            velocity: Vec3::ZERO,
        }
    }

    fn run(mut motion: Motion, input: &PlayerInput, ticks: u32, terrain: &ChunkMap) -> Motion {
        for _ in 0..ticks {
            motion = advance(motion, input, DT, terrain);
        }
        motion
    }

    #[test]
    fn idle_character_stays_put() {
        let start = standing_at(Vec3::new(-5.0, 0.0, -5.0));
        let motion = run(start, &PlayerInput::default(), 30, &flat());
        assert!(
            motion.position.abs_diff_eq(start.position, 1e-3),
            "drifted to {}",
            motion.position
        );
        assert_eq!(motion.velocity, Vec3::ZERO);
    }

    #[test]
    fn forward_follows_yaw() {
        let start = Vec3::new(-10.0, 0.0, -10.0);
        let north = advance(standing_at(start), &walk_forward(0.0), DT, &flat());
        assert!(
            north
                .position
                .abs_diff_eq(start + Vec3::NEG_Z * WALK_SPEED * DT, 1e-4)
        );

        let west = advance(standing_at(start), &walk_forward(FRAC_PI_2), DT, &flat());
        assert!(
            west.position
                .abs_diff_eq(start + Vec3::NEG_X * WALK_SPEED * DT, 1e-4)
        );
    }

    #[test]
    fn diagonal_input_is_not_faster() {
        let input = PlayerInput {
            movement: Vec2::ONE,
            ..default()
        };
        let motion = advance(
            standing_at(Vec3::new(-10.0, 0.0, -10.0)),
            &input,
            DT,
            &flat(),
        );
        assert!((motion.velocity.length() - WALK_SPEED).abs() < 1e-3);
    }

    #[test]
    fn oversized_input_is_clamped() {
        let input = PlayerInput {
            movement: Vec2::Y * 100.0,
            ..default()
        };
        let motion = advance(
            standing_at(Vec3::new(-10.0, 0.0, -10.0)),
            &input,
            DT,
            &flat(),
        );
        assert!((motion.velocity.length() - WALK_SPEED).abs() < 1e-3);
    }

    #[test]
    fn non_finite_input_is_ignored() {
        let start = standing_at(Vec3::new(-10.0, 0.0, -10.0));
        let input = PlayerInput {
            movement: Vec2::new(f32::NAN, 1.0),
            yaw: f32::INFINITY,
            jump: false,
        };
        assert_eq!(advance(start, &input, DT, &flat()), start);
    }

    #[test]
    fn jump_rises_and_lands() {
        let jump = PlayerInput {
            jump: true,
            ..default()
        };
        let terrain = flat();
        let mut motion = advance(
            standing_at(Vec3::new(-10.0, 0.0, -10.0)),
            &jump,
            DT,
            &terrain,
        );
        let mut apex = motion.position.y;
        let mut ticks = 1;
        while motion.position.y > 1e-3 {
            motion = advance(motion, &PlayerInput::default(), DT, &terrain);
            apex = apex.max(motion.position.y);
            ticks += 1;
            assert!(ticks < 300, "character never landed");
        }

        let ideal_apex = JUMP_SPEED * JUMP_SPEED / (2.0 * GRAVITY);
        assert!((apex - ideal_apex).abs() < 0.01, "apex was {apex}");
        assert!(
            motion.velocity.y.abs() < f32::EPSILON,
            "still falling after landing: {motion:?}"
        );
    }

    #[test]
    fn walls_block_movement() {
        let terrain = terrain(|x, _| if x > -10.0 { 10.0 } else { 0.0 });
        let east = walk_forward(-FRAC_PI_2);
        let motion = run(
            standing_at(Vec3::new(-14.0, 0.0, -10.0)),
            &east,
            60,
            &terrain,
        );

        assert!(
            motion.position.x < -10.0 - BODY_RADIUS + 0.1,
            "walked into the wall to {}",
            motion.position
        );
        assert!(
            motion.position.x > -11.0,
            "stopped short at {}",
            motion.position
        );
    }

    #[test]
    fn low_ledges_are_climbed() {
        let terrain = terrain(|x, _| if x > -10.0 { 0.4 } else { 0.0 });
        let east = walk_forward(-FRAC_PI_2);
        let motion = run(
            standing_at(Vec3::new(-14.0, 0.0, -10.0)),
            &east,
            60,
            &terrain,
        );

        assert!(motion.position.x > -8.0, "stuck at {}", motion.position);
        assert!(
            (motion.position.y - 0.4).abs() < 0.1,
            "height is {}",
            motion.position.y
        );
    }

    #[test]
    fn walking_off_a_cliff_falls_to_the_ground_below() {
        let terrain = terrain(|x, _| if x < -10.0 { 3.0 } else { 0.0 });
        let east = walk_forward(-FRAC_PI_2);
        let motion = run(
            standing_at(Vec3::new(-12.0, 3.0, -10.0)),
            &east,
            60,
            &terrain,
        );

        assert!(motion.position.x > -8.0, "stuck at {}", motion.position);
        assert!(
            motion.position.y.abs() < 0.05,
            "height is {}",
            motion.position.y
        );
    }

    #[test]
    fn characters_wait_where_terrain_is_not_loaded() {
        let start = Motion {
            position: Vec3::new(100.0, 5.0, 100.0),
            velocity: Vec3::new(0.0, -3.0, 0.0),
        };
        let motion = advance(start, &walk_forward(0.0), DT, &flat());
        assert_eq!(motion.position, start.position);
        assert_eq!(motion.velocity, Vec3::ZERO);
    }
}
