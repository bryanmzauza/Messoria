//! Validating and applying players' shovel requests.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_shared::{
    movement::{BODY_HEIGHT, BODY_RADIUS, EYE_HEIGHT},
    protocol::{PlayerId, Position, ShovelAction, ShovelRequest},
    shovel,
    terrain::{ChunkChanged, Terrain},
};

use super::{TerrainEdited, editable};
use crate::players::ControlledCharacter;

/// Clients pace their requests at `shovel::COOLDOWN`; network jitter can
/// bunch them up in transit, so the server enforces a slightly shorter gap.
const MIN_INTERVAL: Duration = shovel::COOLDOWN.saturating_sub(Duration::from_millis(50));
/// How far from the surface a target may be. Requests for points deep in the
/// air or underground did not come from aiming at the terrain.
const MAX_SURFACE_DISTANCE: f32 = 1.0;

pub(super) struct ShovelPlugin;

impl Plugin for ShovelPlugin {
    fn build(&self, app: &mut App) {
        // lightyear drops messages left unread at the end of a frame, and not
        // every frame runs a fixed tick, so requests are handled as they arrive.
        app.add_systems(
            PreUpdate,
            apply_shovel_requests.after(MessageSystems::Receive),
        );
    }
}

/// When the player behind a connection last used the shovel.
#[derive(Component)]
struct LastShovelUse(Duration);

fn apply_shovel_requests(
    time: Res<Time>,
    mut clients: Query<(
        Entity,
        &mut MessageReceiver<ShovelRequest>,
        &ControlledCharacter,
        Option<&LastShovelUse>,
    )>,
    characters: Query<&Position, With<PlayerId>>,
    mut terrain: ResMut<Terrain>,
    mut edited: MessageWriter<TerrainEdited>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut commands: Commands,
) {
    let now = time.elapsed();
    for (client, mut requests, character, last_use) in &mut clients {
        let previous_use = last_use.map(|last| last.0);
        let mut last_use = previous_use;
        for request in requests.receive() {
            let Ok(feet) = characters.get(character.0) else {
                continue;
            };
            if last_use.is_some_and(|last| now.saturating_sub(last) < MIN_INTERVAL) {
                continue;
            }
            if let Err(reason) = validate(&request, feet.0, &terrain, &characters) {
                debug!("rejected shovel request {request:?}: {reason}");
                continue;
            }

            for changes in terrain.apply_brush(&shovel::brush(&request)) {
                chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
                edited.write(TerrainEdited(changes));
            }
            last_use = Some(now);
        }
        if let Some(last_use) = last_use.filter(|_| last_use != previous_use) {
            commands.entity(client).insert(LastShovelUse(last_use));
        }
    }
}

fn validate(
    request: &ShovelRequest,
    feet: Vec3,
    terrain: &Terrain,
    characters: &Query<&Position, With<PlayerId>>,
) -> Result<(), &'static str> {
    let target = request.target;
    if !target.is_finite() || !editable(target) {
        return Err("target outside the editable world");
    }
    if !shovel::in_reach(feet + Vec3::Y * EYE_HEIGHT, target) {
        return Err("target out of reach");
    }
    if !terrain
        .distance(target)
        .is_some_and(|distance| distance.abs() <= MAX_SURFACE_DISTANCE)
    {
        return Err("target is not on the terrain surface");
    }
    if request.action == ShovelAction::Raise
        && characters
            .iter()
            .any(|character| body_overlaps_brush(character.0, target))
    {
        return Err("raising would bury a player");
    }
    Ok(())
}

/// Whether the brush sphere at `center` intersects a body standing at `feet`,
/// treating the body as a capsule around its vertical axis.
fn body_overlaps_brush(feet: Vec3, center: Vec3) -> bool {
    let nearest_on_axis = feet.with_y(center.y.clamp(feet.y, feet.y + BODY_HEIGHT));
    nearest_on_axis.distance(center) < shovel::BRUSH_RADIUS + BODY_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raising_next_to_a_body_overlaps_it() {
        let feet = Vec3::new(0.0, 10.0, 0.0);
        assert!(body_overlaps_brush(feet, Vec3::new(1.0, 10.0, 0.0)));
        assert!(body_overlaps_brush(feet, Vec3::new(0.0, 12.5, 0.0)));
    }

    #[test]
    fn raising_at_arms_length_does_not() {
        let feet = Vec3::new(0.0, 10.0, 0.0);
        assert!(!body_overlaps_brush(feet, Vec3::new(2.5, 10.0, 0.0)));
        assert!(!body_overlaps_brush(feet, Vec3::new(0.0, 8.0, 0.0)));
    }
}
