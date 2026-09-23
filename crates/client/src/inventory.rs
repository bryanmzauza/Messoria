//! The local player's inventory: choosing the held hotbar slot, and the
//! hotbar and backpack on screen.
//!
//! Number keys and the mouse wheel pick the held slot. Tab opens the
//! backpack; while it is open, clicking one slot and then another moves the
//! first slot's stack onto the second.

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_calendar::WorldTime;
use messoria_inventory::{HOTBAR_SLOTS, SLOTS, Stack};
use messoria_shared::{
    content::Content,
    protocol::{ActionChannel, Belongings, MoveItem, PlayerInput},
};

use crate::clock::LocalClock;

const TOGGLE_KEY: KeyCode = KeyCode::Tab;
/// Number keys in hotbar order: 1 to 9, then 0 for the tenth slot.
const HOTBAR_KEYS: [KeyCode; HOTBAR_SLOTS] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::Digit0,
];

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 4.0;
const SLOT_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const SLOT_BORDER: Color = Color::srgba(1.0, 1.0, 1.0, 0.15);
const HELD_BORDER: Color = Color::srgb(0.95, 0.8, 0.3);
const PICKED_BORDER: Color = Color::srgb(0.4, 0.8, 1.0);
const LABEL_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
/// Distance of the hotbar from the bottom of the screen.
pub(crate) const HOTBAR_BOTTOM: f32 = 16.0;

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldSlot>()
            .init_resource::<InventoryOpen>()
            .init_resource::<PickedSlot>()
            .add_systems(Startup, spawn_inventory)
            .add_systems(
                Update,
                (choose_held_slot, toggle_backpack, click_slots, show_slots).chain(),
            );
    }
}

/// The hotbar slot whose item the player holds and uses.
#[derive(Resource, Default)]
pub(crate) struct HeldSlot(pub usize);

/// Whether the backpack is open, which frees the cursor for clicking slots.
#[derive(Resource, Default)]
pub(crate) struct InventoryOpen(pub bool);

/// A slot clicked while the backpack is open, waiting for where to move it.
#[derive(Resource, Default)]
struct PickedSlot(Option<usize>);

/// Shows the contents of one inventory slot.
#[derive(Component)]
struct SlotView(usize);

#[derive(Component)]
struct SlotName;

#[derive(Component)]
struct SlotCount;

#[derive(Component)]
struct Backpack;

fn spawn_inventory(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Hotbar"),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(HOTBAR_BOTTOM),
                width: percent(100),
                justify_content: JustifyContent::Center,
                column_gap: px(SLOT_GAP),
                ..default()
            },
        ))
        .with_children(|hotbar| {
            for slot in 0..HOTBAR_SLOTS {
                spawn_slot(hotbar, slot);
            }
        });

    commands
        .spawn((
            Name::new("Backpack"),
            Backpack,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen
                .spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(
                        u16::try_from(HOTBAR_SLOTS).expect("a small constant"),
                        SLOT_SIZE,
                    ),
                    row_gap: px(SLOT_GAP),
                    column_gap: px(SLOT_GAP),
                    ..default()
                })
                .with_children(|grid| {
                    for slot in HOTBAR_SLOTS..SLOTS {
                        spawn_slot(grid, slot);
                    }
                });
        });
}

