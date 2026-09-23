//! Carrying out shovel uses: validating them, reshaping the terrain, and
//! moving ground between the terrain and the character's inventory.

use std::{collections::HashSet, time::Duration};

use bevy::prelude::*;
use messoria_shared::{
    content::Content,
    energy::Energy,
    movement::{BODY_HEIGHT, BODY_RADIUS, EYE_HEIGHT},
    protocol::{Asleep, Belongings, PlayerId, Position, WorldClock},
    shovel::{self, ShovelAction},
    terrain::{ChunkChanged, Terrain},
};

use super::{TerrainEdited, editable};
use crate::inventory::{self, ShovelUse};

/// Clients pace their uses at `shovel::COOLDOWN`; network jitter can bunch
/// them up in transit, so the server enforces a slightly shorter gap.
const MIN_INTERVAL: Duration = shovel::COOLDOWN.saturating_sub(Duration::from_millis(50));
/// How far from the surface a target may be. Requests for points deep in the
/// air or underground did not come from aiming at the terrain.
const MAX_SURFACE_DISTANCE: f32 = 1.0;

pub(super) struct ShovelPlugin;

impl Plugin for ShovelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, apply_shovel_uses.after(inventory::use_items));
    }
}

/// When the player behind a connection last used the shovel.
#[derive(Component)]
struct LastShovelUse(Duration);

fn apply_shovel_uses(
    time: Res<Time>,
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut uses: MessageReader<ShovelUse>,
    last_uses: Query<&LastShovelUse>,
    characters: Query<&Position, With<PlayerId>>,
    mut workers: Query<(&mut Energy, &mut Belongings), Without<Asleep>>,
    mut terrain: ResMut<Terrain>,
    mut edited: MessageWriter<TerrainEdited>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut commands: Commands,
) {
    let now = time.elapsed();
    let mut used_this_frame = HashSet::new();
    for shovel_use in uses.read() {
        let resting = last_uses
            .get(shovel_use.client)
            .is_ok_and(|last| now.saturating_sub(last.0) < MIN_INTERVAL);
        if resting || used_this_frame.contains(&shovel_use.client) {
            continue;
        }
        let (Ok(feet), Ok((mut energy, mut belongings))) = (
            characters.get(shovel_use.character),
            workers.get_mut(shovel_use.character),
        ) else {
            continue;
        };
        if let Err(reason) = validate(shovel_use, feet.0, &terrain, &characters) {
            debug!("rejected {shovel_use:?}: {reason}");
            continue;
        }
        if energy.current() < shovel::ENERGY_COST {
            continue;
        }

        // Settle what moves between terrain and inventory before touching
        // either, so a use is carried out completely or not at all.
        let soil = content.dug_item(shovel::RAISED_MATERIAL);
        match shovel_use.action {
            ShovelAction::Dig => {
                let Some(material) = terrain.surface_material(shovel_use.target) else {
                    continue;
                };
                let dug = content.dug_item(material);
                if belongings.0.room_for(&content, dug) == 0 {
                    debug!("rejected {shovel_use:?}: no room for what it digs up");
                    continue;
                }
                belongings.0.add(&content, dug, 1, clock.0.day());
            }
            ShovelAction::Raise => {
                if !belongings.0.remove(soil, 1) {
                    continue;
                }
            }
        }
        energy.try_spend(shovel::ENERGY_COST);

        let brush = shovel::brush(shovel_use.target, shovel_use.action);
        for changes in terrain.apply_brush(&brush) {
            chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
            edited.write(TerrainEdited(changes));
        }
        used_this_frame.insert(shovel_use.client);
        commands
            .entity(shovel_use.client)
            .insert(LastShovelUse(now));
    }
}

fn validate(
    shovel_use: &ShovelUse,
    feet: Vec3,
    terrain: &Terrain,
    characters: &Query<&Position, With<PlayerId>>,
) -> Result<(), &'static str> {
    let target = shovel_use.target;
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
    if shovel_use.action == ShovelAction::Raise
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
