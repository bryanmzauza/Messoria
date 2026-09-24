//! The local player's inventory: choosing the held hotbar slot, the hotbar
//! on screen, and the backpack window.
//!
//! Number keys and the mouse wheel pick the held slot, whose item is named
//! above the hotbar for a moment. `E` opens the backpack window, which holds
//! the backpack above the hotbar. There, clicking a stack picks it up and
//! it follows the cursor; clicking another slot puts it there, merging with
//! a stack of the same item or swapping with anything else. Hovering over a
//! stack describes it.

use std::{ops::Range, time::Duration};

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::PrimaryWindow,
};
use lightyear::prelude::{input::native::InputMarker, *};
use messoria_calendar::WorldTime;
use messoria_content::{ItemId, ItemKind, Quality, Tool};
use messoria_inventory::{HOTBAR_SLOTS, SLOTS, Stack};
use messoria_shared::{
    content::Content,
    protocol::{ActionChannel, Belongings, MoveItem, PlayerInput},
};

use crate::{
    art::item_color,
    clock::LocalClock,
    panels::OpenPanel,
    ui::{self, ACCENT_COLOR, BACKDROP_COLOR, MUTED_TEXT_COLOR, TEXT_COLOR, TEXT_SIZE, TITLE_SIZE},
};

const TOGGLE_KEY: KeyCode = KeyCode::KeyE;
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
const SLOT_CORNER: f32 = 6.0;
const SLOT_COLOR: Color = Color::srgba(0.07, 0.05, 0.04, 0.72);
const SLOT_EDGE: Color = Color::srgba(0.96, 0.92, 0.83, 0.14);
const HOVERED_EDGE: Color = Color::srgba(0.96, 0.92, 0.83, 0.5);
const HELD_EDGE: Color = ACCENT_COLOR;
/// The slot a picked-up stack came from, while it follows the cursor.
const PICKED_FROM_COLOR: Color = Color::srgba(0.07, 0.05, 0.04, 0.35);
const SWATCH_SIZE: f32 = 14.0;
const SILVER: Color = Color::srgb(0.78, 0.8, 0.85);
const GOLD: Color = Color::srgb(0.98, 0.78, 0.25);
const FRESH: Color = Color::srgb(0.45, 0.78, 0.35);
const STALE: Color = Color::srgb(0.85, 0.35, 0.2);
/// Distance of the hotbar from the bottom of the screen.
pub(crate) const HOTBAR_BOTTOM: f32 = 16.0;
/// How long the held item's name shows after it changes, the last part of
/// it fading out.
const HELD_NAME_TIME: Duration = Duration::from_millis(2_000);
const HELD_NAME_FADE: Duration = Duration::from_millis(500);
/// Where the cursor's stack and the description sit from the cursor.
const CURSOR_OFFSET: Vec2 = Vec2::new(-SLOT_SIZE / 2.0, -SLOT_SIZE / 2.0);
const TOOLTIP_OFFSET: Vec2 = Vec2::new(18.0, 18.0);
const LABEL_SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.7);

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldSlot>()
            .init_resource::<PickedSlot>()
            .add_systems(Startup, (spawn_hotbar, spawn_backpack, spawn_cursor_layer))
            .add_systems(
                Update,
                (
                    choose_held_slot,
                    toggle_backpack,
                    click_slots,
                    show_slots,
                    show_held_name,
                    follow_cursor,
                    describe_hovered,
                )
                    .chain(),
            );
    }
}

/// The hotbar slot whose item the player holds and uses.
#[derive(Resource, Default)]
pub(crate) struct HeldSlot(pub usize);

/// A slot whose stack was picked up in the backpack window, waiting for
/// where to put it.
#[derive(Resource, Default)]
struct PickedSlot(Option<usize>);

/// Shows the contents of one inventory slot, through its parts.
#[derive(Component)]
struct SlotView {
    slot: usize,
    parts: SlotParts,
}

/// The pieces a stack is drawn with.
#[derive(Clone, Copy)]
struct SlotParts {
    swatch: Entity,
    quality: Entity,
    name: Entity,
    count: Entity,
    freshness: Entity,
    freshness_fill: Entity,
}

