//! Character figures: limbs of boxes painted from a skin, each turning at its
//! joint, built as `characters.ron` describes them.

use bevy::{
    asset::RenderAssetUsages, camera::primitives::Aabb, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use messoria_content::{Characters, Cube, ItemId, ItemKind};
use messoria_shared::content::Content;

/// How far up its handle a tool is held, as a share of its length.
const TOOL_HOLD: f32 = 0.3;
/// How far in front of the hand's middle anything but a tool is held, in
/// figure pixels, so that the fist does not hide it.
const ITEM_REACH: f32 = 3.5;

pub(crate) struct FiguresPlugin;

impl Plugin for FiguresPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_meshes)
            .add_systems(Update, fit_held_items);
    }
}

/// A limb of a figure, in the order of `Limbs::all`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Body,
    Head,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
}

impl Part {
    pub(crate) const ALL: [Self; 6] = [
        Self::Body,
        Self::Head,
        Self::RightArm,
        Self::LeftArm,
        Self::RightLeg,
        Self::LeftLeg,
    ];

    /// The limb it hangs from, if any; the others hang from the figure.
    fn parent(self) -> Option<Self> {
        match self {
            Self::Head | Self::RightArm | Self::LeftArm => Some(Self::Body),
            Self::Body | Self::RightLeg | Self::LeftLeg => None,
        }
    }
}

/// A figure standing on its owner's feet: the entity everything hangs from,
/// each limb's joint, which turns to pose it, and what it is dressed in.
#[derive(Component, Clone)]
pub(crate) struct Figure {
    pub root: Entity,
    pub limbs: [Entity; 6],
    pub material: Handle<StandardMaterial>,
}

impl Figure {
    pub(crate) fn limb(&self, part: Part) -> Entity {
        self.limbs[part as usize]
    }
}

/// The meshes of every limb's boxes, shared by all figures.
#[derive(Resource)]
pub(crate) struct FigureMeshes([Vec<Handle<Mesh>>; 6]);

impl FigureMeshes {
    pub(crate) fn of(&self, part: Part) -> &[Handle<Mesh>] {
        &self.0[part as usize]
    }
}

fn build_meshes(content: Res<Content>, mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    let characters = content.characters();
    let limbs = characters.limbs.all().map(|limb| {
        limb.boxes
            .iter()
            .map(|cube| meshes.add(cube_mesh(cube, characters)))
            .collect()
    });
    commands.insert_resource(FigureMeshes(limbs));
}

/// Builds a figure under `owner`, dressed in `material`.
pub(crate) fn spawn_figure(
    commands: &mut Commands,
    characters: &Characters,
    meshes: &FigureMeshes,
    material: &Handle<StandardMaterial>,
    owner: Entity,
) -> Figure {
    let root = commands
        .spawn((
            Name::new("Figure"),
            Transform::default(),
            Visibility::default(),
            ChildOf(owner),
        ))
        .id();
    let mut limbs = [root; 6];
    // Parents come before the limbs hanging from them.
    for (part, limb) in Part::ALL.into_iter().zip(characters.limbs.all()) {
        let parent = part.parent().map_or(root, |parent| limbs[parent as usize]);
        let joint = commands
            .spawn((
                Transform::from_translation(Vec3::from(limb.joint) * characters.pixel),
                Visibility::default(),
                ChildOf(parent),
            ))
            .id();
        for mesh in &meshes.0[part as usize] {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                ChildOf(joint),
            ));
        }
        limbs[part as usize] = joint;
    }
    Figure {
        root,
        limbs,
        material: material.clone(),
    }
}

/// A held item's model, hidden until it is fitted to the hand: its longest
/// side made `size` meters long, and the point `anchor` of the way up it put
/// in the hand at `grip`.
#[derive(Component)]
pub(crate) struct Fitting {
    size: f32,
    anchor: f32,
    grip: Transform,
}

