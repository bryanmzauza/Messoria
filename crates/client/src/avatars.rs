//! How characters look: a figure dressed for its player, posed by what the
//! character does, with what it holds in its hand.
//!
//! Characters are simulated as a `Position` and a `Heading`; this module gives
//! them a figure and keeps its `Transform` in sync, after lightyear has
//! smoothed those values for the current frame. Figures are posed here rather
//! than played from recorded animations: their legs and arms swing with the
//! distance walked, they lean into a run, spread their arms in the air, act
//! out what the server reports their player did, and lie down in bed asleep.
//! Their faces blink now and then, strain at hard work, smile at a trade or
//! a meal, start when they fall and close their eyes in sleep. Shopkeepers
//! are figures too.

use std::{f32::consts::FRAC_PI_2, time::Duration};

use bevy::{prelude::*, transform::TransformSystems};
use lightyear::{
    frame_interpolation::FrameInterpolationSystems,
    prelude::{client::Remote, input::native::InputMarker, *},
};
use messoria_content::{ItemId, Look, Purpose};
use messoria_shared::{
    content::Content,
    movement::{SPRINT_FACTOR, WALK_SPEED},
    protocol::{
        Asleep, Belongings, Happened, Heading, Holding, PlayerId, PlayerInput, Position, Structure,
        Velocity,
    },
};

use crate::{
    art::Models,
    feedback::Witnessed,
    figures::{Expression, Figure, FigureMeshes, HAND, Part, in_hand, spawn_figure},
    inventory::HeldSlot,
    skins::Tailor,
};

/// How far a stride carries a figure, in radians of leg swing per meter.
const STRIDE: f32 = 4.6;
/// How far legs swing at a walk and at a run, in radians, and how far the
/// knees bend as a leg swings through.
const WALK_SWING: f32 = 0.6;
const RUN_SWING: f32 = 1.0;
const WALK_KNEE: f32 = 0.75;
const RUN_KNEE: f32 = 1.5;
/// How much the elbows bend at a walk and at a run, in radians.
const WALK_ELBOW: f32 = 0.3;
const RUN_ELBOW: f32 = 1.5;
/// How far a running figure leans forward, in radians.
const RUN_LEAN: f32 = 0.28;
/// How high a figure bobs with each step, walking and running, in meters.
const WALK_BOB: f32 = 0.03;
const RUN_BOB: f32 = 0.06;
/// Vertical speeds beyond which a figure is jumping, or falling.
const RISING: f32 = 1.5;
const FALLING: f32 = -3.0;
/// A figure that stops falling this fast lands with a crouch, for this long.
const LANDING_SPEED: f32 = -2.0;
const LANDING: Duration = Duration::from_millis(320);
/// How far forward the arm holding something is raised, in radians, and how
/// much its elbow bends.
const HOLDING: f32 = 0.25;
const HOLDING_ELBOW: f32 = 0.45;
/// How quickly limbs turn toward their pose, per second, and faster while
/// acting something out so that strikes land on time.
const EASING: f32 = 12.0;
const ACTING_EASING: f32 = 28.0;
/// How often figures blink, at the least and at the most, in seconds, and for
/// how long.
const BLINK_EVERY: (f32, f32) = (3.2, 5.8);
const BLINK_LENGTH: f32 = 0.14;
/// How far from a stall a trade makes its keeper greet the customer.
const KEEPER_REACH: f32 = 4.0;
/// How far from a bed a sleeper lies down in it, and how high over its
/// base, and how far from its middle toward its foot the sleeper's feet
/// rest.
const BED_REACH: f32 = 2.5;
const BED_HEIGHT: f32 = 0.72;
const BED_FEET: f32 = 0.85;
/// How high over the ground a sleeper without a bed lies.
const GROUND_LYING: f32 = 0.16;

pub(crate) struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(attach_avatar)
            .add_observer(smooth_between_ticks)
            .add_systems(Update, (act_out, hold_items, pose_figures).chain())
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

/// What a figure is doing beyond moving: where it is in its stride and what
/// it acts out, since when.
#[derive(Component, Default)]
pub(crate) struct Pose {
    /// Radians of leg swing walked so far.
    stride: f32,
    acting: Option<(Act, Duration)>,
    /// Vertical speed last frame, to notice a landing.
    vertical: f32,
    /// When the figure last landed from a fall.
    landed: Option<Duration>,
    /// Keeps figures standing idle, or blinking, from all moving as one.
    offset: f32,
    expression: Expression,
}

