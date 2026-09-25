//! Scenery on the loaded columns: which props stand there, which players
//! gathered and when, and the ground props keep.
//!
//! Every app grows the props of the columns it has loaded from the seed
//! (`messoria_worldgen::props_in_column`), so no prop crosses the network.
//! What players gathered is the only state: the server keeps all of it and
//! saves it, and tells each client what was gathered on every column it
//! sends, and every change after. A gathered prop stands as what its kind
//! leaves behind, if anything, until it grows back.
//!
//! Nobody can dig, raise or till the ground a standing prop keeps, and
//! characters walk around those that block the way.

use std::collections::HashMap;

use bevy::{ecs::message::Message, prelude::*};
use messoria_content::Catalog;
use messoria_worldgen::{column_of, props_in_column};

use crate::{
    content::Content,
    obstacles::{Blocker, Obstacles},
    valley::{ColumnLoaded, ColumnUnloaded, PlacedProp, PropKey, Valley},
};

/// Size of the cells props are indexed by, in meters.
const INDEX_CELL: f32 = 4.0;

/// A prop was gathered or grew back.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneryChanged(pub PropKey);

/// Grows and forgets the scenery of columns as they load and unload.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScenerySystems;

pub(crate) struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scenery>()
            .add_message::<SceneryChanged>()
            .add_systems(
                PreUpdate,
                (forget_columns, grow_columns)
                    .chain()
                    .in_set(ScenerySystems),
            )
            .add_systems(PostUpdate, follow_changes);
    }
}

/// The props of the loaded columns, and what players gathered.
#[derive(Resource, Default)]
pub struct Scenery {
    props: HashMap<PropKey, Grown>,
    columns: HashMap<IVec2, Vec<PropKey>>,
    /// Props by the cell their middle is in.
    cells: HashMap<IVec2, Vec<PropKey>>,
    largest_footprint: f32,
    /// The day each gathered prop was gathered: every one on a server, those
    /// on its loaded columns on a client.
    gathered: HashMap<PropKey, u32>,
}

/// A grown prop, and what its kind says about the ground it keeps.
#[derive(Clone, Copy, Debug)]
struct Grown {
    prop: PlacedProp,
    footprint: f32,
    blocks: bool,
    /// Whether something still stands once it is gathered.
    stays: bool,
}

impl Scenery {
    /// The prop `key` names, if its column is loaded.
    pub fn prop(&self, key: PropKey) -> Option<&PlacedProp> {
        self.props.get(&key).map(|grown| &grown.prop)
    }

    /// The day the prop `key` names was gathered, if it was and has not
    /// grown back.
    pub fn gathered(&self, key: PropKey) -> Option<u32> {
        self.gathered.get(&key).copied()
    }

    /// The props of every loaded column.
    pub fn props(&self) -> impl Iterator<Item = &PlacedProp> {
        self.props.values().map(|grown| &grown.prop)
    }

    /// The props of `column`, if it is loaded.
    pub fn column(&self, column: IVec2) -> impl Iterator<Item = &PlacedProp> {
        self.columns
            .get(&column)
            .into_iter()
            .flatten()
            .filter_map(|key| self.prop(*key))
    }

    /// Whether the scenery of `column` has been grown.
    pub fn is_loaded(&self, column: IVec2) -> bool {
        self.columns.contains_key(&column)
    }