/// What to spawn a held item's model with, in the right arm's frame, for it
/// to be fitted to the hand.
pub(crate) fn in_hand(content: &Content, item: ItemId) -> impl Bundle {
    let characters = content.characters();
    let grip = characters.grip;
    let (x, y, z) = grip.turn;
    let mut hand = Transform::from_translation(Vec3::from(grip.at) * characters.pixel);
    // Tools are held by the handle, turned as the grip says; anything else
    // upright, by its middle.
    let (size, anchor) = if let ItemKind::Tool(_) = content.item(item).kind {
        hand.rotation = Quat::from_euler(
            EulerRot::XYZ,
            x.to_radians(),
            y.to_radians(),
            z.to_radians(),
        );
        (grip.tool_size, TOOL_HOLD)
    } else {
        hand.translation.z -= ITEM_REACH * characters.pixel;
        (grip.item_size, 0.5)
    };
    (
        hand,
        Visibility::Hidden,
        Fitting {
            size,
            anchor,
            grip: hand,
        },
    )
}

/// Fits each held model once its meshes have bounds, measured in the
/// model's own frame.
fn fit_held_items(
    mut fitting: Query<(
        Entity,
        &Fitting,
        &GlobalTransform,
        &mut Transform,
        &mut Visibility,
    )>,
    descendants: Query<&Children>,
    bounds: Query<(&Aabb, &GlobalTransform)>,
    mut commands: Commands,
) {
    for (entity, fit, placed, mut transform, mut visibility) in &mut fitting {
        let to_model = placed.affine().inverse();
        let (low, high) = descendants
            .iter_descendants(entity)
            .filter_map(|mesh| bounds.get(mesh).ok())
            .flat_map(|(aabb, mesh)| {
                let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
                [-1.0, 1.0].into_iter().flat_map(move |x| {
                    [-1.0, 1.0].into_iter().flat_map(move |y| {
                        [-1.0, 1.0]
                            .map(move |z| mesh.transform_point(center + half * Vec3::new(x, y, z)))
                    })
                })
            })
            .map(|corner| to_model.transform_point3(corner))
            .fold((Vec3::MAX, Vec3::MIN), |(low, high), corner| {
                (low.min(corner), high.max(corner))
            });
        let extent = high - low;
        if !extent.is_finite() || extent.max_element() <= 0.0 {
            continue;
        }
        let scale = fit.size / extent.max_element();
        let middle = (low + high) / 2.0;
        let anchor = Vec3::new(middle.x, low.y + extent.y * fit.anchor, middle.z);
        *transform =
            fit.grip * Transform::from_translation(-anchor * scale).with_scale(Vec3::splat(scale));
        *visibility = Visibility::Inherited;
        commands.entity(entity).remove::<Fitting>();
    }
}

/// A box's mesh, in meters from its limb's joint, with each face painted from
/// its place in the skin (see `characters.ron`).
fn cube_mesh(cube: &Cube, characters: &Characters) -> Mesh {
    let pixel = characters.pixel;
    let grow = Vec3::splat(cube.inflate);
    let low = (Vec3::from(cube.from) - grow) * pixel;
    let high = (Vec3::from(cube.from) + Vec3::from(cube.size) + grow) * pixel;
    #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
    let (texels, skin) = (
        characters.texels as f32,
        Vec2::new(characters.skin_size.0 as f32, characters.skin_size.1 as f32),
    );
    #[expect(clippy::cast_precision_loss, reason = "skins are small images")]
    let origin = Vec2::new(cube.uv.0 as f32, cube.uv.1 as f32);
    let faces = faces(low, high, Vec3::from(cube.size) * texels);

    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (corners, normal, at, size) in faces {
        let first = u16::try_from(positions.len()).expect("a box has 24 corners");
        let picture = [
            Vec2::ZERO,
            Vec2::new(size.x, 0.0),
            size,
            Vec2::new(0.0, size.y),
        ];
        for (corner, spot) in corners.into_iter().zip(picture) {
            positions.push(corner.to_array());
            normals.push(normal.to_array());
            uvs.push(((origin + at + spot) / skin).to_array());
        }
        // Clockwise on the picture is counterclockwise seen from outside.
        indices.extend([0, 3, 2, 0, 2, 1].map(|corner| first + corner));
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U16(indices))
}