fn spawn_slot(parent: &mut ChildSpawnerCommands, slot: usize) {
    parent
        .spawn((
            SlotView(slot),
            Button,
            Node {
                width: px(SLOT_SIZE),
                height: px(SLOT_SIZE),
                padding: UiRect::all(px(4)),
                border: UiRect::all(px(2)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            BackgroundColor(SLOT_COLOR),
            BorderColor::all(SLOT_BORDER),
        ))
        .with_children(|view| {
            view.spawn((
                SlotName,
                Text::new(""),
                TextFont::from_font_size(11.0),
                TextColor(LABEL_COLOR),
            ));
            view.spawn((
                SlotCount,
                Text::new(""),
                TextFont::from_font_size(13.0),
                TextColor(LABEL_COLOR),
                Node {
                    align_self: AlignSelf::End,
                    ..default()
                },
            ));
        });
}

fn choose_held_slot(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    open: Res<InventoryOpen>,
    mut held: ResMut<HeldSlot>,
) {
    if let Some(slot) = HOTBAR_KEYS.iter().position(|&key| keys.just_pressed(key)) {
        held.0 = slot;
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        // Touchpads report pixels; count about 40 of them as one notch.
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if !open.0 && notches != 0.0 {
        // Scrolling down moves right along the hotbar, as in most games.
        let step = if notches < 0.0 { 1 } else { HOTBAR_SLOTS - 1 };
        held.0 = (held.0 + step) % HOTBAR_SLOTS;
    }
}

fn toggle_backpack(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<InventoryOpen>,
    mut picked: ResMut<PickedSlot>,
    mut backpack: Single<&mut Visibility, With<Backpack>>,
) {
    let toggle = keys.just_pressed(TOGGLE_KEY);
    let close = keys.just_pressed(KeyCode::Escape);
    if toggle || (close && open.0) {
        open.0 = toggle && !open.0;
        picked.0 = None;
        **backpack = if open.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn click_slots(
    open: Res<InventoryOpen>,
    mut picked: ResMut<PickedSlot>,
    clicked: Query<(&Interaction, &SlotView), Changed<Interaction>>,
    mut sender: Query<&mut MessageSender<MoveItem>, With<Client>>,
) {
    if !open.0 {
        return;
    }
    for (interaction, view) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match picked.0.take() {
            None => picked.0 = Some(view.0),
            Some(from) if from == view.0 => {}
            Some(from) => {
                if let Ok(mut sender) = sender.single_mut() {
                    sender.send::<ActionChannel>(MoveItem {
                        from: u8::try_from(from).expect("slot indices fit in u8"),
                        to: u8::try_from(view.0).expect("slot indices fit in u8"),
                    });
                }
            }
        }
    }
}

fn show_slots(
    content: Res<Content>,
    clock: Res<LocalClock>,
    held: Res<HeldSlot>,
    picked: Res<PickedSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut views: Query<(&SlotView, &Children, &mut BorderColor)>,
    mut names: Query<&mut Text, (With<SlotName>, Without<SlotCount>)>,
    mut counts: Query<&mut Text, (With<SlotCount>, Without<SlotName>)>,
) {
    let belongings = player.single().ok();
    let today = clock.time().map(WorldTime::day);
    for (view, children, mut border) in &mut views {
        let stack = belongings.and_then(|belongings| belongings.0.slot(view.0));
        let (name, count) = describe(&content, stack, today);
        for child in children {
            if let Ok(mut text) = names.get_mut(*child) {
                text.set_if_neq(Text::new(name.clone()));
            }
            if let Ok(mut text) = counts.get_mut(*child) {
                text.set_if_neq(Text::new(count.clone()));
            }
        }
        let color = if picked.0 == Some(view.0) {
            PICKED_BORDER
        } else if held.0 == view.0 {
            HELD_BORDER
        } else {
            SLOT_BORDER
        };
        border.set_if_neq(BorderColor::all(color));
    }
}

/// The name and the count line shown for a slot. Perishables also show how
/// many days they have left.
fn describe(content: &Content, stack: Option<&Stack>, today: Option<u32>) -> (String, String) {
    let Some(stack) = stack else {
        return (String::new(), String::new());
    };
    let name = content.item(stack.item).name.clone();
    let count = if stack.count > 1 {
        stack.count.to_string()
    } else {
        String::new()
    };
    let freshness = match (stack.spoils_on, today) {
        (Some(spoils_on), Some(today)) => format!("{}d ", spoils_on.saturating_sub(today)),
        _ => String::new(),
    };
    (name, format!("{freshness}{count}"))
}
