//! How the ground is painted: pixel-art textures laid over the terrain in
//! world space, one set per ground material, tinted by the season.
//!
//! The textures are painted in spring's colors, 32 texels to the meter, four
//! variants of each, in one strip (`textures/ground.png`). They become one
//! array texture with mipmaps: crisp squares up close, calm in the distance.
//! The shader (`shaders/ground.wgsl`) picks the ground material texel by
//! texel from the materials at the triangle's corners, so borders between
//! grass and soil are drawn in pixels rather than blended; paving on flat
//! ground turns to rock on steep faces, and grass too steep to hold turns to
//! soil. In winter snow lies where grass grew.

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor,
        TextureViewDimension,
    },
    shader::ShaderRef,
};
use messoria_calendar::Season;
use messoria_shared::content::Content;
use messoria_voxel::Material;

use crate::art::{DrawnSeason, srgb};

/// The strip of ground textures, and the shader that lays them.
const TEXTURES: &str = "textures/ground.png";
const SHADER: &str = "shaders/ground.wgsl";
/// Side of one texture, in texels.
const TEXTURE_SIZE: u32 = 32;

/// The ground's material: the standard one, lit as usual, with its color
/// painted by the ground shader.
pub(crate) type GroundMaterial = ExtendedMaterial<StandardMaterial, Ground>;

pub(crate) struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default())
            .add_systems(Startup, create_ground_material)
            .add_systems(Update, (stack_textures, follow_the_season));
    }
}

/// What the ground shader paints with.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub(crate) struct Ground {
    #[uniform(100)]
    look: GroundLook,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    textures: Handle<Image>,
}

impl MaterialExtension for Ground {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// The season's tint for each ground material, in the order of
/// [`Material::ALL`], over the spring colors the textures are painted in,
/// and whether snow covers the grass.
#[derive(ShaderType, Debug, Clone)]
struct GroundLook {
    tints: [Vec4; 4],
    snow: u32,
}

/// The ground material every terrain chunk is drawn with.
#[derive(Resource)]
pub(crate) struct GroundLooks {
    pub material: Handle<GroundMaterial>,
    /// The strip as loaded, until it is stacked into the array texture.
    strip: Option<Handle<Image>>,
}

fn create_ground_material(
    content: Res<Content>,
    season: Res<DrawnSeason>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<GroundMaterial>>,
    mut commands: Commands,
) {
    // Stacked from the strip once it has loaded; until then, plain white.
    let mut blank = Image::new_fill(
        Extent3d::default(),
        TextureDimension::D2,
        &[255; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    blank.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    let textures = images.add(blank);
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.92,
            reflectance: 0.2,
            ..default()
        },
        extension: Ground {
            look: look_in(&content, season.0),
            textures,
        },
    });
    commands.insert_resource(GroundLooks {
        material,
        strip: Some(assets.load(TEXTURES)),
    });
}

/// Once the strip has loaded, stacks its textures into the ground's array
/// texture, with mipmaps.
fn stack_textures(
    mut looks: ResMut<GroundLooks>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<GroundMaterial>>,
) {
    let Some(strip) = looks.strip.as_ref() else {
        return;
    };
    let Some(loaded) = images.get(strip) else {
        return;
    };
    let stacked = match stack(loaded) {
        Ok(stacked) => stacked,
        Err(problem) => {
            error!("{TEXTURES} cannot be used: {problem}");
            looks.strip = None;
            return;
        }
    };
    let stacked = images.add(stacked);
    if let Some(mut material) = materials.get_mut(&looks.material) {
        material.extension.textures = stacked;
    }
    looks.strip = None;
}

fn follow_the_season(
    content: Res<Content>,
    season: Res<DrawnSeason>,
    looks: Res<GroundLooks>,
    mut materials: ResMut<Assets<GroundMaterial>>,
) {
    if !season.is_changed() {
        return;
    }
    if let Some(mut material) = materials.get_mut(&looks.material) {
        material.extension.look = look_in(&content, season.0);
    }
}

fn look_in(content: &Content, season: Season) -> GroundLook {
    let palette = content.palette();
    let tints = Material::ALL.map(|material| {
        let now = srgb(palette.ground(material, season)).to_linear();
        let spring = srgb(palette.ground(material, Season::Spring)).to_linear();
        Vec4::new(
            now.red / spring.red.max(f32::EPSILON),
            now.green / spring.green.max(f32::EPSILON),
            now.blue / spring.blue.max(f32::EPSILON),
            1.0,
        )
    });
    GroundLook {
        tints,
        snow: u32::from(season == Season::Winter),
    }
}

/// Cuts a strip of square textures into the layers of an array texture and
/// gives each layer its mipmaps, averaged in linear light.
fn stack(strip: &Image) -> Result<Image, String> {
    let strip = strip
        .convert(TextureFormat::Rgba8UnormSrgb)
        .ok_or("its format cannot be converted")?;
    let (width, height) = (strip.width(), strip.height());
    if height != TEXTURE_SIZE || width % TEXTURE_SIZE != 0 {
        return Err(format!(
            "it must be a row of {TEXTURE_SIZE}×{TEXTURE_SIZE} textures"
        ));
    }
    let data = strip.data.as_ref().ok_or("it has no pixels")?;
    let layers = width / TEXTURE_SIZE;
    let levels = TEXTURE_SIZE.ilog2() + 1;
    let side = TEXTURE_SIZE as usize;

    let mut stacked = Vec::new();
    for layer in 0..layers as usize {
        // The layer's texels in linear light, row by row.
        let mut level: Vec<[f32; 4]> = (0..side * side)
            .map(|texel| {
                let (x, y) = (layer * side + texel % side, texel / side);
                let at = 4 * (y * width as usize + x);
                let pixel = &data[at..at + 4];
                let linear = Color::srgba_u8(pixel[0], pixel[1], pixel[2], pixel[3]).to_linear();
                linear.to_f32_array()
            })
            .collect();
        let mut level_side = side;
        loop {
            for texel in &level {
                let color = Color::linear_rgba(texel[0], texel[1], texel[2], texel[3]).to_srgba();
                stacked.extend(color.to_u8_array());
            }
            if level_side == 1 {
                break;
            }
            let half = level_side / 2;
            level = (0..half * half)
                .map(|texel| {
                    let (x, y) = (2 * (texel % half), 2 * (texel / half));
                    let corners = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
                        .map(|(x, y)| level[y * level_side + x]);
                    [0, 1, 2, 3].map(|channel| {
                        corners.iter().map(|corner| corner[channel]).sum::<f32>() / 4.0
                    })
                })
                .collect();
            level_side = half;
        }
    }

    let mut image = Image::new_uninit(
        Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: layers,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(stacked);
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    // Texels stay square up close; farther away the mipmaps take over.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_strip_is_stacked_into_layers_with_their_mipmaps() {
        let strip = Image::new_fill(
            Extent3d {
                width: 2 * TEXTURE_SIZE,
                height: TEXTURE_SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[200, 100, 50, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );
        let stacked = stack(&strip).expect("a strip of two textures");
        assert_eq!(stacked.texture_descriptor.size.depth_or_array_layers, 2);
        // 32² + 16² + ... + 1² texels per layer.
        let per_layer: u32 = (0..6).map(|level| (TEXTURE_SIZE >> level).pow(2)).sum();
        assert_eq!(
            stacked.data.as_ref().map(Vec::len),
            Some((2 * per_layer * 4) as usize)
        );
        // A flat color stays the same color all the way down.
        let data = stacked.data.as_ref().expect("stacked pixels");
        assert_eq!(&data[data.len() - 4..], &[200, 100, 50, 255]);
    }
}
