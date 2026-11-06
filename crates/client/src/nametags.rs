//! Other players' names over their heads, fading with distance.

use bevy::{prelude::*, transform::TransformSystems};
use lightyear::prelude::input::native::InputMarker;
use messoria_shared::protocol::{PlayerId, PlayerInput, PlayerName};

use crate::{
    camera::WorldCamera,
    ui::{self, TEXT_COLOR},
};

/// How high over a character's feet its name floats, in meters.
const ABOVE_FEET: f32 = 2.15;
/// Beyond this distance names are not shown; they fade over the last part.
const SEEN_UP_TO: f32 = 40.0;
const FADE_FROM: f32 = 25.0;
const TAG_WIDTH: f32 = 160.0;
const TAG_SIZE: f32 = 14.0;

pub(crate) struct NameTagsPlugin;

impl Plugin for NameTagsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            place_name_tags.after(TransformSystems::Propagate),
        );
    }
}

/// The name shown over the character that is this entity.
#[derive(Component)]
struct NameTag(Entity);

fn place_name_tags(
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    players: Query<
        (Entity, &GlobalTransform, &PlayerName),
        (With<PlayerId>, Without<InputMarker<PlayerInput>>),
    >,
    mut tags: Query<(
        Entity,
        &NameTag,
        &mut Node,
        &mut Text,
        &mut TextColor,
        &mut Visibility,
    )>,
    mut commands: Commands,
) {
    let (camera, camera_transform) = *camera;
    let mut placed: Vec<Entity> = Vec::new();
    for (entity, tag, mut node, mut text, mut color, mut visibility) in &mut tags {
        let Ok((_, placed_at, name)) = players.get(tag.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        placed.push(tag.0);
        let head = placed_at.translation() + Vec3::Y * ABOVE_FEET;
        let distance = camera_transform.translation().distance(head);
        let on_screen = camera.world_to_viewport(camera_transform, head).ok();
        let Some(at) = on_screen.filter(|_| distance < SEEN_UP_TO) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        node.left = px(at.x - TAG_WIDTH / 2.0);
        node.top = px(at.y - TAG_SIZE);
        if text.0 != name.0 {
            text.0.clone_from(&name.0);
        }
        let fade = ((SEEN_UP_TO - distance) / (SEEN_UP_TO - FADE_FROM)).clamp(0.0, 1.0);
        color.0 = TEXT_COLOR.with_alpha(fade);
    }
    for (player, _, name) in &players {
        if placed.contains(&player) {
            continue;
        }
        commands.spawn((
            Name::new(format!("Name tag of {}", name.0)),
            NameTag(player),
            ui::label(name.0.clone(), TAG_SIZE, TEXT_COLOR),
            TextLayout::justify(Justify::Center),
            TextShadow::default(),
            Node {
                position_type: PositionType::Absolute,
                width: px(TAG_WIDTH),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ));
    }
}
