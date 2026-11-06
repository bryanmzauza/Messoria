//! The villagers: people with lives of their own, each following their
//! day's schedule from villagers.ron, walking the village's paths between
//! home, work, the square, the gardens and the tavern.
//!
//! Each villager is a replicated character like a player's, moved here
//! instead of by a client. Where they are going comes from the schedule
//! and the clock; how they get there is a route along the paths in
//! village.ron, found by `paths`, with a spot's `via` chain to pass a door.
//! While no player is near the village nobody watches them walk, so they
//! are moved straight to where their schedule has them, once in a while.

mod paths;

use std::collections::VecDeque;

use bevy::prelude::*;
use lightyear::prelude::*;
use messoria_content::{Activity, SpotPose, VillagerDef};
use messoria_shared::{
    content::Content,
    movement::{BODY_HEIGHT, BODY_RADIUS},
    obstacles::Obstacles,
    protocol::{
        CurrentWeather, Doing, Heading, PlayerId, Position, Velocity, Villager, WorldClock,
    },
    tick::TICK_RATE_HZ,
    valley::Valley,
    village,
};

use crate::day_cycle::ClockSystems;

/// How fast villagers walk, in meters per second: an unhurried pace.
const PACE: f32 = 1.9;
/// How close to a waypoint a villager must come to pass it.
const ARRIVED: f32 = 0.3;
/// Ticks without headway after which a villager gives up on a waypoint and
/// makes for the next.
const PATIENCE: u32 = 90;
/// How far out from a building's door its doorstep is.
const DOORSTEP: f32 = 0.8;
/// Within this distance of the village's middle a player sees villagers
/// walk; farther than that from every player, villagers are moved straight
/// to where they belong, every so many ticks.
const WATCHED: f32 = 320.0;
const UNWATCHED_TICKS: u32 = 60;

pub(crate) struct VillagersPlugin;

impl Plugin for VillagersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_villagers)
            .add_systems(FixedUpdate, live.after(ClockSystems));
    }
}

/// A villager turned toward whoever is talking to them, for a while.
#[derive(Component)]
pub(crate) struct Attending {
    pub heading: f32,
    pub ticks: u32,
}

/// Where a villager is bound, and the way there.
#[derive(Component, Default)]
struct Errand {
    activity: Option<Activity>,
    /// Which stop of a stroll comes next.
    stop: usize,
    /// The spot the villager stands at, once arrived, for the way back out
    /// of it.
    at: Option<String>,
    waypoints: VecDeque<Vec2>,
    /// Where the villager was when it last checked its headway, and for how
    /// many ticks it has been checking.
    checkpoint: Option<Vec2>,
    ticks: u32,
}

/// Where an activity takes a villager: the spot, how to hold oneself there,
/// which way to face, and the spots passed on the way in.
struct Goal {
    at: Vec2,
    face: f32,
    pose: Doing,
    via: Vec<Vec2>,
    label: Option<String>,
}

fn spawn_villagers(content: Res<Content>, valley: Res<Valley>, mut commands: Commands) {
    for (id, villager) in content.villagers() {
        let home = home_goal(&content, villager);
        let feet = Vec3::new(home.at.x, valley.height(home.at), home.at.y);
        commands.spawn((
            Name::new(villager.name.clone()),
            Villager(id),
            Position(feet),
            Velocity::default(),
            Heading(home.face),
            Doing::Indoors,
            Errand::default(),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ));
    }
}

/// The doorstep of the villager's home, facing the door.
fn home_goal(content: &Content, villager: &VillagerDef) -> Goal {
    let layout = content.village();
    let placement = layout
        .labeled(&villager.home)
        .expect("the content checks that every home is a labeled building");
    let definition = content.structure(placement.structure);
    let (x, z) = definition.door.unwrap_or((0.0, -definition.size.1 / 2.0));
    let turn = Vec2::from_angle(-placement.turn);
    let door = village::CENTER + Vec2::from(placement.at) + turn.rotate(Vec2::new(x, z));
    let outward = turn.rotate(Vec2::NEG_Y);
    Goal {
        at: door + outward * DOORSTEP,
        face: placement.turn,
        pose: Doing::Indoors,
        via: Vec::new(),
        label: None,
    }
}

/// Where the spot `label` names is, and the way in to it.
fn spot_goal(content: &Content, label: &str) -> Goal {
    let layout = content.village();
    let spot = layout
        .spot(label)
        .expect("the content checks that every spot in a schedule exists");
    let place = |at: (f32, f32)| village::CENTER + Vec2::from(at);
    Goal {
        at: place(spot.at),
        face: spot.face,
        pose: match spot.pose {
            SpotPose::Stand => Doing::Standing,
            SpotPose::Sit => Doing::Sitting,
            SpotPose::Tend => Doing::Tending,
        },
        via: spot
            .via
            .iter()
            .filter_map(|label| layout.spot(label))
            .map(|spot| place(spot.at))
            .collect(),
        label: Some(label.to_owned()),
    }
}

/// The goal of `activity`, at its `stop` if it is a stroll.
fn goal_of(content: &Content, villager: &VillagerDef, activity: &Activity, stop: usize) -> Goal {
    match activity {
        Activity::Home => home_goal(content, villager),
        Activity::At(label) => spot_goal(content, label),
        Activity::Stroll(labels) => spot_goal(content, &labels[stop % labels.len()]),
    }
}

/// The way out of the spot a villager stands at: its `via` chain backward.
fn way_out(content: &Content, label: &str) -> Vec<Vec2> {
    let layout = content.village();
    layout.spot(label).map_or_else(Vec::new, |spot| {
        spot.via
            .iter()
            .rev()
            .filter_map(|label| layout.spot(label))
            .map(|spot| village::CENTER + Vec2::from(spot.at))
            .collect()
    })
}