/// A box's faces from its lowest to its highest corner: each face's corners
/// from the top left of its picture, clockwise as seen from outside; its
/// normal; and where its picture is within the box's unwrapped faces, and
/// its size, for a box `texels` wide, high and deep.
fn faces(l: Vec3, h: Vec3, texels: Vec3) -> [([Vec3; 4], Vec3, Vec2, Vec2); 6] {
    let (width, height, depth) = (texels.x, texels.y, texels.z);
    [
        // Top, the front toward the bottom of its picture.
        (
            [
                Vec3::new(h.x, h.y, h.z),
                Vec3::new(l.x, h.y, h.z),
                Vec3::new(l.x, h.y, l.z),
                Vec3::new(h.x, h.y, l.z),
            ],
            Vec3::Y,
            Vec2::new(depth, 0.0),
            Vec2::new(width, depth),
        ),
        // Bottom.
        (
            [
                Vec3::new(h.x, l.y, l.z),
                Vec3::new(l.x, l.y, l.z),
                Vec3::new(l.x, l.y, h.z),
                Vec3::new(h.x, l.y, h.z),
            ],
            Vec3::NEG_Y,
            Vec2::new(depth + width, 0.0),
            Vec2::new(width, depth),
        ),
        // Right (+x).
        (
            [
                Vec3::new(h.x, h.y, h.z),
                Vec3::new(h.x, h.y, l.z),
                Vec3::new(h.x, l.y, l.z),
                Vec3::new(h.x, l.y, h.z),
            ],
            Vec3::X,
            Vec2::new(0.0, depth),
            Vec2::new(depth, height),
        ),
        // Front (-z), the figure's right on the left of its picture.
        (
            [
                Vec3::new(h.x, h.y, l.z),
                Vec3::new(l.x, h.y, l.z),
                Vec3::new(l.x, l.y, l.z),
                Vec3::new(h.x, l.y, l.z),
            ],
            Vec3::NEG_Z,
            Vec2::new(depth, depth),
            Vec2::new(width, height),
        ),
        // Left (-x).
        (
            [
                Vec3::new(l.x, h.y, l.z),
                Vec3::new(l.x, h.y, h.z),
                Vec3::new(l.x, l.y, h.z),
                Vec3::new(l.x, l.y, l.z),
            ],
            Vec3::NEG_X,
            Vec2::new(depth + width, depth),
            Vec2::new(depth, height),
        ),
        // Back (+z).
        (
            [
                Vec3::new(l.x, h.y, h.z),
                Vec3::new(h.x, h.y, h.z),
                Vec3::new(h.x, l.y, h.z),
                Vec3::new(l.x, l.y, h.z),
            ],
            Vec3::Z,
            Vec2::new(2.0 * depth + width, depth),
            Vec2::new(width, height),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use messoria_content::Catalog;

    use super::*;

    fn shipped() -> Characters {
        let data = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data");
        Catalog::load(std::path::Path::new(data))
            .expect("the shipped content is valid")
            .characters()
            .clone()
    }

    #[test]
    fn every_face_turns_outward() {
        let characters = shipped();
        let mesh = cube_mesh(&characters.limbs.head.boxes[0], &characters);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("a box has positions");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("a box has normals");
        };
        let Some(Indices::U16(indices)) = mesh.indices() else {
            panic!("a box has indices");
        };
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[usize::from(triangle[i])]));
            let facing = (b - a).cross(c - a);
            let normal = Vec3::from(normals[usize::from(triangle[0])]);
            assert!(facing.dot(normal) > 0.0, "a triangle faces inward");
        }
    }

    #[test]
    fn faces_are_painted_from_inside_the_skin() {
        let characters = shipped();
        for limb in characters.limbs.all() {
            for cube in &limb.boxes {
                let mesh = cube_mesh(cube, &characters);
                let Some(VertexAttributeValues::Float32x2(uvs)) =
                    mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                else {
                    panic!("a box has uvs");
                };
                assert!(uvs.iter().flatten().all(|uv| (0.0..=1.0).contains(uv)));
            }
        }
    }
}