    /// Every gathered prop this app knows of, with the day it was gathered.
    pub fn gathered_props(&self) -> impl Iterator<Item = (PropKey, u32)> + '_ {
        self.gathered.iter().map(|(&key, &day)| (key, day))
    }

    /// The props gathered on the loaded column `column`.
    pub fn gathered_in(&self, column: IVec2) -> Vec<(PropKey, u32)> {
        self.columns
            .get(&column)
            .into_iter()
            .flatten()
            .filter_map(|&key| Some((key, self.gathered(key)?)))
            .collect()
    }

    /// Whether a disc of `radius` around `point` reaches the ground under a
    /// standing prop.
    pub fn blocks(&self, point: Vec3, radius: f32) -> bool {
        self.near(point.xz(), radius).next().is_some()
    }

    /// The standing prop whose footprint is nearest `point`, if one is
    /// within `slack` of it.
    pub fn prop_at(&self, point: Vec3, slack: f32) -> Option<PropKey> {
        let center = point.xz();
        self.near(center, slack)
            .min_by(|a, b| {
                let gap =
                    |grown: &Grown| grown.prop.position.xz().distance(center) - grown.footprint;
                gap(a).total_cmp(&gap(b))
            })
            .map(|grown| grown.prop.key)
    }

    /// Whether `test` holds for the middle and radius of any standing
    /// prop's footprint within `reach` of `center`.
    pub fn any_near(&self, center: Vec2, reach: f32, test: impl Fn(Vec2, f32) -> bool) -> bool {
        self.near(center, reach)
            .any(|grown| test(grown.prop.position.xz(), grown.footprint))
    }

    /// Marks the prop `key` names gathered on `day`.
    pub fn gather(&mut self, key: PropKey, day: u32) {
        self.gathered.insert(key, day);
    }

    /// The prop `key` names grew back.
    pub fn regrow(&mut self, key: PropKey) {
        self.gathered.remove(&key);
    }

    /// Takes note of props gathered, and when, as a save or a server tells.
    pub fn remember_gathered(&mut self, gathered: impl IntoIterator<Item = (PropKey, u32)>) {
        self.gathered.extend(gathered);
    }

    /// Forgets what was gathered on the loaded column `column`, which is
    /// about to be unloaded, for a client that will be told again.
    pub fn forget_gathered_in(&mut self, column: IVec2) {
        for key in self.columns.get(&column).into_iter().flatten() {
            self.gathered.remove(key);
        }
    }

    /// Whether the prop `key` names is loaded and something of it stands.
    fn standing(&self, grown: &Grown) -> bool {
        grown.stays || !self.gathered.contains_key(&grown.prop.key)
    }

    /// Standing props whose footprint a disc of `radius` around `center`
    /// reaches.
    fn near(&self, center: Vec2, radius: f32) -> impl Iterator<Item = &Grown> {
        let reach = radius + self.largest_footprint;
        let (min, max) = (cell_of(center - reach), cell_of(center + reach));
        (min.y..=max.y)
            .flat_map(move |z| (min.x..=max.x).map(move |x| IVec2::new(x, z)))
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .filter_map(|key| self.props.get(key))
            .filter(move |grown| {
                self.standing(grown)
                    && grown.prop.position.xz().distance(center) < radius + grown.footprint
            })
    }

    fn insert_column(&mut self, column: IVec2, props: Vec<PlacedProp>, catalog: &Catalog) {
        let mut keys = Vec::with_capacity(props.len());
        for prop in props {
            let definition = catalog.prop(prop.key.kind);
            let grown = Grown {
                prop,
                footprint: definition.radius * prop.scale,
                blocks: definition.blocks,
                stays: definition.stands_when_gathered(),
            };
            self.largest_footprint = self.largest_footprint.max(grown.footprint);
            self.cells
                .entry(cell_of(prop.position.xz()))
                .or_default()
                .push(prop.key);
            self.props.insert(prop.key, grown);
            keys.push(prop.key);
        }
        self.columns.insert(column, keys);
    }

    /// Forgets the props of `column`, returning their keys.
    fn remove_column(&mut self, column: IVec2) -> Vec<PropKey> {
        let keys = self.columns.remove(&column).unwrap_or_default();
        for key in &keys {
            if let Some(grown) = self.props.remove(key) {
                let cell = cell_of(grown.prop.position.xz());
                if let Some(keys) = self.cells.get_mut(&cell) {
                    keys.retain(|other| other != key);
                    if keys.is_empty() {
                        self.cells.remove(&cell);
                    }
                }
            }
        }
        keys
    }

    /// Where the prop `key` names blocks the way, if it is loaded, standing
    /// and in the way.
    fn obstacle(&self, key: PropKey) -> Option<(Vec3, f32)> {
        let grown = self.props.get(&key)?;
        (grown.blocks && self.standing(grown)).then_some((grown.prop.position, grown.footprint))
    }
}

