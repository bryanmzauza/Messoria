//! Aiming the shovel at the terrain, showing where it will act, and asking the
//! server to use it: left button digs, right button raises.
//!
//! Edits are not predicted; the terrain changes when the server's update
//! arrives, which keeps every player's terrain identical.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_shared::{
    movement::EYE_HEIGHT,
    protocol::{ActionChannel, PlayerInput, Position, ShovelAction, ShovelRequest},
    shovel,
    terrain::Terrain,
};
use messoria_voxel::RayHit;

use crate::camera::{LookSystems, View};

const AIM_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.7);
/// Lifts the aim marker off the surface so it is not hidden inside it.
const AIM_LIFT: f32 = 0.03;

pub(crate) struct ShovelPlugin;

impl Plugin for ShovelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>().add_systems(
            Update,
            (
                aim,
                // Before the cursor is captured, so the capturing click does not dig.
                use_shovel.before(LookSystems),
                draw_aim,
            )
                .chain(),
        );
    }
}

/// Where the shovel would act, if the player is aiming at terrain in reach.
#[derive(Resource, Default)]
struct Aim(Option<RayHit>);

fn aim(
    view: Res<View>,
    terrain: Res<Terrain>,
    camera: Single<&Transform, With<Camera3d>>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
    mut aim: ResMut<Aim>,
) {
    aim.0 = None;
    let Ok(feet) = player.single() else {
        return;
    };
    if !view.captured {
        return;
    }
    // Aim along the view, which in third person starts behind the character,
    // but measure reach from the character's eyes as the server does.
    let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
    let max_distance = camera.translation.distance(eyes) + shovel::REACH;
    aim.0 = terrain
        .raycast(camera.translation, *camera.forward(), max_distance)
        .filter(|hit| shovel::in_reach(eyes, hit.point));
}

fn use_shovel(
    mouse: Res<ButtonInput<MouseButton>>,
    view: Res<View>,
    aim: Res<Aim>,
    time: Res<Time>,
    mut held: Local<Option<ShovelAction>>,
    mut ready_at: Local<std::time::Duration>,
    mut sender: Query<&mut MessageSender<ShovelRequest>, With<Client>>,
) {
    for (button, action) in [
        (MouseButton::Left, ShovelAction::Dig),
        (MouseButton::Right, ShovelAction::Raise),
    ] {
        if mouse.just_pressed(button) {
            *held = view.captured.then_some(action);
        }
        if mouse.just_released(button) && *held == Some(action) {
            *held = None;
        }
    }

    let (Some(action), Some(hit)) = (*held, aim.0) else {
        return;
    };
    if time.elapsed() < *ready_at {
        return;
    }
    if let Ok(mut sender) = sender.single_mut() {
        sender.send::<ActionChannel>(ShovelRequest {
            target: hit.point,
            action,
        });
        *ready_at = time.elapsed() + shovel::COOLDOWN;
    }
}

fn draw_aim(aim: Res<Aim>, mut gizmos: Gizmos) {
    if let Some(hit) = aim.0 {
        let facing = Quat::from_rotation_arc(Vec3::Z, hit.normal);
        gizmos.circle(
            Isometry3d::new(hit.point + hit.normal * AIM_LIFT, facing),
            shovel::BRUSH_RADIUS,
            AIM_COLOR,
        );
    }
}
