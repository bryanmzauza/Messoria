//! Gathering scenery: felling trees, breaking rocks and picking berries, and
//! what was gathered growing back.
//!
//! Tools gather by being used, which the inventory hands over as a
//! [`GatherUse`]; picking by hand arrives as a `Gather` request. A prop
//! gathered with a tool takes a number of strikes, each spending energy, and
//! gives its yield with the last. Once gathered it stands as what its kind
//! leaves behind, and grows back at a dawn if its kind does. What was
//! gathered is kept by the scenery, which the save reads.

use std::collections::HashMap;

use bevy::{ecs::message::Message, prelude::*};
use lightyear::prelude::*;
use messoria_content::{Quality, Tool};
use messoria_shared::{
    content::Content,
    energy::Energy,
    movement::EYE_HEIGHT,
    protocol::{Asleep, Belongings, Gather, Happened, Notice, Position, WorldClock},
    scenery::{Scenery, SceneryChanged},
    tools,
    valley::PropKey,
};

use crate::{
    Beginning, WorldStart,
    day_cycle::{ClockSystems, DayStarted},
    feedback::{Show, Tell},
    inventory::ItemUseSystems,
    players::ControlledCharacter,
    terrain::Restoring,
};

/// How far from a prop's footprint a target may be and still be on it:
/// clients aim at the side of its trunk, which is about this far out.
const TARGET_SLACK: f32 = 0.6;

pub(crate) struct GatheringPlugin;

impl Plugin for GatheringPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GatherUse>()
            .init_resource::<Strikes>()
            .add_systems(Startup, restore_gathered.in_set(Restoring))
            .add_systems(
                PreUpdate,
                (
                    pick_by_hand.after(MessageSystems::Receive),
                    gather.after(ItemUseSystems),
                )
                    .chain(),
            )
            .add_systems(FixedUpdate, grow_back.after(ClockSystems));
    }
}

/// A character working on the prop at `target`, with `tool` or by hand.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct GatherUse {
    pub character: Entity,
    pub target: Vec3,
    pub tool: Option<Tool>,
}

/// Strikes props took toward being gathered.
#[derive(Resource, Default)]
struct Strikes(HashMap<PropKey, u8>);

fn restore_gathered(beginning: Res<Beginning>, mut scenery: ResMut<Scenery>) {
    if let WorldStart::Resume(saved) = &beginning.0 {
        scenery.remember_gathered(saved.world.gathered.iter().map(|gathered| {
            let key = PropKey {
                kind: gathered.kind,
                cell: gathered.cell,
            };
            (key, gathered.day)
        }));
    }
}

fn pick_by_hand(
    mut clients: Query<(&mut MessageReceiver<Gather>, &ControlledCharacter)>,
    mut uses: MessageWriter<GatherUse>,
) {
    for (mut requests, character) in &mut clients {
        uses.write_batch(requests.receive().map(|Gather { target }| GatherUse {
            character: character.0,
            target,
            tool: None,
        }));
    }
}

fn gather(
    content: Res<Content>,
    clock: Single<&WorldClock>,
    mut scenery: ResMut<Scenery>,
    mut strikes: ResMut<Strikes>,
    mut uses: MessageReader<GatherUse>,
    characters: Query<&Position>,
    mut workers: Query<(&mut Energy, &mut Belongings), Without<Asleep>>,
    mut tell: MessageWriter<Tell>,
    mut show: MessageWriter<Show>,
    mut changed: MessageWriter<SceneryChanged>,
) {
    for work in uses.read() {
        let (Ok(feet), Ok((mut energy, mut belongings))) = (
            characters.get(work.character),
            workers.get_mut(work.character),
        ) else {
            continue;
        };
        if !work.target.is_finite() || !tools::in_reach(feet.0 + Vec3::Y * EYE_HEIGHT, work.target)
        {
            continue;
        }
        let Some(key) = scenery.prop_at(work.target, TARGET_SLACK) else {
            continue;
        };
        let definition = content.prop(key.kind);
        let Some(gathering) = &definition.gather else {
            continue;
        };
        let refuse = |notice| Tell {
            character: work.character,
            notice,
        };
        if scenery.gathered(key).is_some() {
            tell.write(refuse(Notice::NothingToGather));
            continue;
        }
        match (gathering.tool, work.tool) {
            (Some(needed), held) if held != Some(needed) => {
                tell.write(refuse(Notice::NeedsTool(needed)));
                continue;
            }
            (None, Some(_)) => {
                tell.write(refuse(Notice::PickByHand));
                continue;
            }
            _ => {}
        }
        if work.tool.is_some() && energy.current() < tools::GATHERING_ENERGY {
            tell.write(refuse(Notice::NotEnoughEnergy));
            continue;
        }

        let struck = strikes.0.get(&key).copied().unwrap_or(0) + 1;
        let happened = work.tool.map_or(Happened::Harvested, Happened::Struck);
        if struck < gathering.strikes {
            energy.try_spend(tools::GATHERING_ENERGY);
            strikes.0.insert(key, struck);
            show.write(Show::at(happened, work.target, work.character));
            continue;
        }

        // The last strike: everything it yields must fit, or it is not made.
        let today = clock.0.day();
        let mut carried = belongings.0.clone();
        let fits = gathering
            .yields
            .iter()
            .all(|&(item, count)| carried.add(&content, item, Quality::Normal, count, today) == 0);
        if !fits {
            tell.write(refuse(Notice::NoRoom));
            continue;
        }
        belongings.0 = carried;
        if work.tool.is_some() {
            energy.try_spend(tools::GATHERING_ENERGY);
        }
        strikes.0.remove(&key);
        scenery.gather(key, today);
        changed.write(SceneryChanged(key));
        show.write(Show::at(happened, work.target, work.character));
    }
}

/// At dawn, what was gathered long enough ago stands as it did before.
fn grow_back(
    content: Res<Content>,
    mut days: MessageReader<DayStarted>,
    clock: Single<&WorldClock>,
    mut scenery: ResMut<Scenery>,
    mut changed: MessageWriter<SceneryChanged>,
) {
    if days.read().count() == 0 {
        return;
    }
    let today = clock.0.day();
    let grown_back: Vec<PropKey> = scenery
        .gathered_props()
        .filter(|&(key, day)| {
            content
                .prop(key.kind)
                .gather
                .as_ref()
                .and_then(|gathering| gathering.regrows_after)
                .is_some_and(|days| today >= day + u32::from(days))
        })
        .map(|(key, _)| key)
        .collect();
    for key in grown_back {
        scenery.regrow(key);
        changed.write(SceneryChanged(key));
    }
}
