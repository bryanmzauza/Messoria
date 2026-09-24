//! Ground cover: grass tufts, flowers and mushrooms on grassy ground around
//! the viewer.
//!
//! Cover is decoration, grown by each client for the chunks near its camera
//! from the chunk's position, so it is the same every time without crossing
//! the network. It grows only on flat grass that nothing stands on, so it
//! goes where the ground is dug, tilled or paved. A chunk's plants are merged
//! into one mesh per palette material, so thousands of them cost a handful
//! of draw calls.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMesh, GltfNode},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use messoria_calendar::Season;
use messoria_shared::{
    content::Content,
    fields::tile_at,
    protocol::{Field, Prop},
    terrain::{ChunkChanged, Terrain},
};
use messoria_voxel::{CHUNK_SIZE, ChunkPos, Material};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::{
    art::{DrawnSeason, Models, PaletteMaterials},
    camera::WorldCamera,
};

/// Horizontal distance, in chunks, within which chunks get cover.
const COVER_RADIUS: i32 = 2;
/// Time per frame spent growing cover.
const COVER_BUDGET: Duration = Duration::from_millis(3);
/// Least upward component of the ground's normal for cover to grow.
const MIN_FLATNESS: f32 = 0.85;
/// Space kept free around props, besides their footprint.
const PROP_CLEARANCE: f32 = 0.6;
/// Side of a chunk, in meters.
#[expect(clippy::cast_precision_loss, reason = "a small constant")]
const CHUNK_METERS: f32 = CHUNK_SIZE as f32;

pub(crate) struct CoverPlugin;

impl Plugin for CoverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cover>()
            .init_resource::<ModelParts>()
            .add_systems(
                Update,
                (take_model_parts, note_stale_cover, grow_cover).chain(),
            );
    }
}

/// The cover entities of each chunk that has cover, and chunks whose cover
/// no longer matches the ground.
#[derive(Resource, Default)]
struct Cover {
    grown: HashMap<ChunkPos, Vec<Entity>>,
    stale: HashSet<ChunkPos>,
}

/// The triangles of each cover model, by material name, ready to merge.
#[derive(Resource, Default)]
struct ModelParts(HashMap<String, Vec<Part>>);

/// The triangles of one material in a model, in the model's own space.
struct Part {
    material: String,
    positions: Vec<Vec3>,
    indices: Vec<u32>,
}

/// Reads each cover model's triangles once it has loaded.
fn take_model_parts(
    content: Res<Content>,
    models: Res<Models>,
    files: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    mut parts: ResMut<ModelParts>,
) {
    for path in content.cover().iter().flat_map(|cover| &cover.models) {
        if parts.0.contains_key(path) {
            continue;
        }
        let Some(file) = models.file(path).and_then(|file| files.get(file)) else {
            continue;
        };
        if let Some(model) = model_parts(file, &nodes, &gltf_meshes, &meshes) {
            parts.0.insert(path.clone(), model);
        }
    }
}

