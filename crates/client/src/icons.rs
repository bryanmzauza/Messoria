//! Item icons, rendered from each item's model when the game starts.
//!
//! Every item gets a small picture of its model, or of a lump of its color
//! if it has none, lit from above and seen at an angle. Seeds show their
//! bag with what they grow into in front of it. Each icon has a
//! camera of its own that renders into the icon's image, on a render layer
//! nothing else is on, far below the valley. Once its model has loaded and
//! been framed, the camera keeps rendering for a moment and stops.

use std::{collections::HashMap, time::Duration};

use bevy::{
    camera::{RenderTarget, ScalingMode, primitives::Aabb, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
    world_serialization::WorldInstanceReady,
};
use messoria_content::ItemId;
use messoria_shared::content::Content;

use crate::art::{DrawnOnLayer, Models, item_color};

/// Width and height of an icon, in pixels.
const ICON_SIZE: u32 = 128;
/// The render layer icons are drawn on.
const ICON_LAYER: usize = 3;
/// Where icon models are laid out, and how far apart.
const ICON_ORIGIN: Vec3 = Vec3::new(0.0, -2000.0, 0.0);
const ICON_SPACING: f32 = 12.0;
/// The direction icons are seen from, and how far the camera stands.
const VIEWED_FROM: Vec3 = Vec3::new(0.9, 0.8, 1.3);
const CAMERA_DISTANCE: f32 = 6.0;
/// Room left around a model in its icon, as a share of its size.
const MARGIN: f32 = 1.2;
/// How long an icon keeps rendering after being framed. Shaders compile in
/// the background and meshes are skipped until theirs are ready, so a fixed
/// number of frames could keep an empty picture.
const SETTLING_TIME: Duration = Duration::from_secs(5);
/// A lump's size and squash.
const LUMP_SIZE: Vec3 = Vec3::new(1.0, 0.75, 0.85);
/// Height of what seeds grow into beside their bag, as a share of the bag's.
const BESIDE_SHARE: f32 = 0.55;

pub(crate) struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, stage_icons)
            .add_observer(note_model_ready)
            .add_systems(Update, frame_icons);
    }
}

/// The icon of each item.
#[derive(Resource)]
pub(crate) struct ItemIcons(HashMap<ItemId, Handle<Image>>);

impl ItemIcons {
    pub(crate) fn get(&self, item: ItemId) -> Option<Handle<Image>> {
        self.0.get(&item).cloned()
    }
}

/// What an icon shows, and the camera drawing it.
#[derive(Component)]
struct IconSubject {
    camera: Entity,
    state: Framing,
    /// The item's own model or lump.
    main: Entity,
    /// A part shown in front of the main one, still to be sized and placed.
    beside: Option<Entity>,
}

/// A model in an icon besides the subject's own.
#[derive(Component)]
struct IconPart {
    subject: Entity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Framing {
    /// Waiting for this many models to load.
    Loading(u8),
    /// Loaded; it is framed as soon as its meshes have bounds.
    Ready,
    /// Framed, rendering until this time.
    Settling(Duration),
    Done,
}

fn stage_icons(
    content: Res<Content>,
    models: Res<Models>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let layer = RenderLayers::layer(ICON_LAYER);
    commands.spawn((
        Name::new("Icon light"),
        DirectionalLight {
            illuminance: 6_000.0,
            ..default()
        },
        Transform::from_translation(ICON_ORIGIN).looking_to(Vec3::new(-0.5, -1.0, -0.8), Vec3::Y),
        layer.clone(),
    ));
    let lump = meshes.add(Sphere::new(0.5));

    let mut icons = HashMap::new();
    #[expect(clippy::cast_precision_loss, reason = "a few dozen items")]
    for (index, (id, item)) in content.items().enumerate() {
        let image = images.add(Image::new_target_texture(
            ICON_SIZE,
            ICON_SIZE,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        icons.insert(id, image.clone());
        let spot = ICON_ORIGIN + Vec3::X * ICON_SPACING * index as f32;

        let camera = commands
            .spawn((
                Name::new(format!("{} icon camera", item.name)),
                Camera3d::default(),
                Camera {
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                RenderTarget::Image(image.into()),
                Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::Fixed {
                        width: 1.0,
                        height: 1.0,
                    },
                    ..OrthographicProjection::default_3d()
                }),
                Transform::from_translation(spot + VIEWED_FROM.normalize() * CAMERA_DISTANCE)
                    .looking_at(spot, Vec3::Y),
                layer.clone(),
            ))
            .id();

        let subject = commands
            .spawn((
                Name::new(format!("{} icon", item.name)),
                Transform::from_translation(spot),
                Visibility::default(),
                layer.clone(),
                DrawnOnLayer(ICON_LAYER),
            ))
            .id();
        let mut part = |item: ItemId, commands: &mut Commands| {
            let scene = content.item(item).model.as_deref();
            // A scene is waited for; a lump is there at once.
            if let Some(scene) = scene.and_then(|model| models.scene(model)) {
                return (commands.spawn((scene, ChildOf(subject))).id(), 1);
            }
            let lump = commands.spawn((
                Mesh3d(lump.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: item_color(&content, item),
                    perceptual_roughness: 0.8,
                    ..default()
                })),
                Transform::from_scale(LUMP_SIZE),
                ChildOf(subject),
            ));
            (lump.id(), 0)
        };
        let (main, mut loading) = part(id, &mut commands);
        let beside = content
            .crop_grown_from(id)
            .map(|crop| part(content.crop(crop).produce, &mut commands))
            .map(|(beside, pending)| {
                loading += pending;
                beside
            });
        for part in [Some(main), beside].into_iter().flatten() {
            commands.entity(part).insert(IconPart { subject });
        }
        commands.entity(subject).insert(IconSubject {
            camera,
            state: if loading == 0 {
                Framing::Ready
            } else {
                Framing::Loading(loading)
            },
            main,
            beside,
        });
    }
    commands.insert_resource(ItemIcons(icons));
}

