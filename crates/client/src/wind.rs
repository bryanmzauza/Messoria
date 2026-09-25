//! The wind: foliage that sways.
//!
//! Materials the palette names as swaying (leaves, grass, crops) are drawn
//! with [`SwayMaterial`], the standard material whose vertices the wind
//! pushes (`shaders/sway.wgsl`), in the main pass and in the passes for
//! depth, shadows and motion alike, so that shadows and anti-aliasing move
//! with the leaves.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

/// The shaders that place swaying vertices, and the module they share.
const SWAY_SHADER: &str = "shaders/sway.wgsl";
const SWAY_PREPASS_SHADER: &str = "shaders/sway_prepass.wgsl";
const WIND_SHADER: &str = "shaders/wind.wgsl";

/// Foliage's material: the standard one, its vertices pushed by the wind.
pub(crate) type SwayMaterial = ExtendedMaterial<StandardMaterial, Sway>;

pub(crate) struct WindPlugin;

impl Plugin for WindPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SwayMaterial>::default())
            .add_systems(Startup, load_wind)
            .add_systems(Update, blow);
    }
}

/// How far the tops of plants drawn with a material sway, and the wind's
/// clock.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub(crate) struct Sway {
    #[uniform(100)]
    wind: Wind,
}

impl Sway {
    /// Sways the tops of plants by up to `strength` meters.
    pub(crate) fn new(strength: f32) -> Self {
        Self {
            wind: Wind {
                strength,
                time: 0.0,
                previous_time: 0.0,
                padding: 0.0,
            },
        }
    }
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct Wind {
    strength: f32,
    time: f32,
    previous_time: f32,
    padding: f32,
}

impl MaterialExtension for Sway {
    fn vertex_shader() -> ShaderRef {
        SWAY_SHADER.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        SWAY_PREPASS_SHADER.into()
    }
}

/// Keeps the module the sway shaders import loaded.
#[derive(Resource)]
struct WindShader(#[expect(dead_code, reason = "held only to stay loaded")] Handle<Shader>);

fn load_wind(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(WindShader(assets.load(WIND_SHADER)));
}

/// Moves the wind's clock on in every swaying material.
fn blow(time: Res<Time>, mut materials: ResMut<Assets<SwayMaterial>>) {
    let now = time.elapsed_secs_wrapped();
    let before = (now - time.delta_secs()).max(0.0);
    for (_, material) in materials.iter_mut() {
        material.extension.wind.time = now;
        material.extension.wind.previous_time = before;
    }
}
