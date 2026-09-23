//! How characters look.
//!
//! Characters are simulated as a `Position` and a `Heading`; this module gives
//! them a body and keeps its `Transform` in sync, after lightyear has smoothed
//! those values for the current frame.

use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};

use bevy::{prelude::*, transform::TransformSystems};
use lightyear::{
    frame_interpolation::FrameInterpolationSystems, prelude::client::Remote, prelude::*,
};
use messoria_shared::protocol::{Heading, PlayerId, Position};

const BODY_RADIUS: f32 = 0.35;
const BODY_HEIGHT: f32 = 1.8;
/// Height of the eyes above the feet, used by the first-person camera.
pub(crate) const EYE_HEIGHT: f32 = 1.6;

pub(crate) struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_avatar_assets)
            .add_observer(attach_avatar)
            .add_observer(smooth_between_ticks)
            .add_systems(
                PostUpdate,
                place_avatars
                    .in_set(AvatarSystems)
                    .after(FrameInterpolationSystems::Interpolate)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Runs once avatar transforms reflect this frame's character state.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AvatarSystems;

#[derive(Resource)]
struct AvatarAssets {
    body: Handle<Mesh>,
    /// Marks the front of the body so facing is readable from a distance.
    visor: Handle<Mesh>,
    visor_material: Handle<StandardMaterial>,
}

fn load_avatar_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(AvatarAssets {
        body: meshes.add(Capsule3d::new(BODY_RADIUS, BODY_HEIGHT - 2.0 * BODY_RADIUS)),
        visor: meshes.add(Cuboid::new(0.4, 0.12, 0.1)),
        visor_material: materials.add(Color::srgb(0.12, 0.12, 0.14)),
    });
}

fn attach_avatar(
    trigger: On<Add, PlayerId>,
    players: Query<&PlayerId>,
    assets: Res<AvatarAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Ok(&PlayerId(peer)) = players.get(trigger.entity) else {
        return;
    };
    let body_material = materials.add(StandardMaterial {
        base_color: player_color(peer),
        perceptual_roughness: 0.8,
        ..default()
    });

    commands
        .entity(trigger.entity)
        .insert((Transform::default(), Visibility::default()))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(assets.body.clone()),
                MeshMaterial3d(body_material),
                Transform::from_xyz(0.0, BODY_HEIGHT / 2.0, 0.0),
            ));
            parent.spawn((
                Mesh3d(assets.visor.clone()),
                MeshMaterial3d(assets.visor_material.clone()),
                Transform::from_xyz(0.0, EYE_HEIGHT, -BODY_RADIUS),
            ));
        });
}

/// Characters simulated in this app (all of them on a host, the predicted one
/// on a client) change once per tick; blending between ticks keeps them smooth
/// at any frame rate. Interpolated remote characters are already smoothed
/// between snapshots.
fn smooth_between_ticks(
    trigger: On<Add, (PlayerId, Predicted)>,
    players: Query<(Has<Remote>, Has<Predicted>), With<PlayerId>>,
    mut commands: Commands,
) {
    if let Ok((remote, predicted)) = players.get(trigger.entity)
        && (predicted || !remote)
    {
        commands.entity(trigger.entity).insert(FrameInterpolate);
    }
}

fn place_avatars(mut avatars: Query<(&Position, &Heading, &mut Transform), With<PlayerId>>) {
    for (position, heading, mut transform) in &mut avatars {
        transform.translation = position.0;
        transform.rotation = Quat::from_rotation_y(heading.0);
    }
}

/// A stable, distinct color per player.
fn player_color(peer: PeerId) -> Color {
    let hash = BuildHasherDefault::<DefaultHasher>::default().hash_one(peer.to_bits());
    #[expect(
        clippy::cast_precision_loss,
        reason = "the value is reduced below 360 first"
    )]
    let hue = (hash % 360) as f32;
    Color::hsl(hue, 0.55, 0.55)
}
