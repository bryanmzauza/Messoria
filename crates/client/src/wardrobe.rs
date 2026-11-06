//! The wardrobe in the cabin: the window for changing clothes.
//!
//! `F` at a wardrobe opens it; `F` again, Escape or walking away closes
//! it. The window shows what the player wears, in the looking glass, and
//! every garment they carry, with a button to change into each; a hat can
//! be taken off. The server makes every change, and what comes off goes
//! back into the backpack.

use bevy::prelude::*;
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_content::{ItemKind, Wear};
use messoria_shared::{
    content::Content,
    movement::EYE_HEIGHT,
    protocol::{
        ActionChannel, Appearance, Belongings, ChangeClothes, PlayerInput, Position, Structure,
    },
    tools,
};

use crate::{
    actions::INTERACT_KEY,
    creator::garment_name,
    icons::ItemIcons,
    mirror::{MIRROR_SIZE, Mirror},
    panels::OpenPanel,
    ui::{self, HEADING_SIZE, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE},
};

const ICON_SIZE: f32 = 36.0;

pub(crate) struct WardrobePlugin;

impl Plugin for WardrobePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_wardrobe_window).add_systems(
            Update,
            (close_far_wardrobe, show_wardrobe_window, press_buttons),
        );
    }
}

#[derive(Component)]
struct WardrobeWindow;

#[derive(Component)]
struct WardrobeListing;

#[derive(Component, Clone, Copy)]
struct ChangeButton(ChangeClothes);

fn spawn_wardrobe_window(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Wardrobe window"),
            WardrobeWindow,
            ui::screen(),
            Visibility::Hidden,
        ))
        .with_child((
            WardrobeListing,
            ui::window(Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(8),
                min_width: px(560),
                ..default()
            }),
        ));
}

fn close_far_wardrobe(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut panel: ResMut<OpenPanel>,
    wardrobes: Query<&Structure>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
) {
    let OpenPanel::Wardrobe(wardrobe) = *panel else {
        return;
    };
    let within_reach = player
        .single()
        .ok()
        .zip(wardrobes.get(wardrobe).ok())
        .is_some_and(|(feet, wardrobe)| {
            tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, wardrobe.position)
        });
    if !within_reach || keys.clear_just_pressed(INTERACT_KEY) {
        *panel = OpenPanel::None;
    }
}

fn show_wardrobe_window(
    panel: Res<OpenPanel>,
    content: Res<Content>,
    icons: Res<ItemIcons>,
    mut mirror: ResMut<Mirror>,
    player: Query<(Ref<Belongings>, Ref<Appearance>), With<InputMarker<PlayerInput>>>,
    mut window: Single<&mut Visibility, With<WardrobeWindow>>,
    listing: Single<Entity, With<WardrobeListing>>,
    mut commands: Commands,
) {
    let open = matches!(*panel, OpenPanel::Wardrobe(_));
    window.set_if_neq(ui::visible_if(open));
    let Ok((belongings, appearance)) = player.single() else {
        return;
    };
    if !open {
        return;
    }
    let look = &appearance.0;
    mirror.show(look);
    if !panel.is_changed() && !belongings.is_changed() && !appearance.is_changed() {
        return;
    }
    let image = mirror.image.clone();
    commands
        .entity(*listing)
        .despawn_related::<Children>()
        .with_children(|listing| {
            listing.spawn(ui::label("Wardrobe", HEADING_SIZE, TEXT_COLOR));
            listing
                .spawn(Node {
                    column_gap: px(20),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        flex_grow: 1.0,
                        ..default()
                    })
                    .with_children(|column| {
                        column.spawn(ui::label(
                            format!(
                                "Wearing {}.",
                                garment_name(&content, &look.outfit).to_lowercase()
                            ),
                            TEXT_SIZE,
                            TEXT_COLOR,
                        ));
                        spawn_hat_row(column, &content, look.hat.as_deref());
                        column.spawn(ui::label("In the backpack", HEADING_SIZE, TEXT_COLOR));
                        spawn_garment_rows(column, &content, &icons, &belongings);
                    });
                    row.spawn((
                        ImageNode::new(image),
                        Node {
                            width: px(MIRROR_SIZE.0),
                            height: px(MIRROR_SIZE.1),
                            ..default()
                        },
                    ));
                });
        });
}

/// What is on the player's head, and a button to take it off.
fn spawn_hat_row(column: &mut ChildSpawnerCommands, content: &Content, hat: Option<&str>) {
    let Some(hat) = hat else {
        column.spawn(ui::label(
            "Nothing on your head.",
            TEXT_SIZE,
            MUTED_TEXT_COLOR,
        ));
        return;
    };
    column
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(ui::label(
                format!(
                    "On your head, {}.",
                    garment_name(content, hat).to_lowercase()
                ),
                TEXT_SIZE,
                TEXT_COLOR,
            ));
            ui::spawn_button(
                row,
                "Take it off",
                ChangeButton(ChangeClothes { slot: None }),
            );
        });
}

/// Every garment in the backpack, with a button to change into it.
fn spawn_garment_rows(
    column: &mut ChildSpawnerCommands,
    content: &Content,
    icons: &ItemIcons,
    belongings: &Belongings,
) {
    let mut any = false;
    for (slot, stack) in belongings.0.slots().iter().enumerate() {
        let Some(stack) = stack else {
            continue;
        };
        let ItemKind::Garment { wear, .. } = content.item(stack.item).kind else {
            continue;
        };
        any = true;
        let verb = match wear {
            Wear::Outfit => "Change into",
            Wear::Hat => "Put on",
        };
        let slot = u8::try_from(slot).expect("slots fit in u8");
        spawn_garment_row(column, content, icons, stack.item, verb, slot);
    }
    if !any {
        column.spawn(ui::label(
            "Nothing to change into. The tailor sells clothes.",
            TEXT_SIZE,
            MUTED_TEXT_COLOR,
        ));
    }
}

fn spawn_garment_row(
    column: &mut ChildSpawnerCommands,
    content: &Content,
    icons: &ItemIcons,
    item: messoria_content::ItemId,
    verb: &str,
    slot: u8,
) {
    column
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            let icon = Node {
                width: px(ICON_SIZE),
                height: px(ICON_SIZE),
                ..default()
            };
            match icons.get(item) {
                Some(image) => {
                    row.spawn((ImageNode::new(image), icon));
                }
                None => {
                    row.spawn(icon);
                }
            }
            row.spawn((
                ui::label(content.item(item).name.clone(), TEXT_SIZE, TEXT_COLOR),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            ui::spawn_button(row, verb, ChangeButton(ChangeClothes { slot: Some(slot) }));
        });
}

fn press_buttons(
    panel: Res<OpenPanel>,
    buttons: Query<(&Interaction, &ChangeButton), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<ChangeClothes>, With<Client>>,
) {
    if !matches!(*panel, OpenPanel::Wardrobe(_)) {
        return;
    }
    let Ok(mut sender) = sender.single_mut() else {
        return;
    };
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            sender.send::<ActionChannel>(button.0);
        }
    }
}
