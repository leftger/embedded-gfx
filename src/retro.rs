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

/// Palette quantization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Off,
    /// 3-3-2 bit palette approximation (256 colors).
    Rgb332,
}

impl PaletteMode {
    #[inline]
    pub fn apply(self, color: Rgb565) -> Rgb565 {
        match self {
            PaletteMode::Off => color,
            PaletteMode::Rgb332 => {
                let r3 = ((color.r() as u16 * 7 + 15) / 31) as u8;
                let g3 = ((color.g() as u16 * 7 + 31) / 63) as u8;
                let b2 = ((color.b() as u16 * 3 + 15) / 31) as u8;
                let r = (r3 as u16 * 31 / 7) as u8;
                let g = (g3 as u16 * 63 / 7) as u8;
                let b = (b2 as u16 * 31 / 3) as u8;
                Rgb565::new(r, g, b)
            }
        }
    }
}

/// Procedural sky background rendered before world geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyConfig {
    pub top_color: Rgb565,
    pub bottom_color: Rgb565,
    pub stripe_color: Rgb565,
    pub stripe_strength: u8,
    pub stripe_width: u8,
}

impl SkyConfig {
    pub const fn retro_blue() -> Self {
        Self {
            top_color: Rgb565::new(6, 16, 31),
            bottom_color: Rgb565::new(1, 4, 12),
            stripe_color: Rgb565::new(18, 30, 31),
            stripe_strength: 18,
            stripe_width: 16,
        }
    }
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
    /// Optional palette quantization.
    pub palette_mode: PaletteMode,
    /// Optional procedural sky.
    pub sky: Option<SkyConfig>,
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
            palette_mode: PaletteMode::Off,
            sky: None,
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
            palette_mode: PaletteMode::Rgb332,
            sky: Some(SkyConfig::retro_blue()),
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
            palette_mode: PaletteMode::Rgb332,
            sky: Some(SkyConfig::retro_blue()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_off_is_identity() {
        let c = Rgb565::new(17, 45, 23);
        assert_eq!(PaletteMode::Off.apply(c), c);
    }

    #[test]
    fn palette_rgb332_quantizes_channels() {
        let c = Rgb565::new(17, 45, 23);
        let q = PaletteMode::Rgb332.apply(c);
        // Blue channel is reduced to 2 bits; red/green map to 3 bits.
        assert_eq!(q, Rgb565::new(17, 45, 20));
    }

    #[test]
    fn screen_tint_strength_extremes() {
        let base = Rgb565::new(7, 20, 5);
        let tint = Rgb565::new(31, 0, 31);

        let off = ScreenTint {
            color: tint,
            strength: 0,
        };
        assert_eq!(off.apply(base), base);

        let full = ScreenTint {
            color: tint,
            strength: 255,
        };
        assert_eq!(full.apply(base), tint);
    }

    #[test]
    fn doom_walkable_preset_matches_expected_profile() {
        let s = RetroStyle::doom_walkable();
        assert!(s.fog.is_none());
        assert_eq!(s.vertex_snap_bits, 0);
        assert_eq!(s.texture_mapping, TextureMapping::Affine);
        assert_eq!(s.light_levels, LightLevels::Doom32);
        assert_eq!(s.stipple_mode, StippleMode::Off);
        assert_eq!(s.palette_mode, PaletteMode::Rgb332);
        assert!(s.sky.is_some());
        assert!(s.dither.is_some());
    }

    #[test]
    fn psx_preset_enables_snap_and_fog() {
        let s = RetroStyle::psx();
        assert_eq!(s.vertex_snap_bits, 6);
        assert_eq!(s.texture_mapping, TextureMapping::Affine);
        assert_eq!(s.light_levels, LightLevels::Linear);
        assert_eq!(s.palette_mode, PaletteMode::Rgb332);
        assert!(s.fog.is_some());
        assert!(s.dither.is_some());
        assert!(s.sky.is_some());
    }
}
