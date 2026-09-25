//! The look shared by on-screen windows: colors, text, frames and buttons.
//!
//! Windows are dark wood with a brass edge, in keeping with the valley's
//! cozy medieval style.

use bevy::prelude::*;

const WINDOW_COLOR: Color = Color::srgba(0.13, 0.1, 0.08, 0.97);
const WINDOW_EDGE_COLOR: Color = Color::srgb(0.6, 0.45, 0.26);
pub(crate) const TEXT_COLOR: Color = Color::srgb(0.96, 0.92, 0.83);
pub(crate) const MUTED_TEXT_COLOR: Color = Color::srgba(0.96, 0.92, 0.83, 0.55);
pub(crate) const ACCENT_COLOR: Color = Color::srgb(0.96, 0.78, 0.36);
/// Darkens the world behind windows that stop play, such as the game menu.
pub(crate) const BACKDROP_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
pub(crate) const TITLE_SIZE: f32 = 26.0;
pub(crate) const HEADING_SIZE: f32 = 20.0;
pub(crate) const TEXT_SIZE: f32 = 14.0;
const WINDOW_CORNER: f32 = 8.0;
const BUTTON_CORNER: f32 = 4.0;
const BUTTON_COLOR: Color = Color::srgb(0.3, 0.22, 0.15);
const BUTTON_HOVER_COLOR: Color = Color::srgb(0.44, 0.32, 0.2);
const BUTTON_EDGE_COLOR: Color = Color::srgb(0.52, 0.38, 0.22);
const BUTTON_HOVER_EDGE_COLOR: Color = ACCENT_COLOR;
/// Width of the buttons in menus, which stack in a column.
const WIDE_BUTTON_WIDTH: f32 = 300.0;

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

/// A window laid out by `node`, framed in the shared style.
pub(crate) fn window(node: Node) -> impl Bundle {
    (
        Node {
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(WINDOW_CORNER)),
            ..node
        },
        BackgroundColor(WINDOW_COLOR),
        BorderColor::all(WINDOW_EDGE_COLOR),
    )
}

/// A node covering the whole screen that centers what it holds.
pub(crate) fn screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

/// Spawns a button showing `text` under `parent`, carrying `action` so a
/// system can tell which button was pressed.
pub(crate) fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: impl Component,
) {
    spawn_framed_button(
        parent,
        text,
        action,
        Node {
            padding: UiRect::axes(px(8), px(4)),
            ..default()
        },
    );
}

/// Spawns a button as wide as the others in a menu.
pub(crate) fn spawn_wide_button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: impl Component,
) {
    spawn_framed_button(
        parent,
        text,
        action,
        Node {
            width: px(WIDE_BUTTON_WIDTH),
            padding: UiRect::axes(px(12), px(9)),
            justify_content: JustifyContent::Center,
            ..default()
        },
    );
}

fn spawn_framed_button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: impl Component,
    node: Node,
) {
    parent
        .spawn((
            StyledButton,
            Button,
            action,
            Node {
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(BUTTON_CORNER)),
                ..node
            },
            BackgroundColor(BUTTON_COLOR),
            BorderColor::all(BUTTON_EDGE_COLOR),
        ))
        .with_child(label(text, TEXT_SIZE, TEXT_COLOR));
}

fn highlight_buttons(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (With<StyledButton>, Changed<Interaction>),
    >,
) {
    for (interaction, mut color, mut edge) in &mut buttons {
        let hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);
        color.0 = if hovered {
            BUTTON_HOVER_COLOR
        } else {
            BUTTON_COLOR
        };
        *edge = BorderColor::all(if hovered {
            BUTTON_HOVER_EDGE_COLOR
        } else {
            BUTTON_EDGE_COLOR
        });
    }
}
