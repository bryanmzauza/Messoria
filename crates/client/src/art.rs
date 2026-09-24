//! Models loaded from glTF files, drawn in the palette's colors for the
//! season.
//!
//! Every mesh whose material has a name in the palette is drawn with one
//! material shared by everything of that name, instead of the one its file
//! came with. When the season turns, those shared materials change color and
//! everything drawn with them follows, from leaves to grass.

use std::collections::HashMap;

use bevy::{
    camera::visibility::RenderLayers,
    gltf::{Gltf, GltfAssetLabel, GltfMaterialName},
    prelude::*,
    world_serialization::{WorldAsset, WorldAssetRoot},
};
use messoria_calendar::{Season, WorldTime};
use messoria_content::{CropDef, ItemId, ItemKind, Palette, Rgb};
use messoria_shared::content::Content;

use crate::clock::LocalClock;

/// Folder of the models, inside the assets folder.
const MODELS_FOLDER: &str = "models";

pub(crate) struct ArtPlugin;

impl Plugin for ArtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawnSeason>()
            .init_resource::<PaletteMaterials>()
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

/// The materials drawn in palette colors, one for each name used so far.
#[derive(Resource, Default)]
pub(crate) struct PaletteMaterials(HashMap<String, Handle<StandardMaterial>>);

impl PaletteMaterials {
    /// The shared material for `name`, created on first use, or `None` if
    /// the palette has no color for it.
    pub(crate) fn get(
        &mut self,
        name: &str,
        palette: &Palette,
        season: Season,
        materials: &mut Assets<StandardMaterial>,
    ) -> Option<Handle<StandardMaterial>> {
        if let Some(material) = self.0.get(name) {
            return Some(material.clone());
        }
        let color = palette.color(name, season)?;
        let material = materials.add(StandardMaterial {
            base_color: srgb(color),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        });
        self.0.insert(name.to_owned(), material.clone());
        Some(material)
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
    mut meshes: Query<
        (&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>),
        Added<GltfMaterialName>,
    >,
    content: Res<Content>,
    season: Res<DrawnSeason>,
    mut shared: ResMut<PaletteMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (name, mut material) in &mut meshes {
        if let Some(palette) = shared.get(name, content.palette(), season.0, &mut materials) {
            material.0 = palette;
        }
    }
}

/// Follows the world's season, recoloring the palette's materials when it
/// turns.
fn follow_season(
    clock: Res<LocalClock>,
    content: Res<Content>,
    mut season: ResMut<DrawnSeason>,
    shared: Res<PaletteMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(now) = clock.time().map(WorldTime::season) else {
        return;
    };
    if season.0 == now {
        return;
    }
    season.0 = now;
    for (name, handle) in &shared.0 {
        if let (Some(color), Some(mut material)) = (
            content.palette().color(name, now),
            materials.get_mut(handle),
        ) {
            material.base_color = srgb(color);
        }
    }
}
