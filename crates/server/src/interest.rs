//! What each client is sent of what stands in the world: only the fields,
//! structures, stalls and characters near its own character.
//!
//! Every column of the world is a room. Whatever stands in the world is put
//! in the room of the column it stands in, characters moving from room to
//! room as they walk, and each client joins the rooms of the columns around
//! its character, as far as it is sent terrain. Things in no room, such as
//! the clock and the market, reach every client.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::{connection::host::HostClient, prelude::*};
use messoria_shared::protocol::{Field, PlayerId, Position, Shopfront, Structure};
use messoria_worldgen::{COLUMNS, column_of};

use crate::{players::ControlledCharacter, terrain::VIEW_RADIUS};

/// Distance, in columns, within which a client is sent what stands in the
/// world: as far as it keeps terrain.
const INTEREST_RADIUS: i32 = VIEW_RADIUS + 1;

pub(crate) struct InterestPlugin;

impl Plugin for InterestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RoomPlugin);
        // Before anything is placed in them, even at startup.
        let rooms = allocate_rooms(&mut app.world_mut().resource_mut::<RoomAllocator>());
        app.insert_resource(rooms)
            .add_observer(place_field)
            .add_observer(place_structure)
            .add_observer(place_stall)
            .add_systems(PostUpdate, (follow_characters, follow_viewers).chain());
    }
}

/// The room of each column of the world.
#[derive(Resource)]
struct ColumnRooms(HashMap<IVec2, RoomId>);

impl ColumnRooms {
    fn room(&self, column: IVec2) -> Option<RoomId> {
        self.0.get(&column).copied()
    }

    /// The rooms of the columns within `radius` of `center`.
    fn around(&self, center: IVec2, radius: i32) -> Rooms {
        (-radius..=radius)
            .flat_map(|z| (-radius..=radius).map(move |x| IVec2::new(x, z)))
            .filter(|offset| offset.length_squared() <= radius * radius)
            .filter_map(|offset| self.room(center + offset))
            .into()
    }
}

/// The column a character's room is for.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct InColumn(IVec2);

/// The column a client's rooms are centered on.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct Watching(IVec2);

fn allocate_rooms(allocator: &mut RoomAllocator) -> ColumnRooms {
    ColumnRooms(
        COLUMNS
            .flat_map(|z| COLUMNS.map(move |x| IVec2::new(x, z)))
            .map(|column| (column, allocator.allocate()))
            .collect(),
    )
}

/// Puts `entity`, standing at `point`, in the room of its column.
fn place(commands: &mut Commands, rooms: &ColumnRooms, entity: Entity, point: Vec2) {
    if let Some(room) = rooms.room(column_of(point)) {
        commands.entity(entity).insert(Rooms::single(room));
    }
}

fn place_field(
    trigger: On<Add, Field>,
    fields: Query<&Field>,
    rooms: Res<ColumnRooms>,
    mut commands: Commands,
) {
    if let Ok(field) = fields.get(trigger.entity) {
        place(&mut commands, &rooms, trigger.entity, field.tile.as_vec2());
    }
}

fn place_structure(
    trigger: On<Add, Structure>,
    structures: Query<&Structure>,
    rooms: Res<ColumnRooms>,
    mut commands: Commands,
) {
    if let Ok(structure) = structures.get(trigger.entity) {
        place(
            &mut commands,
            &rooms,
            trigger.entity,
            structure.position.xz(),
        );
    }
}

fn place_stall(
    trigger: On<Add, Shopfront>,
    stalls: Query<&Shopfront>,
    rooms: Res<ColumnRooms>,
    mut commands: Commands,
) {
    if let Ok(stall) = stalls.get(trigger.entity) {
        place(&mut commands, &rooms, trigger.entity, stall.position.xz());
    }
}

/// Moves each character into the room of the column it walked into.
fn follow_characters(
    rooms: Res<ColumnRooms>,
    characters: Query<(Entity, &Position, Option<&InColumn>), With<PlayerId>>,
    mut commands: Commands,
) {
    for (entity, feet, was) in &characters {
        let column = column_of(feet.0.xz());
        if was.is_some_and(|was| was.0 == column) {
            continue;
        }
        if let Some(room) = rooms.room(column) {
            commands
                .entity(entity)
                .insert((InColumn(column), Rooms::single(room)));
        }
    }
}

/// Moves each client into the rooms around its character.
fn follow_viewers(
    rooms: Res<ColumnRooms>,
    characters: Query<&Position>,
    clients: Query<(Entity, &ControlledCharacter, Option<&Watching>), Without<HostClient>>,
    mut commands: Commands,
) {
    for (client, character, was) in &clients {
        let Ok(feet) = characters.get(character.0) else {
            continue;
        };
        let column = column_of(feet.0.xz());
        if was.is_some_and(|was| was.0 == column) {
            continue;
        }
        commands
            .entity(client)
            .insert((Watching(column), rooms.around(column, INTEREST_RADIUS)));
    }
}