/// Something a figure acts out once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    /// Brings a tool down from over the head, as an axe or a hoe.
    Chop,
    /// Drives a shovel into the ground.
    Dig,
    /// Bends down to the ground, to plant or pick.
    Pick,
    /// Holds a watering can out and pours.
    Water,
    /// Reaches out with something, as when trading or building.
    Reach,
    Eat,
    /// Waves, as shopkeepers do at a trade.
    Greet,
}

impl Act {
    fn of(what: Happened) -> Self {
        match what {
            Happened::Tilled | Happened::Struck(_) => Self::Chop,
            Happened::Dug(_) | Happened::Raised => Self::Dig,
            Happened::Planted | Happened::Fertilized | Happened::Harvested => Self::Pick,
            Happened::Watered => Self::Water,
            Happened::Traded | Happened::Built => Self::Reach,
            Happened::Ate => Self::Eat,
        }
    }

    /// The face a figure makes while acting this out.
    fn expression(self) -> Expression {
        match self {
            Self::Chop | Self::Dig => Expression::Effort,
            Self::Reach | Self::Eat | Self::Greet => Expression::Smile,
            Self::Pick | Self::Water => Expression::Rest,
        }
    }

    fn length(self) -> Duration {
        Duration::from_millis(match self {
            Self::Chop => 550,
            Self::Dig | Self::Reach => 600,
            Self::Pick => 750,
            Self::Water | Self::Eat => 900,
            Self::Greet => 1500,
        })
    }

    /// Poses the limbs it moves, `progress` of the way through it, at `clock`
    /// seconds.
    fn pose(self, progress: f32, clock: f32, angles: &mut Angles) {
        let t = progress;
        match self {
            Self::Chop => {
                // Both hands raise the tool over the head, elbows bent, and
                // bring it down straightening; the body follows through.
                let raise = along(&[(0.0, 0.4), (0.4, 2.9), (0.62, 0.7), (1.0, 0.5)], t);
                angles.right_arm.x = raise;
                angles.left_arm.x = raise * 0.85;
                angles.left_arm.z = -0.3;
                let elbow = along(&[(0.0, 0.3), (0.4, 1.3), (0.62, 0.2), (1.0, 0.4)], t);
                angles.right_forearm.x = elbow;
                angles.left_forearm.x = elbow;
                angles.body.x = along(&[(0.0, 0.0), (0.4, 0.15), (0.62, -0.35), (1.0, -0.1)], t);
                angles.head.x = -angles.body.x * 0.6;
                let step = along(&[(0.0, 0.0), (0.4, 0.0), (0.62, 1.0), (1.0, 0.6)], t);
                angles.left_leg.x = 0.45 * step;
                angles.right_leg.x = -0.3 * step;
                angles.left_shin.x = -0.5 * step;
            }
            Self::Dig => {
                // The shovel goes in with the weight of the body over it,
                // knees bending, and lifts.
                let arms = along(&[(0.0, 0.5), (0.35, 1.3), (0.6, 0.6), (1.0, 0.6)], t);
                angles.right_arm.x = arms;
                angles.left_arm.x = arms * 0.8;
                angles.right_forearm.x =
                    along(&[(0.0, 0.4), (0.35, 0.2), (0.6, 1.0), (1.0, 0.5)], t);
                angles.left_forearm.x = angles.right_forearm.x * 0.8;
                let crouch = along(&[(0.0, 0.0), (0.35, 0.2), (0.6, 1.0), (1.0, 0.4)], t);
                angles.body.x = -0.5 * crouch;
                angles.head.x = 0.3 * crouch;
                angles.right_leg.x = 0.55 * crouch;
                angles.left_leg.x = 0.55 * crouch;
                angles.right_shin.x = -0.9 * crouch;
                angles.left_shin.x = -0.9 * crouch;
                angles.root_lift = -0.1 * crouch;
            }
            Self::Pick => {
                // A squat down to the ground and back up.
                let squat = along(&[(0.0, 0.0), (0.35, 1.0), (0.7, 1.0), (1.0, 0.0)], t);
                angles.body.x = -0.55 * squat;
                angles.head.x = 0.35 * squat;
                angles.right_leg.x = 1.3 * squat;
                angles.left_leg.x = 1.3 * squat;
                angles.right_shin.x = -2.3 * squat;
                angles.left_shin.x = -2.3 * squat;
                angles.root_lift = -0.24 * squat;
                let reach = along(&[(0.0, 0.2), (0.35, 0.9), (0.7, 1.1), (1.0, 0.3)], t);
                angles.right_arm.x = reach;
                angles.left_arm.x = reach * 0.9;
                angles.right_forearm.x = 0.4 * squat;
                angles.left_forearm.x = 0.4 * squat;
            }
            Self::Water => {
                angles.right_arm.x = along(&[(0.0, 0.4), (0.25, 0.9), (0.8, 0.9), (1.0, 0.5)], t);
                let pouring = along(&[(0.0, 0.0), (0.25, 1.0), (0.8, 1.0), (1.0, 0.0)], t);
                angles.right_forearm.x = 0.5 * pouring;
                angles.right_arm.z = -0.25 * pouring + 0.06 * (clock * 18.0).sin() * pouring;
                angles.body.x = -0.12 * pouring;
                angles.right_leg.x = 0.2 * pouring;
                angles.right_shin.x = -0.25 * pouring;
            }
            Self::Reach => {
                angles.right_arm.x = along(&[(0.0, 0.3), (0.4, 1.2), (1.0, 0.4)], t);
                angles.right_forearm.x = along(&[(0.0, 0.3), (0.4, 0.6), (1.0, 0.4)], t);
                angles.body.y = along(&[(0.0, 0.0), (0.4, -0.15), (1.0, 0.0)], t);
            }
            Self::Eat => {
                // The forearm folds up to the mouth, and the head bobs
                // chewing.
                angles.right_arm.x = along(&[(0.0, 0.3), (0.3, 0.7), (0.8, 0.7), (1.0, 0.4)], t);
                angles.right_forearm.x =
                    along(&[(0.0, 0.4), (0.3, 2.1), (0.8, 2.1), (1.0, 0.5)], t);
                angles.right_arm.z =
                    along(&[(0.0, 0.0), (0.3, -0.35), (0.8, -0.35), (1.0, 0.0)], t);
                let chewing = along(&[(0.0, 0.0), (0.3, 1.0), (0.8, 1.0), (1.0, 0.0)], t);
                angles.head.x = 0.08 * (clock * 18.0).sin() * chewing - 0.1 * chewing;
            }
            Self::Greet => {
                // The arm goes up and the forearm waves.
                let raised = along(&[(0.0, 0.0), (0.2, 1.0), (0.8, 1.0), (1.0, 0.0)], t);
                angles.right_arm.z = 0.1 + raised * 2.4;
                angles.right_arm.x = 0.1;
                angles.right_forearm.x = 0.5 * raised;
                angles.right_forearm.z = raised * 0.5 * (clock * 12.0).sin();
                angles.head.z = -0.12 * raised;
            }
        }
    }
}

