//! Character skins: each look is laid from its layers into one image once
//! they have loaded, and every figure dressed that way shares its material.

use std::{borrow::Cow, collections::HashMap};

use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    image::{ImageLoaderSettings, ImageSampler},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use messoria_content::Look;
use messoria_shared::content::Content;

/// Folder the skin layers are in, within the assets.
const SKINS_FOLDER: &str = "skins";
/// Texels at least this opaque are drawn; the rest, such as the gaps in
/// hair over a head, are not.
const OPAQUE_ENOUGH: f32 = 0.5;

pub(crate) struct SkinsPlugin;

impl Plugin for SkinsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Skins>().add_systems(Update, lay_skins);
    }
}

#[derive(Resource, Default)]
struct Skins {
    /// Every layer asked for so far, by file.
    layers: HashMap<String, Handle<Image>>,
    /// The material of every look asked for so far.
    looks: Vec<(Look, Handle<StandardMaterial>)>,
    /// Skins whose layers have not all loaded yet.
    unlaid: Vec<(Look, Handle<Image>)>,
}

/// Dresses figures: gives the material for a look, whose skin is laid as
/// soon as its layers have loaded.
#[derive(SystemParam)]
pub(crate) struct Tailor<'w> {
    skins: ResMut<'w, Skins>,
    assets: Res<'w, AssetServer>,
    content: Res<'w, Content>,
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl Tailor<'_> {
    pub(crate) fn material(&mut self, look: &Look) -> Handle<StandardMaterial> {
        if let Some((_, material)) = self.skins.looks.iter().find(|(known, _)| known == look) {
            return material.clone();
        }
        for layer in look.layers() {
            if !self.skins.layers.contains_key(layer) {
                let handle = self
                    .assets
                    .load_builder()
                    // Layers are only read to lay skins, never drawn.
                    .with_settings(|settings: &mut ImageLoaderSettings| {
                        settings.asset_usage = RenderAssetUsages::MAIN_WORLD;
                    })
                    .load(format!("{SKINS_FOLDER}/{layer}"));
                self.skins.layers.insert(layer.to_owned(), handle);
            }
        }
        let (width, height) = self.content.characters().skin_size;
        // Transparent until laid, so a figure shows up dressed or not at all.
        let mut skin = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        // Texels stay crisp squares up close.
        skin.sampler = ImageSampler::nearest();
        let skin = self.images.add(skin);
        let material = self.materials.add(StandardMaterial {
            base_color_texture: Some(skin.clone()),
            alpha_mode: AlphaMode::Mask(OPAQUE_ENOUGH),
            perceptual_roughness: 0.85,
            reflectance: 0.15,
            ..default()
        });
        self.skins.unlaid.push((look.clone(), skin));
        self.skins.looks.push((look.clone(), material.clone()));
        material
    }
}

fn lay_skins(
    mut skins: ResMut<Skins>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    if skins.unlaid.is_empty() {
        return;
    }
    let Skins { layers, unlaid, .. } = &mut *skins;
    unlaid.retain(|(look, skin)| {
        let handles = look.layers().map(|layer| &layers[layer]);
        if let Some(failed) = handles
            .iter()
            .find(|handle| assets.load_state(**handle).is_failed())
        {
            warn!("a skin layer could not be loaded: {:?}", failed.path());
            return false;
        }
        let Some(loaded) = handles
            .iter()
            .map(|handle| images.get(*handle))
            .collect::<Option<Vec<&Image>>>()
        else {
            return true;
        };
        let size = loaded[0].size();
        let laid = lay(&loaded, look.hair_color);
        match (laid, images.get_mut(skin)) {
            (Some(texels), Some(mut image)) if image.size() == size => image.data = Some(texels),
            _ => warn!("the skin layers of {look:?} are not all the skin's size"),
        }
        false
    });
}

/// Lays `layers` over each other, bottom first, the last one tinted with
/// `hair_color`. `None` if any layer is not the size of the first.
fn lay(layers: &[&Image], hair_color: (f32, f32, f32)) -> Option<Vec<u8>> {
    let size = layers.first()?.size();
    let mut laid = vec![0_u8; 4 * size.element_product() as usize];
    let last = layers.len() - 1;
    for (index, layer) in layers.iter().enumerate() {
        if layer.size() != size {
            return None;
        }
        let layer = match layer.texture_descriptor.format {
            TextureFormat::Rgba8UnormSrgb => Cow::Borrowed(*layer),
            _ => Cow::Owned(layer.convert(TextureFormat::Rgba8UnormSrgb)?),
        };
        let tint = if index == last {
            [hair_color.0, hair_color.1, hair_color.2]
        } else {
            [1.0; 3]
        };
        let (laid_texels, _) = laid.as_chunks_mut::<4>();
        let (layer_texels, _) = layer.data.as_ref()?.as_chunks::<4>();
        for (below, above) in laid_texels.iter_mut().zip(layer_texels) {
            let alpha = f32::from(above[3]) / 255.0;
            if alpha == 0.0 {
                continue;
            }
            for channel in 0..3 {
                let over = f32::from(above[channel]) * tint[channel];
                let blended = over * alpha + f32::from(below[channel]) * (1.0 - alpha);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "clamped to the range of a byte"
                )]
                let byte = blended.round().clamp(0.0, 255.0) as u8;
                below[channel] = byte;
            }
            below[3] = below[3].max(above[3]);
        }
    }
    Some(laid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(texel: [u8; 4]) -> Image {
        Image::new_fill(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &texel,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
    }

    #[test]
    fn hair_is_tinted_and_laid_over_the_rest() {
        let body = image([200, 150, 100, 255]);
        let outfit = image([0, 0, 0, 0]);
        let hair = image([200, 200, 200, 255]);
        let laid = lay(&[&body, &outfit, &hair], (0.5, 0.25, 1.0)).expect("same sizes");
        assert_eq!(&laid[..4], &[100, 50, 200, 255]);
    }

    #[test]
    fn transparent_layers_leave_what_is_below() {
        let body = image([10, 20, 30, 255]);
        let clear = image([0, 0, 0, 0]);
        let laid = lay(&[&body, &clear, &clear], (1.0, 1.0, 1.0)).expect("same sizes");
        assert_eq!(&laid[..4], &[10, 20, 30, 255]);
    }
}
