//! Everything that crosses the wire: replicated components, player input,
//! the valley, inventories, fields, money, shops and player actions.

use std::fmt;

use bevy::{ecs::entity::MapEntities, prelude::*};
use lightyear::prelude::{input::native::InputPlugin, *};
use messoria_calendar::{Weather, WorldTime};
use messoria_content::{ItemId, ShopId, StructureId, Tool};
use messoria_economy::{Market, Refusal, SalesLedger, Wallet};
use messoria_farming::Planting;
use messoria_inventory::Inventory;
use messoria_voxel::{ChunkChanges, Material};
use serde::{Deserialize, Serialize};

use crate::{energy::Energy, valley::PropKey};

/// The peer an entity belongs to.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub PeerId);

/// Position of a character's feet in world space, in meters.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Position(pub Vec3);

/// Velocity in meters per second.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Velocity(pub Vec3);

/// Facing direction around the vertical axis, in radians. Zero faces -Z.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Heading(pub f32);

/// Controls sampled by a client once per simulation tick.
///
/// The server treats every field as untrusted; see `movement::advance`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PlayerInput {
    /// Desired movement relative to `yaw`: `x` strafes right, `y` moves forward.
    pub movement: Vec2,
    /// Camera heading the movement is relative to, in radians.
    pub yaw: f32,
    pub jump: bool,
    /// Run faster while moving forward.
    pub sprint: bool,
    /// The hotbar slot whose item the character holds, for others to see.
    pub held: u8,
}

impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// Marks a character that is asleep. Sleeping characters do not move.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asleep;

/// The world's current time, on the single clock entity the server replicates.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldClock(pub WorldTime);

/// Today's weather, on the clock entity.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentWeather(pub Weather);

/// How many players are asleep, and how many must be for the day to end.
/// Kept on the clock entity.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SleepTally {
    pub asleep: u32,
    pub required: u32,
}

/// A client asking for its character to go to sleep or wake up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepRequest {
    Sleep,
    Wake,
}

/// A square of tilled soil; see `fields`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub tile: IVec2,
    /// Height of the ground in the middle of the square.
    pub height: f32,
}

/// Marks a field watered for the day, by hand or by rain.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watered;

/// Marks a field spread with fertilizer, which improves its next harvest.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fertilized;

/// The crop growing in a field.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop(pub Planting);

/// A client asking to harvest the ripe crop in the field at `target`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct HarvestRequest {
    pub target: Vec3,
}

/// A client moving a stack between the inventory of its character and the
/// chest standing at `chest`, or within either. The character must be within
/// reach of the chest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MoveStored {
    pub chest: Vec3,
    pub from: Place,
    pub to: Place,
}

/// A slot of the character's inventory or of a chest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    Carried(u8),
    Stored(u8),
}

/// A client asking to gather, by hand, the prop at `target`, such as a berry
/// bush. Props gathered with a tool are gathered by using it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Gather {
    pub target: Vec3,
}

/// The valley streamed from the server to a client, a column of chunks at a
/// time, with what players gathered of the scenery on it.
///
/// A single message type keeps every update on one ordered channel, so the
/// seed arrives before any column, and a column before the changes made to
/// it afterwards.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TerrainUpdate {
    /// The world's seed, which the client grows the valley's shape and its
    /// scenery from.
    World { seed: u64 },
    /// A column of chunks came into range: each chunk encoded with
    /// `Chunk::encode`, from the bottom up, and the props gathered on it,
    /// with the day each was gathered.
    Column {
        column: IVec2,
        chunks: Vec<Vec<u8>>,
        gathered: Vec<(PropKey, u32)>,
    },
    /// Voxels of a loaded chunk changed.
    Changed(ChunkChanges),
    /// A column went out of range and can be forgotten.
    ColumnUnloaded(IVec2),
    /// A prop on a loaded column was gathered on `day`.
    Gathered { prop: PropKey, day: u32 },
    /// A prop on a loaded column grew back.
    Regrown(PropKey),
}

