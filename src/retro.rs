use crate::draw::{DitherConfig, FogConfig};
use embedded_graphics_core::pixelcolor::Rgb565;

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
        }
    }

    /// Doom-leaning preset for coarse affine textures without fog.
    pub const fn doom_walkable() -> Self {
        Self {
            fog: None,
            dither: Some(DitherConfig { intensity: 20 }),
            vertex_snap_bits: 0,
            texture_mapping: TextureMapping::Affine,
        }
    }

    /// PSX-leaning preset: snapped vertices + affine textures + visible dither.
    pub fn psx() -> Self {
        Self {
            fog: Some(FogConfig::new(Rgb565::new(2, 2, 4), 6.0, 20.0)),
            dither: Some(DitherConfig::new(72)),
            vertex_snap_bits: 6,
            texture_mapping: TextureMapping::Affine,
        }
    }
}