fn live(
    content: Res<Content>,
    valley: Res<Valley>,
    obstacles: Res<Obstacles>,
    clock: Single<(&WorldClock, &CurrentWeather)>,
    players: Query<&Position, With<PlayerId>>,
    mut villagers: Query<
        (
            Entity,
            &Villager,
            &mut Position,
            &mut Velocity,
            &mut Heading,
            &mut Doing,
            &mut Errand,
            Option<&mut Attending>,
        ),
        Without<PlayerId>,
    >,
    mut ticks: Local<u32>,
    mut commands: Commands,
) {
    let (clock, weather) = *clock;
    let watched = players
        .iter()
        .any(|feet| feet.0.xz().distance(village::CENTER) < WATCHED);
    *ticks += 1;
    if !watched && !ticks.is_multiple_of(UNWATCHED_TICKS) {
        return;
    }
    let rains = weather.0.waters_fields();
    let paths = &content.village().paths;
    #[expect(clippy::cast_precision_loss, reason = "the tick rate is small")]
    let step = PACE / TICK_RATE_HZ as f32;

    for (
        entity,
        villager,
        mut position,
        mut velocity,
        mut heading,
        mut doing,
        mut errand,
        attending,
    ) in &mut villagers
    {
        let definition = content.villager(villager.0);
        let activity = definition.activity(clock.0, rains);
        let here = position.0.xz();
        if errand.activity.as_ref() != Some(activity) {
            errand.activity = Some(activity.clone());
            errand.stop = 0;
            let goal = goal_of(&content, definition, activity, 0);
            set_out(&content, paths, &mut errand, here, &goal);
        }
        let goal = goal_of(&content, definition, activity, errand.stop);

        if !watched {
            // Nobody sees them: straight to where they belong.
            errand.waypoints.clear();
            errand.at.clone_from(&goal.label);
            arrive(&valley, &goal, &mut position, &mut heading, &mut doing);
            velocity.0 = Vec3::ZERO;
            continue;
        }

        let Some(&next) = errand.waypoints.front() else {
            if let Some(mut attending) = attending {
                // Facing whoever is talking to them, until they lose interest.
                attending.ticks = attending.ticks.saturating_sub(1);
                if attending.ticks == 0 {
                    commands.entity(entity).remove::<Attending>();
                } else if !matches!(activity, Activity::Stroll(_)) {
                    if (heading.0 - attending.heading).abs() > f32::EPSILON {
                        heading.0 = attending.heading;
                    }
                    continue;
                }
            }
            if let Activity::Stroll(stops) = activity {
                // Turn at each stop and walk on to the next.
                errand.stop = (errand.stop + 1) % stops.len();
                let onward = goal_of(&content, definition, activity, errand.stop);
                errand.at.clone_from(&goal.label);
                set_out(&content, paths, &mut errand, here, &onward);
            } else {
                errand.at.clone_from(&goal.label);
                arrive(&valley, &goal, &mut position, &mut heading, &mut doing);
                velocity.0 = Vec3::ZERO;
            }
            continue;
        };

        let toward = next - here;
        let distance = toward.length();
        let moved = if distance <= step {
            next
        } else {
            here + toward / distance * step
        };
        let feet = Vec3::new(moved.x, valley.height(moved), moved.y);
        let clear = obstacles.push_out(feet, BODY_RADIUS, BODY_HEIGHT);
        #[expect(clippy::cast_precision_loss, reason = "the tick rate is small")]
        let per_second = TICK_RATE_HZ as f32;
        velocity.0 = (clear - position.0) * per_second;
        position.0 = clear;
        if distance > 0.01 {
            heading.0 = (-toward.x).atan2(-toward.y);
        }
        doing.set_if_neq(Doing::Walking);

        if clear.xz().distance(next) < ARRIVED {
            errand.waypoints.pop_front();
            errand.ticks = 0;
            errand.checkpoint = None;
        } else {
            errand.ticks += 1;
            if errand.ticks >= PATIENCE {
                errand.ticks = 0;
                // Stuck against something: the waypoint is given up on.
                if errand
                    .checkpoint
                    .is_some_and(|checkpoint| checkpoint.distance(clear.xz()) < 0.5)
                {
                    errand.waypoints.pop_front();
                }
                errand.checkpoint = Some(clear.xz());
            }
        }
    }
}

/// Lays the villager's way from `here` to `goal` into its errand.
fn set_out(
    content: &Content,
    paths: &messoria_content::Paths,
    errand: &mut Errand,
    here: Vec2,
    goal: &Goal,
) {
    let out = errand
        .at
        .as_deref()
        .map_or_else(Vec::new, |label| way_out(content, label));
    errand.waypoints = paths::route(paths, here, &out, &goal.via, goal.at)
        .into_iter()
        .collect();
    errand.ticks = 0;
    errand.checkpoint = None;
}

/// Puts a villager at its goal, facing as the spot says.
fn arrive(
    valley: &Valley,
    goal: &Goal,
    position: &mut Position,
    heading: &mut Heading,
    doing: &mut Doing,
) {
    let feet = Vec3::new(goal.at.x, valley.height(goal.at), goal.at.y);
    if position.0.xz().distance(goal.at) > ARRIVED {
        position.0 = feet;
    }
    if (heading.0 - goal.face).abs() > f32::EPSILON {
        heading.0 = goal.face;
    }
    if *doing != goal.pose {
        *doing = goal.pose;
    }
}
