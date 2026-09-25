//! The river's water: one ribbon along its whole course, at the level the
//! valley gives its water at each point, wide enough to reach up its banks.
//! The ground hides it where the banks rise over it.

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use messoria_shared::valley::Valley;
use messoria_worldgen::RIVER_HALF_WIDTH;

/// How far past the channel's edge the water reaches, under the banks.
const OVER_THE_BANKS: f32 = 0.8;
const WATER_COLOR: Color = Color::srgba(0.07, 0.33, 0.56, 0.9);

pub(crate) struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, pour_the_river);
    }
}

/// The river's water, once the valley is known.
#[derive(Component)]
struct River;

fn pour_the_river(
    valley: Option<Res<Valley>>,
    poured: Query<(), With<River>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(valley) = valley else {
        return;
    };
    if !poured.is_empty() {
        return;
    }
    let course: Vec<_> = valley.river().collect();
    let mut positions = Vec::with_capacity(course.len() * 2);
    for (index, point) in course.iter().enumerate() {
        let before = course[index.saturating_sub(1)].position;
        let after = course[(index + 1).min(course.len() - 1)].position;
        let across =
            (after - before).normalize_or(Vec2::Y).perp() * (RIVER_HALF_WIDTH + OVER_THE_BANKS);
        for side in [point.position - across, point.position + across] {
            positions.push([side.x, point.level, side.y]);
        }
    }
    let count = u32::try_from(course.len()).expect("a few hundred points");
    let indices: Vec<u32> = (0..count - 1)
        .flat_map(|index| {
            let (a, b, c, d) = (2 * index, 2 * index + 1, 2 * index + 2, 2 * index + 3);
            [a, b, c, b, d, c, a, c, b, b, c, d]
        })
        .collect();
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0.0, 1.0, 0.0]; positions.len()],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices));
    commands.spawn((
        Name::new("River"),
        River,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: WATER_COLOR,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.6,
            reflectance: 0.1,
            ..default()
        })),
        Transform::default(),
        NotShadowCaster,
    ));
}
