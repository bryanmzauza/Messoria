//! Carrying out shovel uses: validating them, reshaping the terrain, and
//! moving ground between the terrain and the character's inventory.

use bevy::prelude::*;
use messoria_content::Quality;
use messoria_shared::{
    content::Content,
    energy::Energy,
    movement::{BODY_RADIUS, EYE_HEIGHT},
    protocol::{Asleep, Belongings, PlayerId, Position, WorldClock},
    terrain::{ChunkChanged, Terrain},
    tools::{self, ShovelAction},
};
use messoria_voxel::Brush;

use super::{GroundReshaped, TerrainEdited, editable};
use crate::inventory::{ItemUseSystems, ShovelUse};

/// How far from the surface a target may be. Requests for points deep in the
/// air or underground did not come from aiming at the terrain.
const MAX_SURFACE_DISTANCE: f32 = 1.0;

pub(super) struct ShovelPlugin;

impl Plugin for ShovelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, apply_shovel_uses.after(ItemUseSystems));
    }
}

fn apply_shovel_uses(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut uses: MessageReader<ShovelUse>,
    characters: Query<&Position, With<PlayerId>>,
    mut workers: Query<(&mut Energy, &mut Belongings), Without<Asleep>>,
    mut terrain: ResMut<Terrain>,
    mut edited: MessageWriter<TerrainEdited>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
    mut reshaped: MessageWriter<GroundReshaped>,
) {
    for shovel_use in uses.read() {
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
        if energy.current() < tools::SHOVEL_ENERGY {
            continue;
        }

        // Settle what moves between terrain and inventory before touching
        // either, so a use is carried out completely or not at all.
        let soil = content.dug_item(tools::RAISED_MATERIAL);
        match shovel_use.action {
            ShovelAction::Dig => {
                let Some(material) = terrain.surface_material(shovel_use.target) else {
                    continue;
                };
                let dug = content.dug_item(material);
                if belongings.0.room_for(&content, dug, Quality::Normal) == 0 {
                    debug!("rejected {shovel_use:?}: no room for what it digs up");
                    continue;
                }
                belongings
                    .0
                    .add(&content, dug, Quality::Normal, 1, clock.0.day());
            }
            ShovelAction::Raise => {
                if !belongings.0.remove(soil, 1) {
                    continue;
                }
            }
        }
        energy.try_spend(tools::SHOVEL_ENERGY);

        let brush = tools::shovel_brush(shovel_use.target, shovel_use.action);
        for changes in terrain.apply_brush(&brush) {
            chunk_changed.write_batch(changes.affected_chunks().map(ChunkChanged));
            edited.write(TerrainEdited(changes));
        }
        reshaped.write(GroundReshaped(brush));
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
    if !tools::in_reach(feet + Vec3::Y * EYE_HEIGHT, target) {
        return Err("target out of reach");
    }
    if !terrain
        .distance(target)
        .is_some_and(|distance| distance.abs() <= MAX_SURFACE_DISTANCE)
    {
        return Err("target is not on the terrain surface");
    }
    if shovel_use.action == ShovelAction::Raise {
        let brush = tools::shovel_brush(target, shovel_use.action);
        if characters
            .iter()
            .any(|character| lifts_body(&brush, character.0))
        {
            return Err("raising would bury a player");
        }
    }
    Ok(())
}

/// Whether `brush` would lift the ground under any part of a body standing
/// at `feet`. The ground moves most at the point of the body's footprint
/// nearest the brush's middle.
fn lifts_body(brush: &Brush, feet: Vec3) -> bool {
    let middle = brush.center.xz();
    let nearest = feet.xz() + (middle - feet.xz()).clamp_length_max(BODY_RADIUS);
    brush.shift(nearest, feet.y) > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raise_at(x: f32, y: f32) -> Brush {
        tools::shovel_brush(Vec3::new(x, y, 0.0), ShovelAction::Raise)
    }

    #[test]
    fn raising_next_to_a_body_lifts_it() {
        let feet = Vec3::new(0.0, 10.0, 0.0);
        assert!(lifts_body(&raise_at(1.0, 10.0), feet));
        assert!(lifts_body(&raise_at(tools::BRUSH_RADIUS, 10.2), feet));
    }

    #[test]
    fn raising_at_arms_length_or_below_the_feet_does_not() {
        let feet = Vec3::new(0.0, 10.0, 0.0);
        assert!(!lifts_body(&raise_at(2.5, 10.0), feet));
        assert!(!lifts_body(&raise_at(0.5, 8.0), feet));
    }
}
