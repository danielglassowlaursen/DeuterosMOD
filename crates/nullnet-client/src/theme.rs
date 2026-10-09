//! The look of the client: Deus Ex: Human Revolution's gold on black, with
//! angular panels whose top left and bottom right corners are cut off. A
//! shader draws the panels, so the client still ships no images. The text
//! is Chakra Petch (SIL Open Font License) and the icons are a cut-down
//! Lucide (ISC); both fonts are embedded in the build, with their licences
//! beside them in `assets/fonts`.

use bevy::asset::{AssetId, AssetPath, embedded_asset, embedded_path, uuid_handle};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const REGULAR: &[u8] = include_bytes!("../assets/fonts/ChakraPetch-Regular.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf");
const ICONS: &[u8] = include_bytes!("../assets/fonts/lucide-subset.ttf");

/// Chakra Petch SemiBold, for headings and buttons. The regular weight
/// replaces Bevy's default font, so plain text needs no handle.
pub const BOLD_FONT: Handle<Font> = uuid_handle!("6b0f4d6e-2c55-4f0e-9d0f-6a8c3e1a9b21");
/// The icon glyphs in [`crate::icons`].
pub const ICON_FONT: Handle<Font> = uuid_handle!("0f6f1b9e-6a3a-4b8f-a0a8-2f7d2c5e8d43");

pub const GOLD: Color = Color::srgb(0.93, 0.67, 0.25);
pub const GOLD_BRIGHT: Color = Color::srgb(1.0, 0.83, 0.47);
pub const GOLD_DIM: Color = Color::srgba(0.93, 0.67, 0.25, 0.4);
pub const FG: Color = Color::srgb(0.94, 0.9, 0.81);
pub const MUTED: Color = Color::srgba(0.74, 0.66, 0.52, 0.9);
pub const WARN: Color = Color::srgb(0.96, 0.36, 0.27);
/// The one cold colour Deus Ex sets against its gold.
pub const GOOD: Color = Color::srgb(0.5, 0.85, 0.93);
/// Dark text on a gold button.
pub const INK: Color = Color::srgb(0.07, 0.05, 0.02);

/// A side panel: the window, the outliner, the turn box, the guide.
pub const PANEL: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f11");
/// A panel over everything: the story, the help, a replay.
pub const OVERLAY: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f12");
/// The bar along the top of the screen.
pub const BAR: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f13");
/// A button, as it rests, under the cursor, and pressed or chosen.
pub const BUTTON: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f14");
pub const BUTTON_HOVER: Handle<PanelMaterial> =
    uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f15");
pub const BUTTON_ON: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f16");
/// The one button that matters most on screen: Hand in.
pub const PRIMARY: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f17");
pub const PRIMARY_HOVER: Handle<PanelMaterial> =
    uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f18");