/// The hotbar along the bottom of the screen, hidden while the backpack
/// window shows its own.
#[derive(Component)]
struct Hotbar;

#[derive(Component)]
struct BackpackWindow;

/// A column beside the backpack window for panels that open with it.
#[derive(Component)]
pub(crate) struct BackpackSide;

/// The backdrop behind the backpack window; clicking it puts a picked-up
/// stack back.
#[derive(Component)]
struct Backdrop;

#[derive(Component)]
struct HeldName;

/// The picked-up stack, drawn at the cursor.
#[derive(Component)]
struct CursorStack(SlotParts);

#[derive(Component)]
struct Tooltip;

#[derive(Component)]
struct TooltipName;

#[derive(Component)]
struct TooltipLines;

fn spawn_hotbar(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Hotbar"),
            Hotbar,
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
                spawn_slot(hotbar, slot, false);
            }
        });

    commands.spawn((
        Name::new("Held item name"),
        HeldName,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(HOTBAR_BOTTOM + SLOT_SIZE + 12.0),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Text::new(""),
        TextFont::from_font_size(16.0),
        TextColor(TEXT_COLOR),
        TextLayout::justify(Justify::Center),
        TextShadow {
            offset: Vec2::splat(1.5),
            color: LABEL_SHADOW,
        },
    ));
}

fn spawn_backpack(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Backpack window"),
            BackpackWindow,
            Backdrop,
            Button,
            ui::screen(),
            BackgroundColor(BACKDROP_COLOR),
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen
                .spawn(ui::window(Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(18)),
                    row_gap: px(10),
                    ..default()
                }))
                .with_children(|window| {
                    window.spawn(ui::label("Backpack", TITLE_SIZE, TEXT_COLOR));
                    window.spawn(ui::label(
                        "Click a stack to pick it up, and a slot to put it down.",
                        TEXT_SIZE,
                        MUTED_TEXT_COLOR,
                    ));
                    spawn_grid(window, HOTBAR_SLOTS..SLOTS);
                    window.spawn(ui::label("Hotbar", TEXT_SIZE, MUTED_TEXT_COLOR));
                    spawn_grid(window, 0..HOTBAR_SLOTS);
                    // Beside the window rather than in a row with it, so the
                    // window stays in the middle of the screen.
                    window.spawn((
                        BackpackSide,
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(100),
                            top: px(0),
                            margin: UiRect::left(px(12)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(12),
                            ..default()
                        },
                    ));
                });
        });
}

fn spawn_cursor_layer(mut commands: Commands) {
    let parts = commands
        .spawn((
            Name::new("Cursor stack"),
            Node {
                position_type: PositionType::Absolute,
                width: px(SLOT_SIZE),
                height: px(SLOT_SIZE),
                border_radius: BorderRadius::all(px(SLOT_CORNER)),
                ..default()
            },
            BackgroundColor(SLOT_COLOR),
            GlobalZIndex(2),
            Visibility::Hidden,
        ))
        .id();
    let slot_parts = spawn_slot_body(&mut commands, parts);
    commands.entity(parts).insert(CursorStack(slot_parts));

    commands.spawn((
        Name::new("Tooltip"),
        Tooltip,
        ui::window(Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(px(10), px(8)),
            row_gap: px(4),
            max_width: px(300),
            ..default()
        }),
        GlobalZIndex(3),
        Visibility::Hidden,
        children![
            (TooltipName, ui::label("", 16.0, TEXT_COLOR)),
            (TooltipLines, ui::label("", 13.0, MUTED_TEXT_COLOR)),
        ],
    ));
}

/// Spawns clickable slots in rows as long as the hotbar.
fn spawn_grid(parent: &mut ChildSpawnerCommands, slots: Range<usize>) {
    let columns = u16::try_from(HOTBAR_SLOTS).expect("a small constant");
    parent
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(columns, SLOT_SIZE),
            row_gap: px(SLOT_GAP),
            column_gap: px(SLOT_GAP),
            ..default()
        })
        .with_children(|grid| {
            for slot in slots {
                spawn_slot(grid, slot, true);
            }
        });
}