fn note_model_ready(
    trigger: On<WorldInstanceReady>,
    parts: Query<&IconPart>,
    mut subjects: Query<&mut IconSubject>,
) {
    let Ok(part) = parts.get(trigger.entity) else {
        return;
    };
    if let Ok(mut subject) = subjects.get_mut(part.subject)
        && let Framing::Loading(waiting) = subject.state
    {
        subject.state = if waiting > 1 {
            Framing::Loading(waiting - 1)
        } else {
            Framing::Ready
        };
    }
}

/// Fits each loaded model in its camera's view, then lets the camera stop
/// once the framed picture has been drawn.
fn frame_icons(
    time: Res<Time<Real>>,
    mut subjects: Query<(Entity, &mut IconSubject)>,
    descendants: Query<&Children>,
    bounds: Query<(&Aabb, &GlobalTransform)>,
    mut parts: Query<(&mut Transform, &GlobalTransform), Without<Camera>>,
    mut cameras: Query<(&mut Camera, &mut Projection, &mut Transform)>,
) {
    let corners_of = |root: Entity| -> Vec<Vec3> {
        std::iter::once(root)
            .chain(descendants.iter_descendants(root))
            .filter_map(|mesh| bounds.get(mesh).ok())
            .flat_map(|(aabb, transform)| {
                corners(aabb).map(|corner| transform.transform_point(corner))
            })
            .collect()
    };
    for (entity, mut subject) in &mut subjects {
        match subject.state {
            Framing::Loading(_) | Framing::Done => {}
            Framing::Settling(until) => {
                if time.elapsed() < until {
                    continue;
                }
                if let Ok((mut camera, ..)) = cameras.get_mut(subject.camera) {
                    camera.is_active = false;
                }
                subject.state = Framing::Done;
            }
            Framing::Ready => {
                if let Some(beside) = subject.beside {
                    // Placed this frame, framed the next, once moved.
                    let (Some(main), Some(other)) = (
                        extent(&corners_of(subject.main)),
                        extent(&corners_of(beside)),
                    ) else {
                        continue;
                    };
                    if let Ok((mut transform, global)) = parts.get_mut(beside) {
                        place_beside(&mut transform, global.translation(), main, other);
                    }
                    subject.beside = None;
                    continue;
                }
                let corners = corners_of(entity);
                let Some((min, max)) = extent(&corners) else {
                    continue;
                };
                let Ok((_, mut projection, mut transform)) = cameras.get_mut(subject.camera) else {
                    continue;
                };
                let middle = (min + max) / 2.0;
                *transform =
                    Transform::from_translation(middle + VIEWED_FROM.normalize() * CAMERA_DISTANCE)
                        .looking_at(middle, Vec3::Y);
                // The model's extent across the view, seen from the camera.
                let (right, up) = (transform.right(), transform.up());
                let extent = corners.iter().fold(0.0_f32, |extent, &corner| {
                    let offset = corner - middle;
                    extent
                        .max(offset.dot(*right).abs())
                        .max(offset.dot(*up).abs())
                });
                let size = (2.0 * extent * MARGIN).max(0.01);
                if let Projection::Orthographic(orthographic) = &mut *projection {
                    orthographic.scaling_mode = ScalingMode::Fixed {
                        width: size,
                        height: size,
                    };
                }
                subject.state = Framing::Settling(time.elapsed() + SETTLING_TIME);
            }
        }
    }
}

/// Scales a part to a share of the main part's height and stands it on the
/// ground in front of the main part, toward the camera and to its right.
fn place_beside(transform: &mut Transform, origin: Vec3, main: (Vec3, Vec3), part: (Vec3, Vec3)) {
    let ((main_min, main_max), (part_min, part_max)) = (main, part);
    let factor = BESIDE_SHARE * (main_max.y - main_min.y) / (part_max.y - part_min.y).max(0.01);
    let toward_camera = Vec3::new(VIEWED_FROM.x, 0.0, VIEWED_FROM.z).normalize();
    let right = Vec3::Y.cross(toward_camera);
    let reach = (main_max - main_min).xz().max_element() / 2.0;
    let main_middle = (main_min + main_max) / 2.0;
    let spot = Vec3::new(main_middle.x, main_min.y, main_middle.z)
        + (toward_camera * 0.8 + right * 0.6) * reach;
    let foot = Vec3::new(
        f32::midpoint(part_min.x, part_max.x),
        part_min.y,
        f32::midpoint(part_min.z, part_max.z),
    );
    // Scaling about the part's origin moves its foot; it is set back on the spot.
    transform.translation += spot - (origin + (foot - origin) * factor);
    transform.scale *= factor;
}

/// The smallest box holding the given points, if there are any.
fn extent(points: &[Vec3]) -> Option<(Vec3, Vec3)> {
    (!points.is_empty()).then(|| {
        points
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(min, max), &point| {
                (min.min(point), max.max(point))
            })
    })
}

fn corners(aabb: &Aabb) -> [Vec3; 8] {
    let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
    ]
    .map(|sign| center + half * sign)
}
