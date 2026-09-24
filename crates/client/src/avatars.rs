//! How characters look: an animated body, acting out what the character does,
//! with what it holds in its hand.
//!
//! Characters are simulated as a `Position` and a `Heading`; this module gives
//! them a body and keeps its `Transform` in sync, after lightyear has smoothed
//! those values for the current frame. Each player is drawn with one of the
//! character models, picked from who they are. Bodies walk, run, jump and
//! fall as their character moves, and swing, work or bend down when their
//! player does something, as the server reports it. Shopkeepers use the same
//! bodies.

use std::{
    collections::HashMap,
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher},
    time::Duration,
};

use bevy::{
    gltf::Gltf, prelude::*, transform::TransformSystems, world_serialization::WorldInstanceReady,
};
use lightyear::{
    frame_interpolation::FrameInterpolationSystems,
    prelude::{client::Remote, input::native::InputMarker, *},
};
use messoria_content::{Animations, Catalog, ItemId};
use messoria_shared::{
    content::Content,
    movement::{SPRINT_FACTOR, WALK_SPEED},
    protocol::{Belongings, Happened, Heading, Holding, PlayerId, PlayerInput, Position, Velocity},
};

use crate::{art::Models, feedback::Witnessed, inventory::HeldSlot};

/// How long a character acts something out before going back to moving.
const ACTING: Duration = Duration::from_millis(700);
/// How long one animation blends into the next.
const BLEND: Duration = Duration::from_millis(200);
/// Horizontal speeds above which a body walks, and runs.
const WALKING: f32 = 0.4;
const RUNNING: f32 = WALK_SPEED * (1.0 + SPRINT_FACTOR) / 2.0;
/// Vertical speeds beyond which a body is jumping, or falling.
const RISING: f32 = 1.5;
const FALLING: f32 = -3.0;
/// How far from a stall a trade makes its keeper greet the customer.
const KEEPER_REACH: f32 = 4.0;

