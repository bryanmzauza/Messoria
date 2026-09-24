//! What the crosshair is on, named under it: a crop and how it is doing, a
//! field, a stall, or a piece of scenery and how to gather it.

use bevy::prelude::*;
use messoria_calendar::WorldTime;
use messoria_content::{CropDef, Tool};
use messoria_farming::Planting;
use messoria_shared::{
    content::Content,
    fields::tile_at,
    protocol::{Crop, Fertilized, Field, Gathered, Prop, Shopfront, Watered},
};

use crate::{
    actions::{Aim, AimSystems, INTERACT_KEY_NAME},
    clock::LocalClock,
    ui::{MUTED_TEXT_COLOR, TEXT_COLOR},
};

/// How far below the middle of the screen the description sits.
const BELOW_CROSSHAIR: f32 = 24.0;
const NAME_SIZE: f32 = 16.0;
const DETAIL_SIZE: f32 = 13.0;
const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.7);
const SHADOW_OFFSET: f32 = 1.5;

pub(crate) struct TargetPlugin;

impl Plugin for TargetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_description)
            .add_systems(Update, describe_target.after(AimSystems));
    }
}

#[derive(Component)]
struct TargetName;

#[derive(Component)]
struct TargetDetail;

/// A name, and a line about its state.
#[derive(Default, PartialEq, Eq)]
struct Description {
    name: String,
    detail: String,
}

fn spawn_description(mut commands: Commands) {
    commands.spawn((
        Name::new("Target"),
        Node {
            position_type: PositionType::Absolute,
            top: percent(50),
            width: percent(100),
            margin: UiRect::top(px(BELOW_CROSSHAIR)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        },
        children![
            (
                TargetName,
                Text::new(""),
                TextFont::from_font_size(NAME_SIZE),
                TextColor(TEXT_COLOR),
                TextShadow {
                    offset: Vec2::splat(SHADOW_OFFSET),
                    color: SHADOW,
                },
            ),
            (
                TargetDetail,
                Text::new(""),
                TextFont::from_font_size(DETAIL_SIZE),
                TextColor(MUTED_TEXT_COLOR.with_alpha(0.85)),
                TextShadow {
                    offset: Vec2::splat(SHADOW_OFFSET),
                    color: SHADOW,
                },
            ),
        ],
    ));
}

fn describe_target(
    aim: Res<Aim>,
    content: Res<Content>,
    clock: Res<LocalClock>,
    fields: Query<(&Field, Option<&Crop>, Has<Watered>, Has<Fertilized>)>,
    props: Query<(&Prop, Option<&Gathered>)>,
    stalls: Query<&Shopfront>,
    mut name: Single<&mut Text, (With<TargetName>, Without<TargetDetail>)>,
    mut detail: Single<&mut Text, With<TargetDetail>>,
) {
    let today = clock.time().map(WorldTime::day);
    let described = if let Some((thing, _)) = aim.thing {
        if let Ok(stall) = stalls.get(thing) {
            Some(Description {
                name: format!("{}'s stall", content.shop(stall.shop).name),
                detail: format!("Stand close and press {INTERACT_KEY_NAME} to trade"),
            })
        } else {
            props
                .get(thing)
                .ok()
                .map(|(prop, gathered)| describe_prop(&content, prop, gathered.copied(), today))
        }
    } else if let Some(hit) = aim.ground {
        let tile = tile_at(hit.point);
        fields.iter().find(|(field, ..)| field.tile == tile).map(
            |(_, crop, watered, fertilized)| {
                describe_field(&content, clock.time(), crop, watered, fertilized)
            },
        )
    } else {
        None
    }
    .unwrap_or_default();

    if name.0 != described.name {
        name.0 = described.name;
    }
    if detail.0 != described.detail {
        detail.0 = described.detail;
    }
}

/// A prop's name, and how to gather it or when it grows back.
fn describe_prop(
    content: &Content,
    prop: &Prop,
    gathered: Option<Gathered>,
    today: Option<u32>,
) -> Description {
    let definition = content.prop(prop.kind);
    let detail = match (&definition.gather, gathered) {
        (None, _) => String::new(),
        (Some(gathering), Some(gathered)) => match (gathering.regrows_after, today) {
            (Some(days), Some(today)) => {
                let left = (gathered.day + u32::from(days)).saturating_sub(today);
                match left {
                    0 | 1 => "Grows back tomorrow".to_owned(),
                    left => format!("Grows back in {left} days"),
                }
            }
            _ => String::new(),
        },
        (Some(gathering), None) => match gathering.tool {
            Some(Tool::Axe) => "Chop it with an axe".to_owned(),
            Some(Tool::Pickaxe) => "Break it with a pickaxe".to_owned(),
            Some(_) => String::new(),
            None => format!("Press {INTERACT_KEY_NAME} to pick"),
        },
    };
    Description {
        name: definition.name.clone(),
        detail,
    }
}

fn describe_field(
    content: &Content,
    now: Option<WorldTime>,
    crop: Option<&Crop>,
    watered: bool,
    fertilized: bool,
) -> Description {
    let Some(Crop(planting)) = crop else {
        let mut state = vec![if watered { "Watered" } else { "Dry" }];
        if fertilized {
            state.push("fertilized");
        }
        return Description {
            name: "Tilled soil".to_owned(),
            detail: state.join(", "),
        };
    };
    let definition = content.crop(planting.crop);
    Description {
        name: definition.name.clone(),
        detail: crop_state(definition, *planting, now, watered),
    }
}

/// How a crop is doing: ripe, or how long until it is and whether it still
/// needs water today, and whether the season is about to kill it.
fn crop_state(
    definition: &CropDef,
    planting: Planting,
    now: Option<WorldTime>,
    watered: bool,
) -> String {
    let mut state = Vec::new();
    if planting.is_ripe(definition) {
        state.push(format!("Ripe, press {INTERACT_KEY_NAME} to harvest"));
    } else {
        let days = definition
            .days_to_ripen()
            .saturating_sub(planting.days_grown);
        state.push(match days {
            1 => "Ripe in 1 day of water".to_owned(),
            days => format!("Ripe in {days} days of water"),
        });
        if !watered {
            state.push("needs water today".to_owned());
        }
    }
    if let Some(now) = now
        && !definition.seasons.contains(&now.season())
    {
        state.push("out of season, it will wither".to_owned());
    }
    state.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use messoria_shared::content::load_content;

    #[test]
    fn crop_state_says_what_the_crop_needs() {
        let content = load_content().expect("the shipped content is valid");
        let turnip = content.crop_id("turnip").expect("turnips exist");
        let definition = content.crop(turnip);
        let mut planting = Planting::new(turnip);

        let spring_day = WorldTime::FIRST_DAWN;
        let days = definition.days_to_ripen();
        assert_eq!(
            crop_state(definition, planting, Some(spring_day), false),
            format!("Ripe in {days} days of water, needs water today")
        );
        assert_eq!(
            crop_state(definition, planting, Some(spring_day), true),
            format!("Ripe in {days} days of water")
        );
        planting.days_grown = days;
        assert_eq!(
            crop_state(definition, planting, Some(spring_day), false),
            format!("Ripe, press {INTERACT_KEY_NAME} to harvest")
        );
    }
}