/// Spawns a slot; slots in the backpack window can be clicked.
fn spawn_slot(parent: &mut ChildSpawnerCommands, slot: usize, clickable: bool) {
    let mut view = parent.spawn((
        Node {
            width: px(SLOT_SIZE),
            height: px(SLOT_SIZE),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(SLOT_CORNER)),
            // Names too long for the slot are cut at its edge rather than
            // spilling into the next one.
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(SLOT_COLOR),
        BorderColor::all(SLOT_EDGE),
    ));
    if clickable {
        view.insert(Button);
    }
    let id = view.id();
    let parts = spawn_slot_body(view.commands_mut(), id);
    view.insert(SlotView { slot, parts });
}

/// Spawns the pieces a stack is drawn with inside `slot`.
fn spawn_slot_body(commands: &mut Commands, slot: Entity) -> SlotParts {
    let corner =
        |left: Option<f32>, top: Option<f32>, right: Option<f32>, bottom: Option<f32>| Node {
            position_type: PositionType::Absolute,
            left: left.map_or(Val::Auto, px),
            top: top.map_or(Val::Auto, px),
            right: right.map_or(Val::Auto, px),
            bottom: bottom.map_or(Val::Auto, px),
            ..default()
        };
    let swatch = commands
        .spawn((
            Node {
                width: px(SWATCH_SIZE),
                height: px(SWATCH_SIZE),
                border_radius: BorderRadius::all(px(3)),
                ..corner(Some(5.0), Some(5.0), None, None)
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    let quality = commands
        .spawn((
            Node {
                width: px(9),
                height: px(9),
                border_radius: BorderRadius::MAX,
                ..corner(None, Some(6.0), Some(6.0), None)
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    let name = commands
        .spawn((
            corner(Some(5.0), Some(22.0), Some(3.0), None),
            Text::new(""),
            TextFont::from_font_size(11.0),
            TextColor(TEXT_COLOR),
        ))
        .id();
    let count = commands
        .spawn((
            corner(None, None, Some(5.0), Some(2.0)),
            Text::new(""),
            TextFont::from_font_size(14.0),
            TextColor(TEXT_COLOR),
            TextShadow {
                offset: Vec2::splat(1.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.8),
            },
        ))
        .id();
    let freshness_fill = commands
        .spawn((
            Node {
                height: percent(100),
                border_radius: BorderRadius::all(px(2)),
                ..default()
            },
            BackgroundColor(FRESH),
        ))
        .id();
    let freshness = commands
        .spawn((
            Node {
                width: px(28),
                height: px(4),
                border_radius: BorderRadius::all(px(2)),
                ..corner(Some(6.0), None, None, Some(7.0))
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            Visibility::Hidden,
        ))
        .add_child(freshness_fill)
        .id();
    commands
        .entity(slot)
        .add_children(&[swatch, quality, name, count, freshness]);
    SlotParts {
        swatch,
        quality,
        name,
        count,
        freshness,
        freshness_fill,
    }
}

fn choose_held_slot(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    panel: Res<OpenPanel>,
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
    if !panel.is_open() && notches != 0.0 {
        // Scrolling down moves right along the hotbar, as in most games.
        let step = if notches < 0.0 { 1 } else { HOTBAR_SLOTS - 1 };
        held.0 = (held.0 + step) % HOTBAR_SLOTS;
    }
}

/// `E` opens the backpack window, or closes it; it also takes the place of
/// any other open window but the game menu.
fn toggle_backpack(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<OpenPanel>,
    mut picked: ResMut<PickedSlot>,
    mut window: Single<&mut Visibility, (With<BackpackWindow>, Without<Hotbar>)>,
    mut hotbar: Single<&mut Visibility, With<Hotbar>>,
) {
    if keys.just_pressed(TOGGLE_KEY) {
        match *panel {
            OpenPanel::Backpack => *panel = OpenPanel::None,
            OpenPanel::Menu | OpenPanel::Options => {}
            OpenPanel::None | OpenPanel::Shop(_) => *panel = OpenPanel::Backpack,
        }
    }
    if panel.is_changed() {
        picked.0 = None;
        let open = *panel == OpenPanel::Backpack;
        window.set_if_neq(ui::visible_if(open));
        hotbar.set_if_neq(ui::visible_if(!open));
    }
}

fn click_slots(
    panel: Res<OpenPanel>,
    mut picked: ResMut<PickedSlot>,
    clicked: Query<
        (&Interaction, Option<&SlotView>),
        (Changed<Interaction>, Or<(With<SlotView>, With<Backdrop>)>),
    >,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut sender: Query<&mut MessageSender<MoveItem>, With<Client>>,
) {
    if *panel != OpenPanel::Backpack {
        return;
    }
    let Ok(belongings) = player.single() else {
        return;
    };
    for (interaction, view) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Clicking beside the window puts the stack back.
        let Some(view) = view else {
            picked.0 = None;
            continue;
        };
        match picked.0.take() {
            None => picked.0 = belongings.0.slot(view.slot).map(|_| view.slot),
            Some(from) if from == view.slot => {}
            Some(from) => {
                if let Ok(mut sender) = sender.single_mut() {
                    sender.send::<ActionChannel>(MoveItem {
                        from: u8::try_from(from).expect("slot indices fit in u8"),
                        to: u8::try_from(view.slot).expect("slot indices fit in u8"),
                    });
                }
            }
        }
    }
}

/// Everything a stack is drawn with, worked out once per slot.
struct StackLook {
    tint: Color,
    quality: Color,
    name: String,
    count: String,
    /// The part of its shelf life a perishable has left.
    freshness: Option<f32>,
}

impl StackLook {
    fn of(content: &Content, stack: Option<&Stack>, today: Option<u32>) -> Self {
        let Some(stack) = stack else {
            return Self {
                tint: Color::NONE,
                quality: Color::NONE,
                name: String::new(),
                count: String::new(),
                freshness: None,
            };
        };
        let definition = content.item(stack.item);
        let freshness = match (stack.spoils_on, definition.shelf_life, today) {
            (Some(spoils_on), Some(shelf_life), Some(today)) => {
                let left =
                    f32::from(u16::try_from(spoils_on.saturating_sub(today)).unwrap_or(u16::MAX));
                Some((left / f32::from(shelf_life.max(1))).clamp(0.0, 1.0))
            }
            _ => None,
        };
        Self {
            tint: item_color(content, stack.item),
            quality: match stack.quality {
                Quality::Normal => Color::NONE,
                Quality::Silver => SILVER,
                Quality::Gold => GOLD,
            },
            name: definition.name.clone(),
            count: if stack.count > 1 {
                stack.count.to_string()
            } else {
                String::new()
            },
            freshness,
        }
    }
}

fn show_slots(
    content: Res<Content>,
    clock: Res<LocalClock>,
    held: Res<HeldSlot>,
    picked: Res<PickedSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    views: Query<(Entity, &SlotView, &Interaction)>,
    hotbar_views: Query<(Entity, &SlotView), Without<Interaction>>,
    cursor: Single<(&CursorStack, &mut Visibility)>,
    mut parts: SlotPartsParams,
) {
    let belongings = player.single().ok();
    let today = clock.time().map(WorldTime::day);
    let stack = |slot: usize| belongings.and_then(|belongings| belongings.0.slot(slot));

    let hovered = |interaction: &Interaction| *interaction != Interaction::None;
    let window_views = views
        .iter()
        .map(|(entity, view, interaction)| (entity, view, hovered(interaction)));
    let bar_views = hotbar_views
        .iter()
        .map(|(entity, view)| (entity, view, false));
    for (entity, view, hovered) in window_views.chain(bar_views) {
        let picked_here = picked.0 == Some(view.slot);
        let shown = if picked_here { None } else { stack(view.slot) };
        parts.draw(&view.parts, &StackLook::of(&content, shown, today));
        let edge = if held.0 == view.slot {
            HELD_EDGE
        } else if hovered {
            HOVERED_EDGE
        } else {
            SLOT_EDGE
        };
        parts.frame(
            entity,
            if picked_here {
                PICKED_FROM_COLOR
            } else {
                SLOT_COLOR
            },
            edge,
        );
    }

    let (cursor, mut visibility) = cursor.into_inner();
    let carried = picked.0.and_then(stack);
    visibility.set_if_neq(ui::visible_if(carried.is_some()));
    parts.draw(&cursor.0, &StackLook::of(&content, carried, today));
}

/// Access to the pieces of every drawn stack.
#[derive(bevy::ecs::system::SystemParam)]
struct SlotPartsParams<'w, 's> {
    colors: Query<'w, 's, &'static mut BackgroundColor>,
    edges: Query<'w, 's, &'static mut BorderColor>,
    texts: Query<'w, 's, &'static mut Text>,
    nodes: Query<'w, 's, &'static mut Node>,
    visibilities: Query<'w, 's, &'static mut Visibility, Without<CursorStack>>,
}

impl SlotPartsParams<'_, '_> {
    fn draw(&mut self, parts: &SlotParts, look: &StackLook) {
        self.color(parts.swatch, look.tint);
        self.color(parts.quality, look.quality);
        self.text(parts.name, &look.name);
        self.text(parts.count, &look.count);
        if let Ok(mut visibility) = self.visibilities.get_mut(parts.freshness) {
            visibility.set_if_neq(ui::visible_if(look.freshness.is_some()));
        }
        if let Some(freshness) = look.freshness {
            self.color(parts.freshness_fill, STALE.mix(&FRESH, freshness));
            if let Ok(mut node) = self.nodes.get_mut(parts.freshness_fill) {
                let width = percent(freshness * 100.0);
                if node.width != width {
                    node.width = width;
                }
            }
        }
    }

    fn frame(&mut self, slot: Entity, color: Color, edge: Color) {
        self.color(slot, color);
        if let Ok(mut border) = self.edges.get_mut(slot) {
            border.set_if_neq(BorderColor::all(edge));
        }
    }

    fn color(&mut self, entity: Entity, color: Color) {
        if let Ok(mut background) = self.colors.get_mut(entity) {
            background.set_if_neq(BackgroundColor(color));
        }
    }

    fn text(&mut self, entity: Entity, text: &str) {
        if let Ok(mut shown) = self.texts.get_mut(entity)
            && shown.0 != text
        {
            text.clone_into(&mut shown.0);
        }
    }
}

/// Names the held item above the hotbar for a moment whenever it changes.
fn show_held_name(
    time: Res<Time>,
    content: Res<Content>,
    held: Res<HeldSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut shown: Local<Option<(Option<ItemId>, Duration)>>,
    label: Single<(&mut Text, &mut TextColor, &mut TextShadow), With<HeldName>>,
) {
    let (mut text, mut color, mut shadow) = label.into_inner();
    let item = player
        .single()
        .ok()
        .and_then(|belongings| belongings.0.slot(held.0))
        .map(|stack| stack.item);
    let now = time.elapsed();
    if shown.is_none_or(|(named, _)| named != item) {
        *shown = Some((item, now));
        text.0 = item
            .map(|item| content.item(item).name.clone())
            .unwrap_or_default();
    }
    let Some((_, since)) = *shown else {
        return;
    };
    let left = HELD_NAME_TIME.saturating_sub(now.saturating_sub(since));
    let opacity = (left.as_secs_f32() / HELD_NAME_FADE.as_secs_f32()).min(1.0);
    color.set_if_neq(TextColor(TEXT_COLOR.with_alpha(opacity)));
    let faded = LABEL_SHADOW.with_alpha(LABEL_SHADOW.alpha() * opacity);
    if shadow.color != faded {
        shadow.color = faded;
    }
}

fn follow_cursor(
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Query<&mut Node, (With<CursorStack>, Without<Tooltip>)>,
    mut tooltip: Query<&mut Node, With<Tooltip>>,
) {
    let Some(at) = window.cursor_position() else {
        return;
    };
    let place = |node: &mut Node, offset: Vec2| {
        node.left = px(at.x + offset.x);
        node.top = px(at.y + offset.y);
    };
    for mut node in &mut cursor {
        place(&mut node, CURSOR_OFFSET);
    }
    for mut node in &mut tooltip {
        place(&mut node, TOOLTIP_OFFSET);
    }
}

/// Describes the stack under the cursor in the backpack window, unless a
/// stack is being carried.
fn describe_hovered(
    content: Res<Content>,
    clock: Res<LocalClock>,
    panel: Res<OpenPanel>,
    picked: Res<PickedSlot>,
    player: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    views: Query<(&SlotView, &Interaction)>,
    tooltip: Single<&mut Visibility, With<Tooltip>>,
    mut name: Single<(&mut Text, &mut TextColor), (With<TooltipName>, Without<TooltipLines>)>,
    mut lines: Single<&mut Text, With<TooltipLines>>,
) {
    let hovered = views
        .iter()
        .find(|(_, interaction)| **interaction == Interaction::Hovered)
        .map(|(view, _)| view.slot);
    let stack = player
        .single()
        .ok()
        .zip(hovered)
        .and_then(|(belongings, slot)| belongings.0.slot(slot).copied());
    let shown = stack.filter(|_| *panel == OpenPanel::Backpack && picked.0.is_none());
    let mut visibility = tooltip.into_inner();
    visibility.set_if_neq(ui::visible_if(shown.is_some()));
    let Some(stack) = shown else {
        return;
    };
    let definition = content.item(stack.item);
    let (text, color) = &mut *name;
    if text.0 != definition.name {
        text.0.clone_from(&definition.name);
    }
    color.set_if_neq(TextColor(match stack.quality {
        Quality::Normal => TEXT_COLOR,
        Quality::Silver => SILVER,
        Quality::Gold => GOLD,
    }));
    let described = describe(&content, &stack, clock.time().map(WorldTime::day));
    if lines.0 != described {
        lines.0 = described;
    }
}

/// Lines describing a stack: what the item is for, its quality, how fresh
/// it is and how full the stack is.
fn describe(content: &Content, stack: &Stack, today: Option<u32>) -> String {
    let definition = content.item(stack.item);
    let mut lines = vec![match &definition.kind {
        ItemKind::Tool(Tool::Shovel) => {
            "Digs the ground up, or raises it with soil you carry.".to_owned()
        }
        ItemKind::Tool(Tool::Hoe) => "Tills the ground for planting.".to_owned(),
        ItemKind::Tool(Tool::WateringCan) => "Waters tilled soil for the day.".to_owned(),
        ItemKind::Tool(Tool::Axe) => "Fells trees and cuts up logs, for wood.".to_owned(),
        ItemKind::Tool(Tool::Pickaxe) => "Breaks rocks, for stone.".to_owned(),
        ItemKind::Seed => match content.crop_grown_from(stack.item) {
            Some(crop) => {
                let crop = content.crop(crop);
                let seasons: Vec<String> = crop.seasons.iter().map(ToString::to_string).collect();
                format!(
                    "Plant in tilled soil. Grows into {} in {}.",
                    crop.name.to_lowercase(),
                    seasons.join(" and ")
                )
            }
            None => "Seeds.".to_owned(),
        },
        ItemKind::Fertilizer => "Spread on tilled soil for better harvests.".to_owned(),
        ItemKind::Food { energy } => format!("Eat to restore {energy} energy."),
        ItemKind::Terrain { .. } => "Raise the ground with it, using the shovel.".to_owned(),
        ItemKind::Goods => "Sell it in the village.".to_owned(),
    }];
    match stack.quality {
        Quality::Normal => {}
        Quality::Silver => lines.push("Silver quality".to_owned()),
        Quality::Gold => lines.push("Gold quality".to_owned()),
    }
    if let (Some(spoils_on), Some(today)) = (stack.spoils_on, today) {
        lines.push(match spoils_on.saturating_sub(today) {
            0 => "Spoils tonight".to_owned(),
            1 => "Fresh for 1 more day".to_owned(),
            days => format!("Fresh for {days} more days"),
        });
    }
    if definition.max_stack > 1 {
        lines.push(format!(
            "{} of {} in this stack",
            stack.count, definition.max_stack
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use messoria_shared::content::load_content;

    #[test]
    fn perishables_say_how_long_they_keep() {
        let content = Content(load_content().expect("the shipped content is valid"));
        let berries = content.id("wild_berries").expect("berries exist");
        let stack = Stack {
            item: berries,
            quality: Quality::Silver,
            count: 3,
            spoils_on: Some(12),
        };
        let described = describe(&content, &stack, Some(10));
        assert!(described.contains("Silver quality"), "{described}");
        assert!(described.contains("Fresh for 2 more days"), "{described}");
        assert!(described.contains("3 of "), "{described}");
        assert!(describe(&content, &stack, Some(12)).contains("Spoils tonight"));
    }
}
