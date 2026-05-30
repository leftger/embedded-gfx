use crate::draw::{DitherConfig, FogConfig};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::pixelcolor::RgbColor;

/// Sector brightness model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightLevels {
    /// Direct linear brightness scale.
    Linear,
    /// Doom-like 32-step distance banding.
    Doom32,
}

/// Screen-space stipple transparency mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StippleMode {
    Off,
    Checkerboard,
}

/// Full-screen tint blended onto final raster colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenTint {
    pub color: Rgb565,
    /// Blend strength in [0, 255].
    pub strength: u8,
}

impl ScreenTint {
    #[inline]
    pub fn apply(&self, base: Rgb565) -> Rgb565 {
        let a = self.strength as u16;
        let inv = 255u16.saturating_sub(a);
        let r = ((base.r() as u16 * inv + self.color.r() as u16 * a) / 255) as u8;
        let g = ((base.g() as u16 * inv + self.color.g() as u16 * a) / 255) as u8;
        let b = ((base.b() as u16 * inv + self.color.b() as u16 * a) / 255) as u8;
        Rgb565::new(r, g, b)
    }
}

/// Texture coordinate interpolation style for textured triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureMapping {
    /// Perspective-correct interpolation using clip-space W.
    PerspectiveCorrect,
    /// Affine interpolation in screen space (retro "texture swim").
    Affine,
}

/// Coarse visual-style controls for retro rendering presets.
#[derive(Debug, Clone, Copy)]
pub struct RetroStyle {
    /// Optional depth fog.
    pub fog: Option<FogConfig>,
    /// Optional ordered dithering.
    pub dither: Option<DitherConfig>,
    /// NDC snap precision in fractional bits. `0` disables snapping.
    pub vertex_snap_bits: u8,
    /// UV interpolation mode for textured surfaces.
    pub texture_mapping: TextureMapping,
    /// Sector light behavior.
    pub light_levels: LightLevels,
    /// Optional checkerboard stippling for fake transparency.
    pub stipple_mode: StippleMode,
    /// Optional full-screen tint.
    pub screen_tint: Option<ScreenTint>,
}

impl Default for RetroStyle {
    fn default() -> Self {
        Self::modern()
    }
}

impl RetroStyle {
    /// Neutral defaults: no extra post-process and perspective-correct texturing.
    pub const fn modern() -> Self {
        Self {
            fog: None,
            dither: None,
            vertex_snap_bits: 0,
            texture_mapping: TextureMapping::PerspectiveCorrect,
            light_levels: LightLevels::Linear,
            stipple_mode: StippleMode::Off,
            screen_tint: None,
        }
    }

    /// Doom-leaning preset for coarse affine textures without fog.
    pub const fn doom_walkable() -> Self {
        Self {
            fog: None,
            dither: Some(DitherConfig { intensity: 20 }),
            vertex_snap_bits: 0,
            texture_mapping: TextureMapping::Affine,
            light_levels: LightLevels::Doom32,
            stipple_mode: StippleMode::Off,
            screen_tint: None,
        }
    }

    /// PSX-leaning preset: snapped vertices + affine textures + visible dither.
    pub fn psx() -> Self {
        Self {
            fog: Some(FogConfig::new(Rgb565::new(2, 2, 4), 6.0, 20.0)),
            dither: Some(DitherConfig::new(72)),
            vertex_snap_bits: 6,
            texture_mapping: TextureMapping::Affine,
            light_levels: LightLevels::Linear,
            stipple_mode: StippleMode::Off,
            screen_tint: None,
        }
    }
}
