//! The game menu Escape opens: back to the game, the options, the world's
//! door for whoever hosts it, who is here, and the way out.
//!
//! The world does not pause behind the menu: it is shared with other
//! players, and hosting it means serving them too.

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_shared::protocol::{ActionChannel, Hosting, SetHospitality};

use crate::{
    camera::LookSystems,
    connection::Session,
    launch::{self, Launched},
    options::MENU_LAYER,
    panels::OpenPanel,
    ui::{self, BACKDROP_COLOR, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, TITLE_SIZE},
};

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_game_menu).add_systems(
            Update,
            (
                // Before looking around, so that the click that closes
                // the menu also captures the cursor.
                press_buttons.before(LookSystems),
                show_menu,
            ),
        );
    }
}

#[derive(Component)]
struct GameMenu;

/// The part of the menu rebuilt whenever what it shows changes.
#[derive(Component)]
struct MenuListing;

#[derive(Component, Clone, Copy)]
enum MenuButton {
    BackToGame,
    Options,
    /// Opens the world to guests, or closes it.
    Door(bool),
    Leave,
}

fn spawn_game_menu(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Game menu"),
            GameMenu,
            ui::screen(),
            BackgroundColor(BACKDROP_COLOR),
            GlobalZIndex(MENU_LAYER),
            Visibility::Hidden,
        ))
        .with_child((
            MenuListing,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                ..default()
            },
        ));
}

fn show_menu(
    panel: Res<OpenPanel>,
    session: Res<Session>,
    launched: Res<Launched>,
    hosting: Query<Ref<Hosting>>,
    mut menu: Single<&mut Visibility, With<GameMenu>>,
    listing: Single<Entity, With<MenuListing>>,
    mut commands: Commands,
) {
    let open = *panel == OpenPanel::Menu;
    if panel.is_changed() {
        menu.set_if_neq(ui::visible_if(open));
    }
    let hosting = hosting.single().ok();
    let changed = panel.is_changed() || hosting.as_ref().is_some_and(DetectChanges::is_changed);
    if !open || !changed {
        return;
    }
    let host = matches!(*session, Session::Host);
    let leave = match (&*session, launched.from_title) {
        (_, true) => "Back to the title screen",
        (Session::Host, false) => "Save and quit",
        (Session::Join { .. }, false) => "Quit",
    };
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|column| {
            column.spawn((
                ui::label("Game Menu", TITLE_SIZE, TEXT_COLOR),
                Node {
                    margin: UiRect::bottom(px(14)),
                    ..default()
                },
            ));
            ui::spawn_wide_button(column, "Back to Game", MenuButton::BackToGame);
            ui::spawn_wide_button(column, "Options...", MenuButton::Options);
            if host {
                match hosting.as_deref() {
                    Some(Hosting {
                        open: true,
                        port: Some(port),
                        ..
                    }) => ui::spawn_wide_button(
                        column,
                        format!("Close the world (open on port {port})"),
                        MenuButton::Door(false),
                    ),
                    Some(Hosting {
                        open: false,
                        port: Some(port),
                        ..
                    }) => ui::spawn_wide_button(
                        column,
                        format!("Open the world to friends on port {port}"),
                        MenuButton::Door(true),
                    ),
                    _ => {}
                }
            }
            ui::spawn_wide_button(column, leave, MenuButton::Leave);
            if let Some(hosting) = hosting.as_deref() {
                column.spawn((
                    ui::label(
                        format!("Here now: {}.", hosting.here.join(", ")),
                        TEXT_SIZE,
                        MUTED_TEXT_COLOR,
                    ),
                    Node {
                        margin: UiRect::top(px(14)),
                        max_width: px(420),
                        ..default()
                    },
                ));
            }
            column.spawn(ui::label(
                "The world goes on while this menu is open.",
                TEXT_SIZE,
                MUTED_TEXT_COLOR,
            ));
        });
}

fn press_buttons(
    launched: Res<Launched>,
    buttons: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    mut panel: ResMut<OpenPanel>,
    mut door: Query<&mut MessageSender<SetHospitality>, With<Client>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            MenuButton::BackToGame => *panel = OpenPanel::None,
            MenuButton::Options => *panel = OpenPanel::Options,
            MenuButton::Door(open) => {
                if let Ok(mut sender) = door.single_mut() {
                    sender.send::<ActionChannel>(SetHospitality { open });
                }
            }
            // A hosted world saves itself as the app exits.
            MenuButton::Leave => {
                if launched.from_title {
                    launch::back_to_title(None);
                }
                exit.write(AppExit::Success);
            }
        }
    }
}
