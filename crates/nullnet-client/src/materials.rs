//! Shader-driven materials. Every visual is generated on the GPU, so the
//! client ships no image assets for nodes, lines or the backdrop.

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{ShaderRef, load_shader_library};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

pub struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/noise.wgsl");
        embedded_asset!(app, "shaders/node.wgsl");
        embedded_asset!(app, "shaders/link.wgsl");
        embedded_asset!(app, "shaders/background.wgsl");
        app.add_plugins((
            Material2dPlugin::<NodeMaterial>::default(),
            Material2dPlugin::<LinkMaterial>::default(),
            Material2dPlugin::<BackgroundMaterial>::default(),
        ));
    }
}

fn embedded_shader(path: AssetPath<'static>) -> ShaderRef {
    ShaderRef::Path(path.with_source("embedded"))
}

/// What a node on the map stands for; the shader draws each differently.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum NodeKind {
    /// A server, mainframe or facility: a hexagonal node.
    Host,
    /// A service on a host: a small round node.
    Subsystem,
    /// The trunk every network hangs off: a vertical beam.
    Backbone,
    /// A field of abandoned data caches: scattered fragments.
    CacheField,
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct NodeParams {
    /// The node's own colour.
    pub fill: Vec4,
    /// rgb: the firewall glow in its holder's colour, a: strength.
    pub ring: Vec4,
    /// x: kind, y: seed, z: hover highlight (0-1), w: pulse rate.
    pub shape: Vec4,
    /// x: quad half extent in node radii, y: height over width.
    pub quad: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct NodeMaterial {
    #[uniform(0)]
    pub params: NodeParams,
}

#[allow(dead_code)]
impl NodeMaterial {
    pub fn highlight(&self) -> f32 {
        self.params.shape.z
    }

    pub fn set_highlight(&mut self, value: f32) {
        self.params.shape.z = value;
    }
}

impl Material2d for NodeMaterial {
    fn fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!(
            "shaders/node.wgsl"
        )))
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct LinkParams {
    pub color: Vec4,
    /// x: length in world units, y: thickness in world units, z: packet
    /// speed in world units per second, w: seed.
    pub shape: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct LinkMaterial {
    #[uniform(0)]
    pub params: LinkParams,
}

impl Material2d for LinkMaterial {
    fn fragment_shader() -> ShaderRef {
        embedded_shader(AssetPath::from_path_buf(embedded_path!(
            "shaders/link.wgsl"
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
    pub grid: Vec4,
    pub fog_a: Vec4,
    pub fog_b: Vec4,
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
