//! Models loaded from glTF files, tinted by the palette for the season and
//! swaying in the wind.
//!
//! Every mesh whose material has a name the palette colors is drawn with its
//! texture tinted by that color, and every one whose name the palette sways
//! is drawn with the wind's material; both are shared by all meshes of that
//! name that came with the same material. Foliage is painted in grays, so the palette gives it its
//! color: when the season turns, the tints change and everything follows,
//! from leaves to grass.

use std::collections::HashMap;

use bevy::{
    camera::visibility::RenderLayers,
    ecs::system::{EntityCommands, SystemParam},
    gltf::{Gltf, GltfAssetLabel, GltfMaterialName},
    prelude::*,
    world_serialization::{WorldAsset, WorldAssetRoot},
};
use messoria_calendar::{Season, WorldTime};
use messoria_content::{CropDef, ItemId, ItemKind, Rgb};
use messoria_shared::content::Content;

use crate::{
    clock::LocalClock,
    wind::{Sway, SwayMaterial},
};

/// Folder of the models, inside the assets folder.
const MODELS_FOLDER: &str = "models";

pub(crate) struct ArtPlugin;

impl Plugin for ArtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawnSeason>()
            .init_resource::<PaletteLooks>()
            .init_resource::<Models>()
            .add_systems(Startup, load_models)
            .add_systems(PreUpdate, (follow_season, use_palette_materials))
            .add_systems(Update, keep_on_layers);
    }
}

/// The season everything is drawn for: the world's, once its clock is known.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DrawnSeason(pub Season);

impl Default for DrawnSeason {
    fn default() -> Self {
        Self(Season::Spring)
    }
}

/// Draws everything under this entity on one render layer, for a camera of
/// its own, such as an icon's.
#[derive(Component, Clone, Copy)]
pub(crate) struct DrawnOnLayer(pub usize);

/// Models spawn their meshes without knowing about layers; this puts each
/// on the layer of the entity it hangs under, if any.
fn keep_on_layers(
    added: Query<Entity, (Added<Mesh3d>, Without<RenderLayers>)>,
    ancestors: Query<&ChildOf>,
    layers: Query<&DrawnOnLayer>,
    mut commands: Commands,
) {
    for mesh in &added {
        if let Some(DrawnOnLayer(layer)) = ancestors
            .iter_ancestors(mesh)
            .find_map(|ancestor| layers.get(ancestor).ok())
        {
            commands.entity(mesh).insert(RenderLayers::layer(*layer));
        }
    }
}

/// Every model the scenery uses, by its path within the models folder.
#[derive(Resource, Default)]
pub(crate) struct Models {
    files: HashMap<String, Handle<Gltf>>,
    scenes: HashMap<String, Handle<WorldAsset>>,
}

impl Models {
    /// The model at `path`, to spawn as a whole.
    pub(crate) fn scene(&self, path: &str) -> Option<WorldAssetRoot> {
        self.scenes.get(path).cloned().map(WorldAssetRoot)
    }

    /// The model file at `path`, for its meshes and material names.
    pub(crate) fn file(&self, path: &str) -> Option<&Handle<Gltf>> {
        self.files.get(path)
    }
}

/// The materials meshes are drawn with in place of their own, by material
/// name and the material they came with.
#[derive(Resource, Default)]
pub(crate) struct PaletteLooks {
    tinted: HashMap<(String, AssetId<StandardMaterial>), Handle<StandardMaterial>>,
    swaying: HashMap<(String, AssetId<StandardMaterial>), Handle<SwayMaterial>>,
}

/// A mesh's material in place of its own.
pub(crate) enum Drawn {
    Tinted(Handle<StandardMaterial>),
    Swaying(Handle<SwayMaterial>),
}

impl Drawn {
    /// Draws `entity`'s mesh with this material instead of its own.
    pub(crate) fn put_on(self, entity: &mut EntityCommands) {
        match self {
            Self::Tinted(material) => {
                entity.insert(MeshMaterial3d(material));
            }
            Self::Swaying(material) => {
                entity
                    .remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert(MeshMaterial3d(material));
            }
        }
    }
}

/// Dresses meshes in the palette's looks for the season.
#[derive(SystemParam)]
pub(crate) struct Palettes<'w> {
    content: Res<'w, Content>,
    season: Res<'w, DrawnSeason>,
    looks: ResMut<'w, PaletteLooks>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    swaying: ResMut<'w, Assets<SwayMaterial>>,
}