fn cell_of(point: Vec2) -> IVec2 {
    (point / INDEX_CELL).floor().as_ivec2()
}

/// Grows the props of columns that were loaded, in the way of whoever walks
/// there.
fn grow_columns(
    mut loaded: MessageReader<ColumnLoaded>,
    valley: Option<Res<Valley>>,
    content: Res<Content>,
    mut scenery: ResMut<Scenery>,
    mut obstacles: ResMut<Obstacles>,
) {
    let Some(valley) = valley else {
        loaded.clear();
        return;
    };
    for &ColumnLoaded(column) in loaded.read() {
        if scenery.is_loaded(column) {
            continue;
        }
        let props = props_in_column(&valley, &content, column);
        scenery.insert_column(column, props, &content);
        let keys: Vec<PropKey> = scenery.columns[&column].clone();
        for key in keys {
            if let Some((position, radius)) = scenery.obstacle(key) {
                obstacles.block_with_prop(key, position, radius);
            }
        }
    }
}

fn forget_columns(
    mut unloaded: MessageReader<ColumnUnloaded>,
    mut scenery: ResMut<Scenery>,
    mut obstacles: ResMut<Obstacles>,
) {
    for &ColumnUnloaded(column) in unloaded.read() {
        for key in scenery.remove_column(column) {
            obstacles.clear(Blocker::Prop(key));
        }
    }
}

/// Props gathered stop blocking the way unless something stands where they
/// did; props that grew back block it again.
fn follow_changes(
    mut changes: MessageReader<SceneryChanged>,
    scenery: Res<Scenery>,
    mut obstacles: ResMut<Obstacles>,
) {
    for &SceneryChanged(key) in changes.read() {
        obstacles.clear(Blocker::Prop(key));
        if let Some((position, radius)) = scenery.obstacle(key) {
            obstacles.block_with_prop(key, position, radius);
        }
    }
}

/// The column the prop `key` names stands in, if its column is loaded.
pub fn column_of_prop(scenery: &Scenery, key: PropKey) -> Option<IVec2> {
    scenery.prop(key).map(|prop| column_of(prop.position.xz()))
}

#[cfg(test)]
mod tests {
    use messoria_worldgen::Landscape;

    use super::*;
    use crate::content::load_content;

    fn grown() -> (Scenery, PlacedProp) {
        let content = load_content().expect("the shipped content is valid");
        let landscape = Landscape::new(7);
        let mut scenery = Scenery::default();
        let column = (0..20)
            .map(|x| IVec2::new(x, 3))
            .find(|&column| !props_in_column(&landscape, &content, column).is_empty())
            .expect("props grow near the village");
        scenery.insert_column(
            column,
            props_in_column(&landscape, &content, column),
            &content,
        );
        let prop = *scenery.column(column).next().expect("a prop");
        (scenery, prop)
    }

    #[test]
    fn the_ground_under_props_is_kept_until_they_leave_nothing_standing() {
        let (mut scenery, prop) = grown();
        assert!(scenery.blocks(prop.position, 0.0));
        assert!(scenery.blocks(prop.position + Vec3::X, 1.5));
        assert_eq!(scenery.prop_at(prop.position, 0.5), Some(prop.key));

        scenery.gather(prop.key, 3);
        assert_eq!(scenery.gathered(prop.key), Some(3));
        let content = load_content().expect("the shipped content is valid");
        let stays = content.prop(prop.key.kind).stands_when_gathered();
        assert_eq!(scenery.blocks(prop.position, 0.0), stays);

        scenery.regrow(prop.key);
        assert!(scenery.blocks(prop.position, 0.0));
    }

    #[test]
    fn unloaded_columns_forget_their_props_but_not_what_was_gathered() {
        let (mut scenery, prop) = grown();
        let column = column_of(prop.position.xz());
        scenery.gather(prop.key, 2);
        assert_eq!(scenery.gathered_in(column), [(prop.key, 2)]);
        scenery.remove_column(column);
        assert!(scenery.prop(prop.key).is_none());
        assert!(!scenery.blocks(prop.position, 0.0));
        assert_eq!(scenery.gathered(prop.key), Some(2));
    }
}
