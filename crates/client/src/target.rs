//! What the crosshair is on, named under it: a crop and how it is doing, a
//! field, a stall or a piece of scenery.

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;
use messoria_calendar::WorldTime;
use messoria_content::CropDef;
use messoria_farming::Planting;
use messoria_shared::{
    content::Content,
    fields::tile_at,
    movement::EYE_HEIGHT,
    obstacles::Obstacles,
    protocol::{Crop, Fertilized, Field, PlayerInput, Position, Prop, Shopfront, Watered},
    tools,
};

use crate::{
    actions::{Aim, AimSystems},
    camera::View,
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
    view: Res<View>,
    aim: Res<Aim>,
    content: Res<Content>,
    clock: Res<LocalClock>,
    obstacles: Res<Obstacles>,
    camera: Single<&Transform, With<Camera3d>>,
    player: Query<&Position, With<InputMarker<PlayerInput>>>,
    fields: Query<(&Field, Option<&Crop>, Has<Watered>, Has<Fertilized>)>,
    props: Query<&Prop>,
    stalls: Query<&Shopfront>,
    mut name: Single<&mut Text, (With<TargetName>, Without<TargetDetail>)>,
    mut detail: Single<&mut Text, With<TargetDetail>>,
) {
    let described = match player.single() {
        Ok(feet) if view.captured => {
            let eyes = feet.0 + Vec3::Y * EYE_HEIGHT;
            let ground = aim.0.map(|hit| (hit.point, hit.distance));
            let reach = camera.translation.distance(eyes) + tools::REACH;
            let obstacle = obstacles
                .raycast(camera.translation, *camera.forward(), reach)
                .filter(|&(_, distance)| {
                    tools::in_reach(eyes, camera.translation + camera.forward() * distance)
                });
            match (obstacle, ground) {
                (Some((thing, distance)), ground)
                    if ground.is_none_or(|(_, ground)| distance < ground) =>
                {
                    describe_thing(&content, thing, &props, &stalls)
                }
                (_, Some((point, _))) => {
                    let tile = tile_at(point);
                    fields.iter().find(|(field, ..)| field.tile == tile).map(
                        |(_, crop, watered, fertilized)| {
                            describe_field(&content, clock.time(), crop, watered, fertilized)
                        },
                    )
                }
                (_, None) => None,
            }
        }
        _ => None,
    }
    .unwrap_or_default();

    if name.0 != described.name {
        name.0 = described.name;
    }
    if detail.0 != described.detail {
        detail.0 = described.detail;
    }
}

fn describe_thing(
    content: &Content,
    thing: Entity,
    props: &Query<&Prop>,
    stalls: &Query<&Shopfront>,
) -> Option<Description> {
    if let Ok(stall) = stalls.get(thing) {
        return Some(Description {
            name: format!("{}'s stall", content.shop(stall.shop).name),
            detail: "Stand close and press E to trade".to_owned(),
        });
    }
    props.get(thing).ok().map(|prop| Description {
        name: content.prop(prop.kind).name.clone(),
        detail: String::new(),
    })
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
        state.push("Ripe, press E to harvest".to_owned());
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
            "Ripe, press E to harvest"
        );
    }
}
