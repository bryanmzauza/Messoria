//! The look shared by on-screen windows: colors, text and buttons.

use bevy::prelude::*;

pub(crate) const WINDOW_COLOR: Color = Color::srgba(0.05, 0.05, 0.07, 0.85);
pub(crate) const TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
pub(crate) const MUTED_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.5);
pub(crate) const HEADING_SIZE: f32 = 20.0;
pub(crate) const TEXT_SIZE: f32 = 14.0;
const BUTTON_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
const BUTTON_HOVER_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.25);

pub(crate) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, highlight_buttons);
    }
}

/// A button drawn in the shared style.
#[derive(Component)]
pub(crate) struct StyledButton;

/// Visibility for something shown exactly while `visible` holds.
pub(crate) fn visible_if(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// Text in the shared style.
pub(crate) fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

/// Spawns a button showing `text` under `parent`, carrying `action` so a
/// system can tell which button was pressed.
pub(crate) fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: impl Component,
) {
    parent
        .spawn((
            StyledButton,
            Button,
            action,
            Node {
                padding: UiRect::axes(px(8), px(4)),
                ..default()
            },
            BackgroundColor(BUTTON_COLOR),
        ))
        .with_child(label(text, TEXT_SIZE, TEXT_COLOR));
}

fn highlight_buttons(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (With<StyledButton>, Changed<Interaction>),
    >,
) {
    for (interaction, mut color) in &mut buttons {
        color.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => BUTTON_HOVER_COLOR,
            Interaction::None => BUTTON_COLOR,
        };
    }
}
