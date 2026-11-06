//! A looking glass: a figure dressed in whatever look a window wants shown,
//! drawn by a camera of its own into an image the window puts on screen,
//! turning slowly so the whole look is seen.
//!
//! The figure stands far below the valley on a render layer nothing else
//! is on, and is posed like any other, so it breathes and blinks. The
//! camera only draws while a window shows the glass.

use std::f32::consts::FRAC_PI_6;

use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
};
use messoria_content::Look;
use messoria_shared::content::Content;

use crate::{
    art::DrawnOnLayer,
    avatars::dress,
    figures::{Figure, FigureMeshes},
    skins::Tailor,
};

const MIRROR_LAYER: usize = 4;
const MIRROR_ORIGIN: Vec3 = Vec3::new(0.0, -2400.0, 0.0);
/// The image's size, in pixels, which the windows show as it is.
pub(crate) const MIRROR_SIZE: (u32, u32) = (220, 330);
/// Where the camera stands from the figure, which faces -z, and what it
/// looks at.
const CAMERA_AT: Vec3 = Vec3::new(0.0, 1.0, -2.3);
const LOOKS_AT: Vec3 = Vec3::new(0.0, 0.92, 0.0);
/// How fast the figure turns to show every side, in radians per second.
const TURN_RATE: f32 = 0.55;

pub(crate) struct MirrorPlugin;

impl Plugin for MirrorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, stage_mirror)
            .add_systems(PostUpdate, dress_mirror);
    }
}

/// The glass: what it shows this frame, asked for by whichever window is
/// open, and the figure in it.
#[derive(Resource)]
pub(crate) struct Mirror {
    pub image: Handle<Image>,
    subject: Entity,
    camera: Entity,
    shown: Option<Look>,
    wanted: Option<Look>,
}

impl Mirror {
    /// Shows `look` this frame; windows ask every frame they are open.
    pub(crate) fn show(&mut self, look: &Look) {
        self.wanted = Some(look.clone());
    }
}

fn stage_mirror(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let layer = RenderLayers::layer(MIRROR_LAYER);
    let image = images.add(Image::new_target_texture(
        MIRROR_SIZE.0,
        MIRROR_SIZE.1,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands.spawn((
        Name::new("Mirror light"),
        DirectionalLight {
            illuminance: 7_000.0,
            ..default()
        },
        Transform::from_translation(MIRROR_ORIGIN).looking_to(Vec3::new(-0.4, -1.0, 0.7), Vec3::Y),
        layer.clone(),
    ));
    let camera = commands
        .spawn((
            Name::new("Mirror camera"),
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::NONE),
                is_active: false,
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            Projection::Perspective(PerspectiveProjection {
                fov: FRAC_PI_6 * 1.9,
                ..default()
            }),
            Transform::from_translation(MIRROR_ORIGIN + CAMERA_AT)
                .looking_at(MIRROR_ORIGIN + LOOKS_AT, Vec3::Y),
            layer.clone(),
        ))
        .id();
    let subject = commands
        .spawn((
            Name::new("Mirror figure"),
            Transform::from_translation(MIRROR_ORIGIN),
            Visibility::default(),
            layer,
            DrawnOnLayer(MIRROR_LAYER),
        ))
        .id();
    commands.insert_resource(Mirror {
        image,
        subject,
        camera,
        shown: None,
        wanted: None,
    });
}

/// Dresses the figure in the look wanted this frame, if it changed, turns
/// it, and draws only while something wants it shown.
fn dress_mirror(
    time: Res<Time>,
    content: Res<Content>,
    meshes: Res<FigureMeshes>,
    mut mirror: ResMut<Mirror>,
    mut tailor: Tailor,
    figures: Query<&Figure>,
    mut transforms: Query<&mut Transform>,
    mut cameras: Query<&mut Camera>,
    mut commands: Commands,
) {
    let wanted = mirror.wanted.take();
    if let Ok(mut camera) = cameras.get_mut(mirror.camera) {
        camera.is_active = wanted.is_some();
    }
    let Some(look) = wanted else {
        return;
    };
    if let Ok(mut transform) = transforms.get_mut(mirror.subject) {
        transform.rotate_y(TURN_RATE * time.delta_secs());
    }
    if mirror.shown.as_ref() == Some(&look) {
        return;
    }
    if let Ok(figure) = figures.get(mirror.subject) {
        commands.entity(figure.root).despawn();
    }
    dress(
        &mut commands,
        &content,
        &meshes,
        &mut tailor,
        mirror.subject,
        &look,
    );
    mirror.shown = Some(look);
}