impl Palettes<'_> {
    /// The material a mesh with material `name`, which came with `source`,
    /// is drawn with, or `None` when the palette neither colors nor sways
    /// that name, and its own will do.
    pub(crate) fn dress(&mut self, name: &str, source: &Handle<StandardMaterial>) -> Option<Drawn> {
        let palette = self.content.palette();
        let color = palette.color(name, self.season.0);
        let strength = palette.sway(name);
        if color.is_none() && strength.is_none() {
            return None;
        }
        let key = (name.to_owned(), source.id());
        if let Some(material) = self.looks.swaying.get(&key) {
            return Some(Drawn::Swaying(material.clone()));
        }
        if let Some(material) = self.looks.tinted.get(&key) {
            return Some(Drawn::Tinted(material.clone()));
        }
        let mut base = self.materials.get(source).cloned().unwrap_or_default();
        if let Some(color) = color {
            base.base_color = srgb(color);
        }
        if let Some(strength) = strength {
            let material = self.swaying.add(SwayMaterial {
                base,
                extension: Sway::new(strength),
            });
            self.looks.swaying.insert(key, material.clone());
            Some(Drawn::Swaying(material))
        } else {
            let material = self.materials.add(base);
            self.looks.tinted.insert(key, material.clone());
            Some(Drawn::Tinted(material))
        }
    }
}

pub(crate) fn srgb([red, green, blue]: Rgb) -> Color {
    Color::srgb(red, green, blue)
}

const TOOL_COLOR: Color = Color::srgb(0.62, 0.64, 0.68);
const FERTILIZER_COLOR: Color = Color::srgb(0.35, 0.26, 0.17);
const GOODS_COLOR: Color = Color::srgb(0.75, 0.62, 0.45);

/// The color an item is marked with until items have icons: the one its
/// data gives, else its crop's color for seeds and produce, its ground's for
/// ground, and a color per kind for the rest.
pub(crate) fn item_color(content: &Content, item: ItemId) -> Color {
    let definition = content.item(item);
    if let Some(color) = definition.color {
        return srgb(color);
    }
    let crop_color = |crop: &CropDef| srgb(crop.color);
    match &definition.kind {
        ItemKind::Seed => content.crop_grown_from(item).map_or(GOODS_COLOR, |crop| {
            crop_color(content.crop(crop)).darker(0.15)
        }),
        ItemKind::Tool(_) => TOOL_COLOR,
        ItemKind::Fertilizer => FERTILIZER_COLOR,
        // What the ground looks like once dug up: soil rather than grass.
        ItemKind::Terrain { materials } => materials.first().map_or(GOODS_COLOR, |&material| {
            srgb(content.palette().ground(material.exposed(), Season::Summer))
        }),
        ItemKind::Structure => GOODS_COLOR,
        ItemKind::Food { .. } | ItemKind::Goods => content
            .crops()
            .find(|(_, crop)| crop.produce == item)
            .map_or(GOODS_COLOR, |(_, crop)| crop_color(crop)),
    }
}

fn load_models(content: Res<Content>, assets: Res<AssetServer>, mut models: ResMut<Models>) {
    let paths = content
        .props()
        .flat_map(|(_, prop)| prop.models_used())
        .chain(content.cover().iter().flat_map(|cover| &cover.models))
        .chain(
            content
                .structures()
                .flat_map(|(_, structure)| structure.parts.iter().map(|part| &part.model)),
        )
        .chain(content.items().filter_map(|(_, item)| item.model.as_ref()))
        .chain(content.crops().flat_map(|(_, crop)| &crop.models))
        .chain(content.shops().map(|(_, shop)| &shop.stall));
    for path in paths {
        if models.files.contains_key(path) {
            continue;
        }
        let file = format!("{MODELS_FOLDER}/{path}");
        models.scenes.insert(
            path.clone(),
            assets.load(GltfAssetLabel::Scene(0).from_asset(file.clone())),
        );
        models.files.insert(path.clone(), assets.load(file));
    }
}

/// Swaps the material each newly spawned model mesh came with for the
/// palette's.
fn use_palette_materials(
    meshes: Query<
        (Entity, &GltfMaterialName, &MeshMaterial3d<StandardMaterial>),
        Added<GltfMaterialName>,
    >,
    mut palettes: Palettes,
    mut commands: Commands,
) {
    for (entity, name, material) in &meshes {
        if let Some(drawn) = palettes.dress(name, &material.0) {
            drawn.put_on(&mut commands.entity(entity));
        }
    }
}

/// Follows the world's season, retinting the palette's materials when it
/// turns.
fn follow_season(
    clock: Res<LocalClock>,
    content: Res<Content>,
    mut season: ResMut<DrawnSeason>,
    looks: Res<PaletteLooks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut swaying: ResMut<Assets<SwayMaterial>>,
) {
    let Some(now) = clock.time().map(WorldTime::season) else {
        return;
    };
    if season.0 == now {
        return;
    }
    season.0 = now;
    let palette = content.palette();
    for ((name, _), handle) in &looks.tinted {
        if let (Some(color), Some(mut material)) =
            (palette.color(name, now), materials.get_mut(handle))
        {
            material.base_color = srgb(color);
        }
    }
    for ((name, _), handle) in &looks.swaying {
        if let (Some(color), Some(mut material)) =
            (palette.color(name, now), swaying.get_mut(handle))
        {
            material.base.base_color = srgb(color);
        }
    }
}