/// The value at `t` along `keys` of (time, value), eased from one to the next.
fn along(keys: &[(f32, f32)], t: f32) -> f32 {
    let mut previous = keys[0];
    for &(time, value) in keys {
        if t <= time {
            let span = time - previous.0;
            let s = if span > 0.0 {
                ((t - previous.0) / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let s = s * s * (3.0 - 2.0 * s);
            return previous.1 + (value - previous.1) * s;
        }
        previous = (time, value);
    }
    previous.1
}

/// How a figure's limbs are turned, around x, y and z. A limb hanging
/// down turned forward around x swings toward -z, the way figures face,
/// and the body and the head lean back; a forearm turned forward bends at
/// the elbow, a shin turned back at the knee. An arm turned around z swings
/// out from the body on the right and in on the left. `root_lift` moves the
/// whole figure up or down, for crouching and bobbing.
#[derive(Default)]
struct Angles {
    body: Vec3,
    head: Vec3,
    right_arm: Vec3,
    left_arm: Vec3,
    right_leg: Vec3,
    left_leg: Vec3,
    right_forearm: Vec3,
    left_forearm: Vec3,
    right_shin: Vec3,
    left_shin: Vec3,
    root_lift: f32,
}

impl Angles {
    fn of(&self, part: Part) -> Vec3 {
        match part {
            Part::Body => self.body,
            Part::Head => self.head,
            Part::RightArm => self.right_arm,
            Part::LeftArm => self.left_arm,
            Part::RightLeg => self.right_leg,
            Part::LeftLeg => self.left_leg,
            Part::RightForearm => self.right_forearm,
            Part::LeftForearm => self.left_forearm,
            Part::RightShin => self.right_shin,
            Part::LeftShin => self.left_shin,
        }
    }
}

/// A shopkeeper, who greets whoever trades at the stall at `stall`.
#[derive(Component)]
pub(crate) struct Keeper {
    pub stall: Vec3,
}

/// The model held in a figure's hand, and the item it shows.
#[derive(Component)]
struct HeldModel {
    item: Option<ItemId>,
    model: Option<Entity>,
}

/// Builds a figure dressed in `look` under `owner`, which also gets a pose.
pub(crate) fn dress(
    commands: &mut Commands,
    content: &Content,
    meshes: &FigureMeshes,
    tailor: &mut Tailor,
    owner: Entity,
    look: &Look,
) {
    let material = tailor.material(look);
    let figure = spawn_figure(commands, content.characters(), meshes, &material, owner);
    #[expect(clippy::cast_precision_loss, reason = "only spreads idle motion")]
    let offset = (owner.index_u32() as f32 * 2.39).rem_euclid(std::f32::consts::TAU);
    commands.entity(owner).insert((
        figure,
        Pose {
            offset,
            ..default()
        },
        Visibility::default(),
    ));
}

fn attach_avatar(
    trigger: On<Add, PlayerId>,
    players: Query<&PlayerId>,
    content: Res<Content>,
    meshes: Res<FigureMeshes>,
    mut tailor: Tailor,
    mut commands: Commands,
) {
    let Ok(&PlayerId(peer)) = players.get(trigger.entity) else {
        return;
    };
    let look = content.characters().look_for(peer.to_bits());
    commands.entity(trigger.entity).insert(Transform::default());
    dress(
        &mut commands,
        &content,
        &meshes,
        &mut tailor,
        trigger.entity,
        &look,
    );
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
    mut characters: Query<(&PlayerId, &mut Pose)>,
    mut keepers: Query<(&Keeper, &mut Pose), Without<PlayerId>>,
) {
    let now = time.elapsed();
    for Witnessed(happening) in witnessed.read() {
        if let Some(by) = happening.by
            && let Some((_, mut pose)) = characters.iter_mut().find(|(id, _)| id.0 == by)
        {
            pose.acting = Some((Act::of(happening.what), now));
        }
        if happening.what == Happened::Traded {
            for (keeper, mut pose) in &mut keepers {
                if keeper.stall.distance(happening.at) < KEEPER_REACH {
                    pose.acting = Some((Act::Greet, now));
                }
            }
        }
    }
}

/// Turns every figure's limbs toward the pose of what its character is
/// doing, and lays sleepers in bed.
fn pose_figures(
    time: Res<Time>,
    content: Res<Content>,
    meshes: Res<FigureMeshes>,
    mut figures: Query<(
        &Figure,
        &mut Pose,
        &GlobalTransform,
        Option<&Velocity>,
        Has<Asleep>,
        Option<&HeldModel>,
    )>,
    beds: Query<&Structure>,
    mut transforms: Query<&mut Transform>,
    mut faces: Query<&mut Mesh3d>,
) {
    let now = time.elapsed();
    let clock = time.elapsed_secs();
    let dt = time.delta_secs();
    for (figure, mut pose, placed, velocity, asleep, held) in &mut figures {
        if pose
            .acting
            .is_some_and(|(act, since)| now >= since + act.length())
        {
            pose.acting = None;
        }
        let velocity = velocity.map_or(Vec3::ZERO, |velocity| velocity.0);
        let speed = velocity.xz().length();
        pose.stride += speed * STRIDE * dt;
        if pose.vertical < LANDING_SPEED && velocity.y > LANDING_SPEED / 2.0 {
            pose.landed = Some(now);
        }
        pose.vertical = velocity.y;
        let holding = held.is_some_and(|held| held.item.is_some());

        let mut angles = Angles::default();
        let mut root = Transform::default();
        if asleep {
            root = lying(placed, &content, &beds);
        } else {
            moving(&mut angles, &pose, velocity, clock + pose.offset);
            if let Some(landed) = pose.landed {
                let progress = now.saturating_sub(landed).as_secs_f32() / LANDING.as_secs_f32();
                landing(progress, &mut angles);
            }
            if holding {
                angles.right_arm.x += HOLDING;
                angles.right_forearm.x += HOLDING_ELBOW;
            }
            if let Some((act, since)) = pose.acting {
                let progress = now.saturating_sub(since).as_secs_f32() / act.length().as_secs_f32();
                act.pose(progress, clock, &mut angles);
            }
            root.translation.y = angles.root_lift;
        }

        let expression = expression(&pose, asleep, velocity, clock);
        if pose.expression != expression
            && let Ok(mut face) = faces.get_mut(figure.face)
        {
            face.0 = meshes.face(expression);
            pose.expression = expression;
        }

        let rate = if pose.acting.is_some() {
            ACTING_EASING
        } else {
            EASING
        };
        let blend = 1.0 - (-rate * dt).exp();
        if let Ok(mut transform) = transforms.get_mut(figure.root) {
            transform.translation = transform.translation.lerp(root.translation, blend);
            transform.rotation = transform.rotation.slerp(root.rotation, blend);
        }
        for part in Part::ALL {
            if let Ok(mut transform) = transforms.get_mut(figure.limb(part)) {
                let turn = angles.of(part);
                let wanted = Quat::from_euler(EulerRot::XYZ, turn.x, turn.y, turn.z);
                transform.rotation = transform.rotation.slerp(wanted, blend);
            }
        }
    }
}

/// The face a figure makes: asleep, acting something out, falling, blinking,
/// or at rest, the first of these that holds.
fn expression(pose: &Pose, asleep: bool, velocity: Vec3, clock: f32) -> Expression {
    if asleep {
        return Expression::Blink;
    }
    if let Some((act, _)) = pose.acting {
        return act.expression();
    }
    if velocity.y < FALLING {
        return Expression::Surprise;
    }
    // Each figure blinks at its own pace, the offset spreading them out.
    let share = pose.offset / std::f32::consts::TAU;
    let every = BLINK_EVERY.0 + (BLINK_EVERY.1 - BLINK_EVERY.0) * share;
    if (clock + pose.offset * 7.3).rem_euclid(every) < BLINK_LENGTH {
        Expression::Blink
    } else {
        Expression::Rest
    }
}

/// Poses a figure for how it moves: standing, walking, running or in the
/// air.
fn moving(angles: &mut Angles, pose: &Pose, velocity: Vec3, clock: f32) {
    let run_speed = WALK_SPEED * (1.0 + SPRINT_FACTOR);
    let speed = velocity.xz().length();
    let walking = (speed / WALK_SPEED).min(1.0);
    let running = ((speed - WALK_SPEED) / (run_speed - WALK_SPEED)).clamp(0.0, 1.0);
    let stride = pose.stride;
    let swing = stride.sin() * (WALK_SWING + (RUN_SWING - WALK_SWING) * running) * walking;

    // Legs swing from the hips; each knee bends as its leg comes forward
    // and straightens as the foot is planted.
    let knee = (WALK_KNEE + (RUN_KNEE - WALK_KNEE) * running) * walking;
    let bend = |phase: f32| -knee * (0.5 + 0.5 * (phase - 1.2).sin()).powi(2);
    angles.right_leg.x = swing;
    angles.left_leg.x = -swing;
    angles.right_shin.x = bend(stride);
    angles.left_shin.x = bend(stride + std::f32::consts::PI);

    // Arms swing against the legs, elbows bent, pumping at a run.
    let elbow = (WALK_ELBOW + (RUN_ELBOW - WALK_ELBOW) * running) * walking;
    angles.right_arm.x = -swing * 0.9;
    angles.left_arm.x = swing * 0.9;
    angles.right_forearm.x = elbow + 0.3 * (-stride.sin()).max(0.0) * walking;
    angles.left_forearm.x = elbow + 0.3 * stride.sin().max(0.0) * walking;

    // The torso leans into a run and twists a little against the hips,
    // which sway with each step; the head stays level.
    angles.body.x = -RUN_LEAN * running - 0.04 * walking;
    angles.body.y = 0.08 * stride.sin() * walking;
    angles.body.z = 0.04 * stride.cos() * walking;
    angles.head.x = RUN_LEAN * 0.6 * running;
    angles.head.z = -angles.body.z * 0.5;
    let bob = WALK_BOB + (RUN_BOB - WALK_BOB) * running;
    angles.root_lift = (2.0 * stride).sin().abs() * bob * walking;

    // Standing still, a figure breathes, shifts its weight and looks about.
    let idle = 1.0 - walking;
    let breath = (clock * 1.7).sin();
    let shift = (clock * 0.45).sin();
    angles.body.x += -0.025 * breath * idle;
    angles.body.z += 0.03 * shift * idle;
    angles.right_arm.z = (0.08 + 0.02 * breath) * idle.max(0.4);
    angles.left_arm.z = -angles.right_arm.z;
    angles.right_forearm.x += 0.12 * idle;
    angles.left_forearm.x += 0.12 * idle;
    angles.right_leg.z = 0.05 * idle;
    angles.left_leg.z = -0.05 * idle;
    angles.right_leg.x += -0.04 * shift * idle;
    angles.left_leg.x += 0.04 * shift * idle;
    angles.head.y = 0.25 * (clock * 0.37).sin() * idle;
    angles.head.x += 0.05 * (clock * 0.53).sin() * idle;

    if velocity.y > RISING {
        // Springing up: arms back and out, knees tucked.
        angles.right_arm.x = -0.5;
        angles.left_arm.x = -0.5;
        angles.right_arm.z = 0.5;
        angles.left_arm.z = -0.5;
        angles.right_forearm.x = 0.8;
        angles.left_forearm.x = 0.8;
        angles.right_leg.x = 0.9;
        angles.left_leg.x = 0.3;
        angles.right_shin.x = -1.3;
        angles.left_shin.x = -0.8;
        angles.body.x = -0.15;
    } else if velocity.y < FALLING {
        // Falling: arms up and out, legs reaching for the ground.
        angles.right_arm.x = -0.2;
        angles.left_arm.x = -0.2;
        angles.right_arm.z = 1.3;
        angles.left_arm.z = -1.3;
        angles.right_forearm.x = 0.5;
        angles.left_forearm.x = 0.5;
        angles.right_leg.x = 0.35;
        angles.left_leg.x = -0.25;
        angles.right_shin.x = -0.4;
        angles.left_shin.x = -0.3;
        angles.body.x = 0.1;
    }
}

/// Bends the figure into a crouch on landing and straightens it up again,
/// `progress` of the way through the landing.
fn landing(progress: f32, angles: &mut Angles) {
    let crouch = along(&[(0.0, 0.3), (0.25, 1.0), (1.0, 0.0)], progress);
    angles.body.x -= 0.35 * crouch;
    angles.head.x += 0.25 * crouch;
    angles.right_leg.x += 0.6 * crouch;
    angles.left_leg.x += 0.6 * crouch;
    angles.right_shin.x -= 1.1 * crouch;
    angles.left_shin.x -= 1.1 * crouch;
    angles.right_arm.x += 0.6 * crouch;
    angles.left_arm.x += 0.6 * crouch;
    angles.right_forearm.x += 0.4 * crouch;
    angles.left_forearm.x += 0.4 * crouch;
    angles.root_lift -= 0.12 * crouch;
}

/// Where a sleeper's figure lies, in its character's frame: in the nearest
/// bed, its head on the pillow, or on the ground.
fn lying(placed: &GlobalTransform, content: &Content, beds: &Query<&Structure>) -> Transform {
    // Face up, the head where the feet pointed.
    let on_back = Quat::from_rotation_x(FRAC_PI_2);
    let at = placed.translation();
    let bed = beds
        .iter()
        .filter(|structure| content.structure(structure.kind).purpose == Purpose::Bed)
        .map(|bed| (bed, bed.position.distance(at)))
        .filter(|&(_, distance)| distance < BED_REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let Some((bed, _)) = bed else {
        return Transform::from_xyz(0.0, GROUND_LYING, 0.0).with_rotation(on_back);
    };
    // Beds are laid out with their head toward +z; the feet rest toward -z.
    let bed_turn = Quat::from_rotation_y(bed.facing);
    let feet = bed.position + bed_turn * Vec3::new(0.0, BED_HEIGHT, -BED_FEET);
    let world = Transform::from_translation(feet).with_rotation(bed_turn * on_back);
    let inverse = placed.affine().inverse();
    Transform::from_matrix(Mat4::from(inverse) * world.to_matrix())
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
        &Figure,
        Option<&Holding>,
        Has<InputMarker<PlayerInput>>,
        Option<&mut HeldModel>,
    )>,
    mut commands: Commands,
) {
    for (entity, figure, holding, is_local, shown) in &mut holders {
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
        let model = item.and_then(|item| {
            let scene = models.scene(content.item(item).model.as_deref()?)?;
            let hand = figure.limb(HAND);
            Some(
                commands
                    .spawn((scene, in_hand(&content, item), ChildOf(hand)))
                    .id(),
            )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_eased_between_and_held_beyond() {
        let keys = [(0.0, 1.0), (0.5, 3.0), (1.0, 2.0)];
        assert!((along(&keys, 0.0) - 1.0).abs() < 1e-6);
        assert!((along(&keys, 0.25) - 2.0).abs() < 1e-6);
        assert!((along(&keys, 0.5) - 3.0).abs() < 1e-6);
        assert!((along(&keys, 2.0) - 2.0).abs() < 1e-6);
    }
}