pub(crate) struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rigs>()
            .add_observer(attach_avatar)
            .add_observer(smooth_between_ticks)
            .add_observer(rig_body)
            .add_systems(Update, (act_out, animate, hold_items).chain())
            .add_systems(
                PostUpdate,
                place_avatars
                    .in_set(AvatarSystems)
                    .after(FrameInterpolationSystems::Interpolate)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Runs once avatar transforms reflect this frame's character state.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AvatarSystems;

/// A body being spawned for its parent, drawn with `model`.
#[derive(Component)]
struct Body {
    model: String,
}

/// A character or shopkeeper whose body is ready to animate: the entity
/// playing its animations, the one things are held in, and its animations.
#[derive(Component, Clone, Copy)]
struct Rigged {
    player: Entity,
    hand: Entity,
    clips: Clips,
}

/// What a body is playing, and what it acts out until when.
#[derive(Component, Default)]
pub(crate) struct Pose {
    playing: Option<AnimationNodeIndex>,
    acting: Option<(AnimationNodeIndex, Duration)>,
}

impl Pose {
    /// Acts `action` out until `until`, from its start even if it is already
    /// playing, so that each strike of a tool shows.
    fn act(&mut self, action: AnimationNodeIndex, until: Duration) {
        self.acting = Some((action, until));
        self.playing = None;
    }
}

/// A shopkeeper, who greets whoever trades at the stall at `stall`.
#[derive(Component)]
pub(crate) struct Keeper {
    pub stall: Vec3,
}

/// The model held in a body's hand, and the item it shows.
#[derive(Component)]
struct HeldModel {
    item: Option<ItemId>,
    model: Option<Entity>,
}

/// Every model's animations, gathered once per model into a graph.
#[derive(Resource, Default)]
struct Rigs(HashMap<String, (Handle<AnimationGraph>, Clips)>);

/// The graph nodes of the animations characters play.
#[derive(Clone, Copy, Debug)]
struct Clips {
    idle: AnimationNodeIndex,
    walk: AnimationNodeIndex,
    run: AnimationNodeIndex,
    jump: AnimationNodeIndex,
    fall: AnimationNodeIndex,
    swing: AnimationNodeIndex,
    work: AnimationNodeIndex,
    pick: AnimationNodeIndex,
    greet: AnimationNodeIndex,
}

impl Clips {
    /// Gathers the animations `names` names from `gltf` into a new graph.
    fn gather(names: &Animations, gltf: &Gltf) -> Option<(AnimationGraph, Self)> {
        let clips = names
            .all()
            .map(|name| gltf.named_animations.get(name).cloned());
        let clips: Vec<Handle<AnimationClip>> = clips.into_iter().collect::<Option<_>>()?;
        let (graph, nodes) = AnimationGraph::from_clips(clips);
        let [idle, walk, run, jump, fall, swing, work, pick, greet] =
            <[AnimationNodeIndex; 9]>::try_from(nodes).ok()?;
        Some((
            graph,
            Self {
                idle,
                walk,
                run,
                jump,
                fall,
                swing,
                work,
                pick,
                greet,
            },
        ))
    }

    /// What a character acts out when it did `what`.
    fn acting_out(&self, what: Happened) -> AnimationNodeIndex {
        match what {
            Happened::Dug(_) | Happened::Raised | Happened::Tilled | Happened::Struck(_) => {
                self.swing
            }
            Happened::Planted | Happened::Fertilized | Happened::Harvested => self.pick,
            Happened::Watered | Happened::Traded | Happened::Built | Happened::Ate => self.work,
        }
    }
}

/// Spawns a body drawn with `model` under `owner`, which also gets a pose.
pub(crate) fn spawn_body(
    commands: &mut Commands,
    content: &Catalog,
    models: &Models,
    owner: Entity,
    model: &str,
) {
    let Some(scene) = models.scene(model) else {
        return;
    };
    let scale = content.characters().scale;
    // The models face +z; characters face -z.
    let body = commands
        .spawn((
            Body {
                model: model.to_owned(),
            },
            scene,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_scale(Vec3::splat(scale)),
        ))
        .id();
    commands
        .entity(owner)
        .insert((Pose::default(), Visibility::default()))
        .add_child(body);
}

fn attach_avatar(
    trigger: On<Add, PlayerId>,
    players: Query<&PlayerId>,
    content: Res<Content>,
    models: Res<Models>,
    mut commands: Commands,
) {
    let Ok(&PlayerId(peer)) = players.get(trigger.entity) else {
        return;
    };
    let choices = &content.characters().models;
    let hash = BuildHasherDefault::<DefaultHasher>::default().hash_one(peer.to_bits());
    let model = &choices[usize::try_from(hash % choices.len() as u64).unwrap_or_default()];
    commands.entity(trigger.entity).insert(Transform::default());
    spawn_body(&mut commands, &content, &models, trigger.entity, model);
}

/// Once a body's model is in the world, finds what plays its animations and
/// its hand, and gives it its animations.
fn rig_body(
    trigger: On<WorldInstanceReady>,
    bodies: Query<(&Body, &ChildOf)>,
    content: Res<Content>,
    models: Res<Models>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut rigs: ResMut<Rigs>,
    descendants: Query<&Children>,
    names: Query<&Name>,
    animation_players: Query<(), With<AnimationPlayer>>,
    mut commands: Commands,
) {
    let Ok((body, owner)) = bodies.get(trigger.entity) else {
        return;
    };
    let characters = content.characters();
    let rig = rigs.0.get(&body.model).cloned().or_else(|| {
        let gltf = models.file(&body.model).and_then(|file| gltfs.get(file))?;
        let (graph, clips) = Clips::gather(&characters.animations, gltf)?;
        let rig = (graphs.add(graph), clips);
        rigs.0.insert(body.model.clone(), rig.clone());
        Some(rig)
    });
    let Some((graph, clips)) = rig else {
        warn!("{} lacks the animations characters play", body.model);
        return;
    };
    let mut player = None;
    let mut hand = None;
    for entity in descendants.iter_descendants(trigger.entity) {
        if animation_players.contains(entity) {
            player = Some(entity);
        }
        if names
            .get(entity)
            .is_ok_and(|name| name.as_str() == characters.hand)
        {
            hand = Some(entity);
        }
    }
    let (Some(player), Some(hand)) = (player, hand) else {
        warn!("{} has no animation player or hand", body.model);
        return;
    };
    commands
        .entity(player)
        .insert((AnimationGraphHandle(graph), AnimationTransitions::new()));
    commands.entity(owner.parent()).insert(Rigged {
        player,
        hand,
        clips,
    });
}

/// Characters simulated in this app (all of them on a host, the predicted one
/// on a client) change once per tick; blending between ticks keeps them smooth
/// at any frame rate. Interpolated remote characters are already smoothed
/// between snapshots.
fn smooth_between_ticks(
    trigger: On<Add, (PlayerId, Predicted)>,
    players: Query<(Has<Remote>, Has<Predicted>), With<PlayerId>>,
    mut commands: Commands,
) {
    if let Ok((remote, predicted)) = players.get(trigger.entity)
        && (predicted || !remote)
    {
        commands.entity(trigger.entity).insert(FrameInterpolate);
    }
}

/// Characters act out what the server reports they did, and shopkeepers
/// greet whoever trades with them.
fn act_out(
    time: Res<Time>,
    mut witnessed: MessageReader<Witnessed>,
    mut characters: Query<(&PlayerId, &Rigged, &mut Pose)>,
    mut keepers: Query<(&Keeper, &Rigged, &mut Pose), Without<PlayerId>>,
) {
    let now = time.elapsed();
    for Witnessed(happening) in witnessed.read() {
        if let Some(by) = happening.by
            && let Some((_, rig, mut pose)) = characters.iter_mut().find(|(id, ..)| id.0 == by)
        {
            pose.act(rig.clips.acting_out(happening.what), now + ACTING);
        }
        if happening.what == Happened::Traded {
            for (keeper, rig, mut pose) in &mut keepers {
                if keeper.stall.distance(happening.at) < KEEPER_REACH {
                    pose.act(rig.clips.greet, now + ACTING * 2);
                }
            }
        }
    }
}

/// Plays, on each body, what its character is doing: acting something out,
/// or else moving as fast as it moves.
fn animate(
    time: Res<Time>,
    mut bodies: Query<(&Rigged, &mut Pose, Option<&Velocity>)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let now = time.elapsed();
    for (rig, mut pose, velocity) in &mut bodies {
        if pose.acting.is_some_and(|(_, until)| now >= until) {
            pose.acting = None;
        }
        let velocity = velocity.map_or(Vec3::ZERO, |velocity| velocity.0);
        let speed = velocity.xz().length();
        let (wanted, looping) = match pose.acting {
            Some((action, _)) => (action, false),
            None if velocity.y > RISING => (rig.clips.jump, false),
            None if velocity.y < FALLING => (rig.clips.fall, true),
            None if speed > RUNNING => (rig.clips.run, true),
            None if speed > WALKING => (rig.clips.walk, true),
            None => (rig.clips.idle, true),
        };
        if pose.playing == Some(wanted) {
            continue;
        }
        let Ok((mut player, mut transitions)) = players.get_mut(rig.player) else {
            continue;
        };
        let playing = transitions.play(&mut player, wanted, BLEND);
        if looping {
            playing.repeat();
        } else {
            playing.replay();
        }
        pose.playing = Some(wanted);
    }
}

/// Puts what each character holds in its hand: for the local player, the
/// held slot's item at once; for everyone else, what the server says they
/// hold.
fn hold_items(
    content: Res<Content>,
    models: Res<Models>,
    held: Res<HeldSlot>,
    local: Query<&Belongings, With<InputMarker<PlayerInput>>>,
    mut holders: Query<(
        Entity,
        &Rigged,
        Option<&Holding>,
        Has<InputMarker<PlayerInput>>,
        Option<&mut HeldModel>,
    )>,
    mut commands: Commands,
) {
    let grip = content.characters().grip;
    for (entity, rig, holding, is_local, shown) in &mut holders {
        let item = if is_local {
            local
                .single()
                .ok()
                .and_then(|belongings| belongings.0.slot(held.0))
                .map(|stack| stack.item)
        } else {
            holding.and_then(|holding| holding.0)
        };
        if shown.as_ref().is_some_and(|shown| shown.item == item) {
            continue;
        }
        if let Some(old) = shown.as_ref().and_then(|shown| shown.model) {
            commands.entity(old).despawn();
        }
        let scene = item
            .and_then(|item| content.item(item).model.as_deref())
            .and_then(|model| models.scene(model));
        let model = scene.map(|scene| {
            let (x, y, z) = grip.at;
            let (tx, ty, tz) = grip.turn;
            commands
                .spawn((
                    scene,
                    Transform::from_xyz(x, y, z)
                        .with_rotation(Quat::from_euler(
                            EulerRot::XYZ,
                            tx.to_radians(),
                            ty.to_radians(),
                            tz.to_radians(),
                        ))
                        .with_scale(Vec3::splat(grip.scale)),
                    ChildOf(rig.hand),
                ))
                .id()
        });
        let held_model = HeldModel { item, model };
        match shown {
            Some(mut shown) => *shown = held_model,
            None => {
                commands.entity(entity).insert(held_model);
            }
        }
    }
}

fn place_avatars(mut avatars: Query<(&Position, &Heading, &mut Transform), With<PlayerId>>) {
    for (position, heading, mut transform) in &mut avatars {
        transform.translation = position.0;
        transform.rotation = Quat::from_rotation_y(heading.0);
    }
}
