//! Shader-driven materials. Every visual is generated on the GPU, so the
//! client ships no image assets for planets, stars or the backdrop.

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{ShaderRef, load_shader_library};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

pub struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/noise.wgsl");
        embedded_asset!(app, "shaders/planet.wgsl");
        embedded_asset!(app, "shaders/background.wgsl");
        app.add_plugins((
            Material2dPlugin::<PlanetMaterial>::default(),
            Material2dPlugin::<BackgroundMaterial>::default(),
        ));
    }
}

fn embedded_shader(path: AssetPath<'static>) -> ShaderRef {
    ShaderRef::Path(path.with_source("embedded"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    Rocky,
    Gas,
    Earth,
    Ice,
    Star,
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct PlanetParams {
    pub color_a: Vec4,
    pub color_b: Vec4,
    pub color_c: Vec4,
    /// rgb: atmosphere colour, a: strength.
    pub atmosphere: Vec4,
    /// x: surface kind, y: noise scale, z: spin speed, w: seed.
    pub shape: Vec4,
    /// x: ring inner radius, y: ring outer radius (0 = none), z: ring tilt,
    /// w: hover highlight (0-1).
    pub extra: Vec4,
    /// x: quad half extent in planet radii.
    pub quad: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PlanetMaterial {
    #[uniform(0)]
    pub params: PlanetParams,
}

impl PlanetMaterial {
    pub fn highlight(&self) -> f32 {
        self.params.extra.w
    }

    pub fn set_highlight(&mut self, value: f32) {
        self.params.extra.w = value;
    }
}

impl Material2d for PlanetMaterial {
    fn fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!(
            "shaders/planet.wgsl"
        )))
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct BackgroundParams {
    /// xy: parallax offset in world units.
    pub offset: Vec4,
    pub nebula_a: Vec4,
    pub nebula_b: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct BackgroundMaterial {
    #[uniform(0)]
    pub params: BackgroundParams,
}

impl Material2d for BackgroundMaterial {
    fn fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!(
            "shaders/background.wgsl"
        )))
    }
}

/// Converts an sRGB hex colour to the linear vector the shaders expect.
pub fn linear(hex: &str) -> Vec4 {
    let color = Srgba::hex(hex).expect("colour constants are valid hex");
    LinearRgba::from(color).to_vec4()
}