/// What a character carries.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Belongings(pub Inventory);

/// A client using the item in one of its hotbar slots, aimed at `target`
/// when the item acts on the world. What happens depends on the item.
///
/// The server checks the slot, the item and the target before acting.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct UseItem {
    pub slot: u8,
    pub action: ItemAction,
    pub target: Option<Vec3>,
}

/// Items have a main use and, for some, a second one; a shovel digs and raises.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    Primary,
    Secondary,
}

/// A client moving the stack in inventory slot `from` onto slot `to`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveItem {
    pub from: u8,
    pub to: u8,
}

/// A character's money.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Money(pub Wallet);

/// What a character sold each shop today, which counts against the shops'
/// daily limits.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SoldToday(pub SalesLedger);

/// The server-wide market that sets what shops pay, on an entity of its
/// own. Clients price goods from it exactly as the server will.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MarketState(pub Market);

/// A shop's stall in the village.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Shopfront {
    pub shop: ShopId,
    /// The ground in the middle of the stall.
    pub position: Vec3,
    /// Direction the counter faces, as a [`Heading`].
    pub facing: f32,
}

/// A client trading with a shop whose stall its character stands at.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trade {
    pub shop: ShopId,
    pub deal: Deal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deal {
    /// Sell up to `count` items from an inventory slot. The server sells as
    /// many as the shop still buys today.
    Sell { slot: u8, count: u16 },
    /// Buy exactly `count` of an item.
    Buy { item: ItemId, count: u16 },
}

/// Something players built, such as a cabin, a bed or a chest, standing at
/// `position` (the middle of its ground) turned by `facing`, as a
/// [`Heading`] is.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Structure {
    pub kind: StructureId,
    pub position: Vec3,
    pub facing: f32,
}

/// What a chest keeps.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored(pub Inventory);

/// A client giving money to another player.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiveMoney {
    pub to: PeerId,
    pub amount: u32,
}

/// Carries [`TerrainUpdate`]s, in order and without loss.
pub struct TerrainChannel;

/// Carries player actions such as [`UseItem`] and [`SleepRequest`], in order:
/// inventory moves only make sense applied in the order they were made.
pub struct ActionChannel;

/// Carries what the server tells clients about actions: [`Notice`]s and
/// [`Happening`]s.
pub struct FeedbackChannel;

/// Why the server did not do something a player asked for, shown to that
/// player.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    NotEnoughEnergy,
    ProtectedGround,
    SceneryInTheWay,
    WouldBuryPlayer,
    NoSoilToRaise,
    NoRoom,
    NotTillable,
    OutOfSeason,
    NotRipe,
    TooEarlyToSleep,
    /// Gathering that takes this tool.
    NeedsTool(Tool),
    /// Gathering by hand, not with a tool.
    PickByHand,
    /// Gathering what has not grown back yet.
    NothingToGather,
    /// Building where something else stands.
    NoRoomToBuild,
    /// Building on ground too uneven for it.
    GroundTooUneven,
    /// Going to sleep away from a bed.
    SleepInABed,
    Trade(Refusal),
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotEnoughEnergy => "Not enough energy. Eat something or sleep.",
            Self::ProtectedGround => "The village's ground cannot be worked.",
            Self::SceneryInTheWay => "Something stands on that ground.",
            Self::WouldBuryPlayer => "Someone is standing there.",
            Self::NoSoilToRaise => "You have no soil to raise the ground with.",
            Self::NoRoom => "Your backpack is full.",
            Self::NotTillable => "That ground is too steep or too hard to till.",
            Self::OutOfSeason => "That does not grow in this season.",
            Self::NotRipe => "That is not ripe yet.",
            Self::TooEarlyToSleep => "It is too early to sleep; bedtime is at 18:00.",
            Self::NeedsTool(tool) => match tool {
                Tool::Axe => "That takes an axe.",
                Tool::Pickaxe => "That takes a pickaxe.",
                Tool::Shovel => "That takes a shovel.",
                Tool::Hoe => "That takes a hoe.",
                Tool::WateringCan => "That takes a watering can.",
            },
            Self::PickByHand => "Pick that by hand.",
            Self::NothingToGather => "There is nothing to gather there yet.",
            Self::NoRoomToBuild => "There is no room to build that here.",
            Self::GroundTooUneven => "The ground is too uneven to build that here.",
            Self::SleepInABed => "Sleep in a bed.",
            Self::Trade(refusal) => return write!(f, "{}.", sentence(&refusal.to_string())),
        })
    }
}

