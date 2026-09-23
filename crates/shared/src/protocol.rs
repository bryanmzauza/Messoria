//! Everything that crosses the wire: replicated components, player input,
//! terrain and player actions.

use bevy::{ecs::entity::MapEntities, prelude::*};
use lightyear::prelude::{input::native::InputPlugin, *};
use messoria_calendar::WorldTime;
use messoria_voxel::{ChunkChanges, ChunkPos};
use serde::{Deserialize, Serialize};

use crate::energy::Energy;

/// The peer an entity belongs to.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub PeerId);

/// Position of a character's feet in world space, in meters.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Position(pub Vec3);

/// Velocity in meters per second.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Velocity(pub Vec3);

/// Facing direction around the vertical axis, in radians. Zero faces -Z.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Heading(pub f32);

/// Controls sampled by a client once per simulation tick.
///
/// The server treats every field as untrusted; see `movement::advance`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerInput {
    /// Desired movement relative to `yaw`: `x` strafes right, `y` moves forward.
    pub movement: Vec2,
    /// Camera heading the movement is relative to, in radians.
    pub yaw: f32,
    pub jump: bool,
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// Marks a character that is asleep. Sleeping characters do not move.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asleep;

/// The world's current time, on the single clock entity the server replicates.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldClock(pub WorldTime);

/// How many players are asleep, and how many must be for the day to end.
/// Kept on the clock entity.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SleepTally {
    pub asleep: u32,
    pub required: u32,
}

/// A client asking for its character to go to sleep or wake up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepRequest {
    Sleep,
    Wake,
}

/// Terrain streamed from the server to a client.
///
/// A single message type keeps every update on one ordered channel, so a
/// chunk always arrives before the changes made to it afterwards.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TerrainUpdate {
    /// A chunk came into range, encoded with `Chunk::encode`.
    Loaded { chunk: ChunkPos, data: Vec<u8> },
    /// Voxels of a loaded chunk changed.
    Changed(ChunkChanges),
    /// A chunk went out of range and can be forgotten.
    Unloaded(ChunkPos),
}

/// A client asking to reshape the terrain at `target`.
///
/// The server validates reach, rate and target before applying it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ShovelRequest {
    pub target: Vec3,
    pub action: ShovelAction,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShovelAction {
    Dig,
    Raise,
}

/// Carries [`TerrainUpdate`]s, in order and without loss.
pub struct TerrainChannel;

/// Carries player actions such as [`ShovelRequest`]s and [`SleepRequest`]s.
pub struct ActionChannel;

impl Ease for Position {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| Position(start.0.lerp(end.0, t)))
    }
}

impl Diffable<Vec3> for Position {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> Vec3 {
        new.0 - self.0
    }

    fn apply_diff(&mut self, delta: &Vec3) {
        self.0 += *delta;
    }
}

impl Ease for Heading {
    /// Turns through the shorter arc, so a heading crossing ±π does not spin
    /// all the way around.
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        let delta = (end.0 - start.0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        FunctionCurve::new(Interval::UNIT, move |t| Heading(start.0 + delta * t))
    }
}

/// Registers the protocol. Must be added after lightyear's client and server
/// plugin groups, which `SharedPlugin` guarantees.
pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputPlugin::<PlayerInput>::default());

        app.add_channel::<TerrainChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
        app.add_channel::<ActionChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<TerrainUpdate>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ShovelRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SleepRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<WorldClock>().replicate();
        app.component::<SleepTally>().replicate();
        app.component::<Energy>().replicate();
        app.component::<Asleep>().replicate();

        app.component::<PlayerId>().replicate();

        app.component::<Position>()
            .replicate()
            .add_linear_interpolation()
            .predict()
            .add_linear_correction::<Vec3>();

        app.component::<Velocity>().replicate().predict();

        app.component::<Heading>()
            .replicate()
            .add_linear_interpolation()
            .predict();
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    #[test]
    fn heading_interpolates_through_the_shorter_arc() {
        let curve = Heading::interpolating_curve_unbounded(Heading(PI - 0.1), Heading(-PI + 0.1));
        let midpoint = curve.sample_unchecked(0.5).0;

        assert!(
            (midpoint.abs() - PI).abs() < 1e-5,
            "midpoint was {midpoint}"
        );
    }
}