/// The tooltip under the cursor.
pub const TIP: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f19");
/// A row in a list: bare at rest, lit under the cursor and when chosen.
pub const ROW: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f1a");
pub const ROW_HOVER: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f1b");
pub const ROW_ON: Handle<PanelMaterial> = uuid_handle!("2d1e7f0c-8b3a-4e65-9f1d-4c7a0b9e2f1c");

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/panel.wgsl");
        app.add_plugins(UiMaterialPlugin::<PanelMaterial>::default());

        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        for (id, bytes) in [
            (AssetId::default(), REGULAR),
            (BOLD_FONT.id(), SEMIBOLD),
            (ICON_FONT.id(), ICONS),
        ] {
            fonts
                .insert(id, Font::from_bytes(bytes.to_vec()))
                .expect("a font handle with no generation");
        }

        let mut panels = app.world_mut().resource_mut::<Assets<PanelMaterial>>();
        let dark = Color::srgba(0.035, 0.028, 0.018, 0.93);
        for (handle, material) in [
            (
                PANEL,
                PanelMaterial::new(dark, GOLD_DIM, GOLD, 14.0, 1.0, 1.0, 0.0),
            ),
            (
                OVERLAY,
                PanelMaterial::new(
                    Color::srgba(0.03, 0.024, 0.015, 0.97),
                    GOLD,
                    GOLD_BRIGHT,
                    20.0,
                    1.0,
                    1.0,
                    0.0,
                ),
            ),
            (
                BAR,
                PanelMaterial::new(
                    Color::srgba(0.03, 0.024, 0.015, 0.95),
                    GOLD_DIM,
                    GOLD,
                    0.0,
                    1.0,
                    0.6,
                    0.0,
                ),
            ),
            (
                BUTTON,
                PanelMaterial::new(
                    Color::srgba(0.09, 0.07, 0.035, 0.95),
                    GOLD_DIM,
                    GOLD_DIM,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
            (
                BUTTON_HOVER,
                PanelMaterial::new(
                    Color::srgba(0.3, 0.21, 0.07, 0.95),
                    GOLD,
                    GOLD_BRIGHT,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
            (
                BUTTON_ON,
                PanelMaterial::new(
                    Color::srgba(0.48, 0.33, 0.1, 0.95),
                    GOLD_BRIGHT,
                    GOLD_BRIGHT,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
            (
                PRIMARY,
                PanelMaterial::new(GOLD, GOLD_BRIGHT, GOLD_BRIGHT, 8.0, 1.0, 0.0, 0.0),
            ),
            (
                PRIMARY_HOVER,
                PanelMaterial::new(GOLD_BRIGHT, Color::WHITE, Color::WHITE, 8.0, 1.0, 0.0, 0.0),
            ),
            (
                ROW,
                PanelMaterial::new(Color::NONE, Color::NONE, Color::NONE, 6.0, 1.0, 0.0, 0.0),
            ),
            (
                ROW_HOVER,
                PanelMaterial::new(
                    Color::srgba(0.93, 0.67, 0.25, 0.14),
                    GOLD_DIM,
                    GOLD,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
            (
                ROW_ON,
                PanelMaterial::new(
                    Color::srgba(0.93, 0.67, 0.25, 0.24),
                    GOLD,
                    GOLD_BRIGHT,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
            (
                TIP,
                PanelMaterial::new(
                    Color::srgba(0.02, 0.016, 0.01, 0.97),
                    GOLD,
                    GOLD_BRIGHT,
                    6.0,
                    1.0,
                    0.0,
                    0.0,
                ),
            ),
        ] {
            panels
                .insert(handle.id(), material)
                .expect("a material handle with no generation");
        }
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct PanelParams {
    pub fill: Vec4,
    pub line: Vec4,
    pub accent: Vec4,
    /// x: corner cut (px), y: line width (px), z: scanlines, w: header (px).
    pub shape: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PanelMaterial {
    #[uniform(0)]
    pub params: PanelParams,
}

impl PanelMaterial {
    fn new(
        fill: Color,
        line: Color,
        accent: Color,
        cut: f32,
        width: f32,
        scanlines: f32,
        header: f32,
    ) -> Self {
        let linear = |c: Color| LinearRgba::from(c).to_vec4();
        PanelMaterial {
            params: PanelParams {
                fill: linear(fill),
                line: linear(line),
                accent: linear(accent),
                shape: Vec4::new(cut, width, scanlines, header),
            },
        }
    }
}

impl UiMaterial for PanelMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/panel.wgsl")).with_source("embedded"),
        )
    }
}

/// Text in the regular weight.
pub fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

/// Text in the heavier weight, for headings and buttons.
pub fn bold(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont::from_font_size(size).with_font(BOLD_FONT),
        TextColor(color),
    )
}

/// An icon from [`crate::icons`].
pub fn icon(glyph: char, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(glyph.to_string()),
        TextFont::from_font_size(size).with_font(ICON_FONT),
        TextColor(color),
    )
}
