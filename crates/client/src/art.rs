//! Models loaded from glTF files, drawn in the palette's colors for the
//! season.
//!
//! Every mesh whose material has a name in the palette is drawn with one
//! material shared by everything of that name, instead of the one its file
//! came with. When the season turns, those shared materials change color and
//! everything drawn with them follows, from leaves to grass.

use std::collections::HashMap;

use bevy::{
    gltf::{Gltf, GltfAssetLabel, GltfMaterialName},
    prelude::*,
    world_serialization::{WorldAsset, WorldAssetRoot},
};
use messoria_calendar::{Season, WorldTime};
use messoria_content::{Palette, Rgb};
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
            .add_systems(PreUpdate, (follow_season, use_palette_materials));
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

fn load_models(content: Res<Content>, assets: Res<AssetServer>, mut models: ResMut<Models>) {
    let paths = content
        .props()
        .flat_map(|(_, prop)| &prop.models)
        .chain(content.cover().iter().flat_map(|cover| &cover.models));
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
