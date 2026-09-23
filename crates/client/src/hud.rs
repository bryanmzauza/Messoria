//! On-screen overlay. For now, a crosshair while the mouse drives the view.

use bevy::prelude::*;

use crate::camera::View;

const CROSSHAIR_SIZE: f32 = 4.0;
const CROSSHAIR_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.8);

pub(crate) struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_crosshair)
            .add_systems(Update, show_crosshair);
    }
}

#[derive(Component)]
struct Crosshair;

fn spawn_crosshair(mut commands: Commands) {
    commands.spawn((
        Name::new("Crosshair"),
        Crosshair,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(CROSSHAIR_SIZE),
            height: px(CROSSHAIR_SIZE),
            margin: UiRect {
                left: px(-CROSSHAIR_SIZE / 2.0),
                top: px(-CROSSHAIR_SIZE / 2.0),
                ..default()
            },
            ..default()
        },
        BackgroundColor(CROSSHAIR_COLOR),
        Visibility::Hidden,
    ));
}

fn show_crosshair(view: Res<View>, mut crosshair: Single<&mut Visibility, With<Crosshair>>) {
    crosshair.set_if_neq(if view.captured {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}
