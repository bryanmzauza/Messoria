//! Character movement.
//!
//! The server runs this for every player it owns, and each client runs it for
//! the character it predicts. Both must produce bit-identical results from the
//! same input, so it depends only on its arguments and the fixed tick length.

use bevy::prelude::*;
use lightyear::prelude::{client::Remote, input::native::ActionState, *};

use crate::{
    protocol::{Heading, PlayerInput, Position, Velocity},
    tick,
};

/// Horizontal speed at full input, in meters per second.
pub const WALK_SPEED: f32 = 4.5;
/// Vertical speed at the start of a jump, in meters per second.
pub const JUMP_SPEED: f32 = 6.0;
/// Downward acceleration, in meters per second squared.
pub const GRAVITY: f32 = 20.0;
/// Height of the walkable ground until voxel terrain provides collision.
pub const GROUND_HEIGHT: f32 = 0.0;

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
/// treated as no input.
pub fn advance(motion: Motion, input: &PlayerInput, dt: f32) -> Motion {
    let wish = if input.movement.is_finite() {
        input.movement.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    let yaw = sanitize_yaw(input.yaw).unwrap_or(0.0);
    let horizontal = Quat::from_rotation_y(yaw) * Vec3::new(wish.x, 0.0, -wish.y) * WALK_SPEED;

    let grounded = motion.position.y <= GROUND_HEIGHT;
    let (start_vertical, end_vertical) = match (grounded, input.jump) {
        (true, false) => (0.0, 0.0),
        (true, true) => (JUMP_SPEED, JUMP_SPEED - GRAVITY * dt),
        (false, _) => (motion.velocity.y, motion.velocity.y - GRAVITY * dt),
    };
    // Averaging the vertical speed over the step integrates constant gravity
    // exactly, so jump height does not depend on the tick rate.
    let displacement = Vec3::new(
        horizontal.x,
        f32::midpoint(start_vertical, end_vertical),
        horizontal.z,
    ) * dt;

    let mut velocity = Vec3::new(horizontal.x, end_vertical, horizontal.z);
    let mut position = motion.position + displacement;
    if position.y < GROUND_HEIGHT {
        position.y = GROUND_HEIGHT;
        velocity.y = 0.0;
    }

    Motion { position, velocity }
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
/// Remote characters that are only interpolated are left alone.
fn move_characters(
    mut characters: Query<
        (
            &mut Position,
            &mut Velocity,
            &mut Heading,
            &ActionState<PlayerInput>,
        ),
        Or<(With<Predicted>, Without<Remote>)>,
    >,
) {
    let dt = tick::tick_duration().as_secs_f32();
    for (mut position, mut velocity, mut heading, input) in &mut characters {
        let motion = advance(
            Motion {
                position: position.0,
                velocity: velocity.0,
            },
            input,
            dt,
        );
        position.set_if_neq(Position(motion.position));
        velocity.set_if_neq(Velocity(motion.velocity));
        if let Some(yaw) = sanitize_yaw(input.yaw) {
            heading.set_if_neq(Heading(yaw));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    const DT: f32 = 1.0 / 30.0;

    fn walk(movement: Vec2, yaw: f32) -> PlayerInput {
        PlayerInput {
            movement,
            yaw,
            jump: false,
        }
    }

    #[test]
    fn idle_character_stays_put() {
        let motion = advance(Motion::default(), &PlayerInput::default(), DT);
        assert_eq!(motion, Motion::default());
    }

    #[test]
    fn forward_follows_yaw() {
        let north = advance(Motion::default(), &walk(Vec2::Y, 0.0), DT);
        assert!(
            north
                .position
                .abs_diff_eq(Vec3::NEG_Z * WALK_SPEED * DT, 1e-6)
        );

        let west = advance(Motion::default(), &walk(Vec2::Y, FRAC_PI_2), DT);
        assert!(
            west.position
                .abs_diff_eq(Vec3::NEG_X * WALK_SPEED * DT, 1e-6)
        );
    }

    #[test]
    fn diagonal_input_is_not_faster() {
        let motion = advance(Motion::default(), &walk(Vec2::ONE, 0.0), DT);
        assert!((motion.velocity.length() - WALK_SPEED).abs() < 1e-4);
    }

    #[test]
    fn oversized_input_is_clamped() {
        let motion = advance(Motion::default(), &walk(Vec2::Y * 100.0, 0.0), DT);
        assert!((motion.velocity.length() - WALK_SPEED).abs() < 1e-4);
    }

    #[test]
    fn non_finite_input_is_ignored() {
        let input = PlayerInput {
            movement: Vec2::new(f32::NAN, 1.0),
            yaw: f32::INFINITY,
            jump: false,
        };
        assert_eq!(advance(Motion::default(), &input, DT), Motion::default());
    }

    #[test]
    fn jump_rises_and_lands() {
        let jump = PlayerInput {
            jump: true,
            ..default()
        };
        let mut motion = advance(Motion::default(), &jump, DT);
        let mut apex = motion.position.y;
        let mut ticks = 1;
        while motion.position.y > GROUND_HEIGHT {
            motion = advance(motion, &PlayerInput::default(), DT);
            apex = apex.max(motion.position.y);
            ticks += 1;
            assert!(ticks < 300, "character never landed");
        }

        let ideal_apex = JUMP_SPEED * JUMP_SPEED / (2.0 * GRAVITY);
        assert!((apex - ideal_apex).abs() < 0.01, "apex was {apex}");
        assert!(
            motion.velocity.y.abs() < f32::EPSILON,
            "still falling after landing"
        );
    }
}