/// Every named-material triangle of `file`, placed by its nodes, or `None`
/// while any of it is still loading.
fn model_parts(
    file: &Gltf,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<Vec<Part>> {
    let material_names: HashMap<_, _> = file
        .named_materials
        .iter()
        .map(|(name, material)| (material.id(), name.to_string()))
        .collect();
    let children: HashSet<_> = file
        .nodes
        .iter()
        .filter_map(|node| nodes.get(node))
        .flat_map(|node| node.children.iter().map(Handle::id))
        .collect();
    let mut pending: Vec<(Handle<GltfNode>, Mat4)> = file
        .nodes
        .iter()
        .filter(|node| !children.contains(&node.id()))
        .map(|node| (node.clone(), Mat4::IDENTITY))
        .collect();

    let mut parts = Vec::new();
    while let Some((node, parent)) = pending.pop() {
        let node = nodes.get(&node)?;
        let placement = parent * node.transform.to_matrix();
        pending.extend(node.children.iter().map(|child| (child.clone(), placement)));
        let Some(mesh) = &node.mesh else {
            continue;
        };
        for primitive in &gltf_meshes.get(mesh)?.primitives {
            let Some(material) = primitive
                .material
                .as_ref()
                .and_then(|material| material_names.get(&material.id()))
            else {
                continue;
            };
            let mesh = meshes.get(&primitive.mesh)?;
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            let indices = match mesh.indices() {
                Some(indices) => indices
                    .iter()
                    .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
                    .collect(),
                None => (0..u32::try_from(positions.len()).unwrap_or(u32::MAX)).collect(),
            };
            parts.push(Part {
                material: material.clone(),
                positions: positions
                    .iter()
                    .map(|&position| placement.transform_point3(Vec3::from(position)))
                    .collect(),
                indices,
            });
        }
    }
    Some(parts)
}

/// Marks the cover of chunks whose ground changed, where fields or props
/// appeared or went, and all of it when the season turns.
fn note_stale_cover(
    mut changes: MessageReader<ChunkChanged>,
    season: Res<DrawnSeason>,
    fields: Query<&Field, Added<Field>>,
    mut removed_fields: RemovedComponents<Field>,
    props: Query<&Prop, Added<Prop>>,
    mut cover: ResMut<Cover>,
) {
    let cover = &mut *cover;
    cover.stale.extend(changes.read().map(|changed| changed.0));
    for field in &fields {
        let center = field.tile.as_vec2() + 0.5;
        cover
            .stale
            .insert(chunk_at(Vec3::new(center.x, field.height, center.y)));
    }
    for prop in &props {
        cover.stale.insert(chunk_at(prop.position));
    }
    // Where a field was, is not known any more: regrow all of the cover.
    if removed_fields.read().count() > 0 || season.is_changed() {
        cover.stale.extend(cover.grown.keys().copied());
    }
}

fn chunk_at(point: Vec3) -> ChunkPos {
    ChunkPos::containing(point.floor().as_ivec3())
}

fn grow_cover(
    content: Res<Content>,
    terrain: Res<Terrain>,
    season: Res<DrawnSeason>,
    parts: Res<ModelParts>,
    camera: Single<&Transform, With<WorldCamera>>,
    fields: Query<&Field>,
    props: Query<&Prop>,
    mut cover: ResMut<Cover>,
    mut shared: ResMut<PaletteMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let all_models_ready = content
        .cover()
        .iter()
        .flat_map(|cover| &cover.models)
        .all(|model| parts.0.contains_key(model));
    if !all_models_ready {
        return;
    }
    let viewer = chunk_at(camera.translation);
    let near = |chunk: ChunkPos| {
        let offset = chunk.0 - viewer.0;
        offset.x.abs() <= COVER_RADIUS && offset.z.abs() <= COVER_RADIUS
    };

    let cover = &mut *cover;
    cover.grown.retain(|&chunk, entities| {
        let keep = near(chunk);
        if !keep {
            for &entity in entities.iter() {
                commands.entity(entity).despawn();
            }
        }
        keep
    });

    let mut wanted: Vec<ChunkPos> = terrain
        .positions()
        .filter(|&chunk| near(chunk))
        .filter(|&chunk| !cover.grown.contains_key(&chunk) || cover.stale.contains(&chunk))
        .filter(|&chunk| {
            terrain
                .get(chunk)
                .is_some_and(|chunk| chunk.uniform_voxel().is_none())
        })
        .collect();
    wanted.sort_unstable_by_key(|chunk| (chunk.0 - viewer.0).length_squared());

    let tilled: HashSet<IVec2> = fields.iter().map(|field| field.tile).collect();
    let started = Instant::now();
    for chunk in wanted {
        if started.elapsed() > COVER_BUDGET {
            break;
        }
        cover.stale.remove(&chunk);
        for entity in cover.grown.remove(&chunk).unwrap_or_default() {
            commands.entity(entity).despawn();
        }
        let origin = chunk.origin().as_vec3();
        let nearby_props: Vec<(Vec2, f32)> = props
            .iter()
            .filter(|prop| prop.position.distance(origin + Vec3::splat(16.0)) < 40.0)
            .map(|prop| {
                (
                    prop.position.xz(),
                    content.prop(prop.kind).radius * prop.scale,
                )
            })
            .collect();
        let clear = |point: Vec3| {
            !tilled.contains(&tile_at(point))
                && nearby_props.iter().all(|&(center, footprint)| {
                    center.distance(point.xz()) > footprint + PROP_CLEARANCE
                })
        };

        let entities = plant_chunk(&content, &terrain, season.0, &parts, chunk, clear)
            .into_iter()
            .filter_map(|(name, builder)| {
                let material = shared.get(name, content.palette(), season.0, &mut materials)?;
                Some(
                    commands
                        .spawn((
                            Name::new(format!("Cover {} {name}", chunk.0)),
                            Mesh3d(meshes.add(builder.build())),
                            MeshMaterial3d(material),
                            Transform::from_translation(origin),
                            // Too small for their shadows to be worth drawing.
                            NotShadowCaster,
                        ))
                        .id(),
                )
            })
            .collect();
        cover.grown.insert(chunk, entities);
    }
}

/// Scatters every kind of cover showing in `season` over `chunk`, where
/// `clear` allows, merged by palette material in the chunk's own space.
fn plant_chunk<'a>(
    content: &'a Content,
    terrain: &Terrain,
    season: Season,
    parts: &'a ModelParts,
    chunk: ChunkPos,
    clear: impl Fn(Vec3) -> bool,
) -> HashMap<&'a str, MeshBuilder> {
    let origin = chunk.origin().as_vec3();
    let mut merged: HashMap<&str, MeshBuilder> = HashMap::new();
    for (index, plants) in content.cover().iter().enumerate() {
        if !plants.shows_in(season) {
            continue;
        }
        let mut rng = SmallRng::seed_from_u64(chunk_seed(chunk, index));
        let expected = CHUNK_METERS * CHUNK_METERS * plants.per_square_meter;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few hundred plants"
        )]
        let count =
            (expected.floor() + f32::from(u8::from(rng.random::<f32>() < expected.fract()))) as u32;
        for _ in 0..count {
            let spot = Vec2::new(rng.random(), rng.random()) * CHUNK_METERS;
            let model = &plants.models[rng.random_range(0..plants.models.len())];
            let scale = rng.random_range(plants.scale.0..=plants.scale.1);
            let turn = rng.random_range(0.0..std::f32::consts::TAU);
            let Some(point) = grassy_ground(terrain, chunk, origin, spot) else {
                continue;
            };
            if !clear(point) {
                continue;
            }
            let placement = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                Quat::from_rotation_y(turn),
                point - origin,
            );
            for part in &parts.0[model] {
                merged
                    .entry(part.material.as_str())
                    .or_default()
                    .add(part, placement);
            }
        }
    }
    merged
}

