//! The farm lots' fences and gates on screen: a section of fence from post
//! to post along every run, each standing on the ground and following its
//! slope, and a gateway where each fence opens onto the road.
//!
//! Fences are drawn for the columns drawn in full, as the valley lays them
//! out, so nothing about them crosses the network.

use std::collections::HashMap;

use bevy::prelude::*;
use messoria_shared::{
    content::Content,
    lots::{FENCE_SECTION, fences_in_column, lots},
    valley::Valley,
};
use messoria_worldgen::column_of;

use crate::{
    art::Models,
    terrain::{FullColumns, FullColumnsChanged},
};

pub(crate) struct FencesPlugin;

impl Plugin for FencesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawnFences>()
            .add_systems(Update, draw_fences);
    }
}

/// The fences drawn on each column.
#[derive(Resource, Default)]
struct DrawnFences(HashMap<IVec2, Vec<Entity>>);

/// Draws the fences of columns that came to be drawn in full, and takes
/// down those of columns that are no longer.
fn draw_fences(
    mut changes: MessageReader<FullColumnsChanged>,
    full: Res<FullColumns>,
    valley: Option<Res<Valley>>,
    content: Res<Content>,
    models: Res<Models>,
    mut drawn: ResMut<DrawnFences>,
    mut commands: Commands,
) {
    let Some(valley) = valley else {
        changes.clear();
        return;
    };
    for &FullColumnsChanged(column) in changes.read() {
        if let Some(entities) = drawn.0.remove(&column) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
        if !full.contains(column) {
            continue;
        }
        let mut entities = Vec::new();
        let (sections, _) = fences_in_column(column);
        let looks = &content.village().lots;
        if let Some(fence) = models.scene(&looks.fence) {
            for (start, direction) in sections {
                let end = start + direction * FENCE_SECTION;
                let (from, to) = (valley.height(start), valley.height(end));
                // The model's rails run along +x: turned to the run, and
                // tilted to follow the ground from one post to the next.
                let yaw = (-direction.y).atan2(direction.x);
                let pitch = (to - from).atan2(FENCE_SECTION);
                let entity = commands
                    .spawn((
                        Name::new("Fence"),
                        fence.clone(),
                        Transform::from_xyz(start.x, from, start.y).with_rotation(
                            Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch),
                        ),
                    ))
                    .id();
                entities.push(entity);
            }
        }
        if let Some(gate) = models.scene(&looks.gate) {
            for lot in lots().filter(|lot| column_of(lot.gate) == column) {
                let along = lot.along_road;
                let entity = commands
                    .spawn((
                        Name::new(format!("Gate of lot {}", lot.id.number())),
                        gate.clone(),
                        Transform::from_xyz(lot.gate.x, valley.height(lot.gate), lot.gate.y)
                            .with_rotation(Quat::from_rotation_y((-along.y).atan2(along.x))),
                    ))
                    .id();
                entities.push(entity);
            }
        }
        drawn.0.insert(column, entities);
    }
}