/// `text` with its first letter capitalized.
fn sentence(text: &str) -> String {
    let mut letters = text.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

/// Something a player did in the world, for every client to hear and see.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Happening {
    pub what: Happened,
    /// Where it happened.
    pub at: Vec3,
    /// The player who did it, whose character acts it out.
    pub by: Option<PeerId>,
}

/// The item a character holds, as everyone sees it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holding(pub Option<ItemId>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Happened {
    /// The ground was dug, and it was made of this.
    Dug(Material),
    Raised,
    Tilled,
    Watered,
    Planted,
    Fertilized,
    Harvested,
    /// A tool struck a prop being gathered.
    Struck(Tool),
    /// A structure was built.
    Built,
    Traded,
    Ate,
}

impl Ease for Position {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| Position(start.0.lerp(end.0, t)))
    }
}

impl Diffable<Vec3> for Position {
    fn base_value() -> Self {
        Self::default()
    }

    fn diff(&self, new: &Self) -> Vec3 {
        new.0 - self.0
    }

    fn apply_diff(&mut self, delta: &Vec3) {
        self.0 += *delta;
    }
}

impl Ease for Heading {
    /// Turns through the shorter arc, so a heading crossing ±π does not spin
    /// all the way around.
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        let delta = (end.0 - start.0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        FunctionCurve::new(Interval::UNIT, move |t| Heading(start.0 + delta * t))
    }
}

/// Registers the protocol. Must be added after lightyear's client and server
/// plugin groups, which `SharedPlugin` guarantees.
pub(crate) struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputPlugin::<PlayerInput>::default());

        app.add_channel::<TerrainChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
        app.add_channel::<ActionChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.add_channel::<FeedbackChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Notice>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<Happening>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<TerrainUpdate>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<UseItem>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<MoveItem>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<HarvestRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Gather>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<MoveStored>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SleepRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<Trade>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<GiveMoney>()
            .add_direction(NetworkDirection::ClientToServer);

        app.component::<WorldClock>().replicate();
        app.component::<SleepTally>().replicate();
        app.component::<Energy>().replicate();
        app.component::<Asleep>().replicate();
        app.component::<Belongings>().replicate();
        app.component::<CurrentWeather>().replicate();
        app.component::<Field>().replicate();
        app.component::<Watered>().replicate();
        app.component::<Fertilized>().replicate();
        app.component::<Crop>().replicate();
        app.component::<Money>().replicate();
        app.component::<SoldToday>().replicate();
        app.component::<MarketState>().replicate();
        app.component::<Shopfront>().replicate();
        app.component::<Structure>().replicate();
        app.component::<Stored>().replicate();
        app.component::<Holding>().replicate();

        app.component::<PlayerId>().replicate();

        app.component::<Position>()
            .replicate()
            .add_linear_interpolation()
            .predict()
            .add_linear_correction::<Vec3>();

        app.component::<Velocity>().replicate().predict();

        app.component::<Heading>()
            .replicate()
            .add_linear_interpolation()
            .predict();
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    #[test]
    fn heading_interpolates_through_the_shorter_arc() {
        let curve = Heading::interpolating_curve_unbounded(Heading(PI - 0.1), Heading(-PI + 0.1));
        let midpoint = curve.sample_unchecked(0.5).0;

        assert!(
            (midpoint.abs() - PI).abs() < 1e-5,
            "midpoint was {midpoint}"
        );
    }
}