/// The ground at `spot` within `chunk`, if it is flat grass inside the chunk.
fn grassy_ground(terrain: &Terrain, chunk: ChunkPos, origin: Vec3, spot: Vec2) -> Option<Vec3> {
    let above = Vec3::new(
        origin.x + spot.x,
        origin.y + CHUNK_METERS + 0.5,
        origin.z + spot.y,
    );
    let height = terrain.surface_below(above, CHUNK_METERS + 0.5)?;
    let point = above.with_y(height);
    let flat = terrain
        .normal(point)
        .is_some_and(|normal| normal.y >= MIN_FLATNESS);
    let grass = terrain.surface_material(point) == Some(Material::Grass);
    (chunk_at(point) == chunk && flat && grass).then_some(point)
}

/// A seed for one kind of cover in one chunk.
fn chunk_seed(chunk: ChunkPos, kind: usize) -> u64 {
    let [x, y, z] = chunk
        .0
        .to_array()
        .map(|coordinate| u64::from(coordinate.cast_unsigned()));
    (x.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ y.wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ z.wrapping_mul(0x94d0_49bb_1331_11eb))
    .wrapping_add(kind as u64)
}

/// Triangles of many plants gathered into one mesh.
#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn add(&mut self, part: &Part, placement: Mat4) {
        let first = u32::try_from(self.positions.len()).unwrap_or(u32::MAX);
        self.positions.extend(
            part.positions
                .iter()
                .map(|&position| placement.transform_point3(position).to_array()),
        );
        // Lit like the ground under them, so thin blades never turn dark
        // when the light is behind them.
        self.normals
            .extend(part.positions.iter().map(|_| Vec3::Y.to_array()));
        self.indices
            .extend(part.indices.iter().map(|&index| first + index));
    }

    fn build(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}
