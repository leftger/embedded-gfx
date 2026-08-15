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

/// Configuration for distance-based texture LOD and flat-color shedding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureLodConfig {
    /// Distance below which textures render with full detail.
    pub near_distance: f32,
    /// Distance at or beyond which textures are dropped and rendered as solid color.
    pub far_distance: f32,
    /// Fallback flat material color when texture is dropped.
    pub fallback_color: Rgb565,
}

impl TextureLodConfig {
    /// Create a new texture LOD threshold configuration.
    pub const fn new(near_distance: f32, far_distance: f32, fallback_color: Rgb565) -> Self {
        Self {
            near_distance,
            far_distance,
            fallback_color,
        }
    }

    /// Evaluates whether a triangle at depth `z` should drop texture sampling.
    #[inline]
    pub fn should_drop_texture(&self, z: f32) -> bool {
        z >= self.far_distance
    }

    /// Evaluates if `z` is in the transition crossfade band [near_distance, far_distance).
    #[inline]
    pub fn is_in_transition(&self, z: f32) -> bool {
        z >= self.near_distance && z < self.far_distance
    }

    /// Compute blend factor [0.0, 1.0] toward flat color.
    #[inline]
    pub fn flat_blend_factor(&self, z: f32) -> f32 {
        if z <= self.near_distance {
            0.0
        } else if z >= self.far_distance {
            1.0
        } else {
            (z - self.near_distance) / (self.far_distance - self.near_distance)
        }
    }
}

/// Dynamic color palette with runtime cycling and animation support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimatedPalette<const N: usize> {
    /// Base palette colors.
    pub colors: [Rgb565; N],
    /// Cycle start index (inclusive).
    pub cycle_start: usize,
    /// Cycle end index (exclusive).
    pub cycle_end: usize,
    /// Current cycle offset.
    pub offset: usize,
}

impl<const N: usize> AnimatedPalette<N> {
    /// Create a new animated palette from static colors.
    pub const fn new(colors: [Rgb565; N]) -> Self {
        Self {
            colors,
            cycle_start: 0,
            cycle_end: N,
            offset: 0,
        }
    }

    /// Constrain palette cycling to a sub-range [start, end).
    pub const fn with_cycle_range(mut self, start: usize, end: usize) -> Self {
        self.cycle_start = start;
        self.cycle_end = if end <= N { end } else { N };
        self
    }

    /// Step the palette cycle by `steps` positions.
    pub fn step(&mut self, steps: usize) {
        let range_len = self.cycle_end.saturating_sub(self.cycle_start);
        if range_len > 1 {
            self.offset = (self.offset + steps) % range_len;
        }
    }

    /// Look up a color by palette index, applying the active animation offset if within cycle range.
    #[inline]
    pub fn get_color(&self, index: usize) -> Rgb565 {
        if index >= N {
            return Rgb565::BLACK;
        }
        if index >= self.cycle_start && index < self.cycle_end {
            let range_len = self.cycle_end - self.cycle_start;
            let cycled_idx =
                self.cycle_start + (index - self.cycle_start + self.offset) % range_len;
            self.colors[cycled_idx]
        } else {
            self.colors[index]
        }
    }
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

    #[test]
    fn test_texture_lod_config() {
        let lod = TextureLodConfig::new(50.0, 150.0, Rgb565::new(16, 32, 16));
        assert!(!lod.should_drop_texture(40.0));
        assert!(!lod.should_drop_texture(100.0));
        assert!(lod.should_drop_texture(150.0));
        assert!(lod.should_drop_texture(200.0));

        assert!(!lod.is_in_transition(40.0));
        assert!(lod.is_in_transition(100.0));
        assert!(!lod.is_in_transition(150.0));

        assert_eq!(lod.flat_blend_factor(50.0), 0.0);
        assert_eq!(lod.flat_blend_factor(100.0), 0.5);
        assert_eq!(lod.flat_blend_factor(150.0), 1.0);
    }

    #[test]
    fn test_animated_palette() {
        let colors = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE];
        let mut pal = AnimatedPalette::new(colors).with_cycle_range(1, 3);

        assert_eq!(pal.get_color(0), Rgb565::RED);
        assert_eq!(pal.get_color(1), Rgb565::GREEN);
        assert_eq!(pal.get_color(2), Rgb565::BLUE);
        assert_eq!(pal.get_color(3), Rgb565::WHITE);

        pal.step(1);
        assert_eq!(pal.get_color(0), Rgb565::RED); // Uncycled
        assert_eq!(pal.get_color(1), Rgb565::BLUE); // Cycled
        assert_eq!(pal.get_color(2), Rgb565::GREEN); // Cycled
        assert_eq!(pal.get_color(3), Rgb565::WHITE); // Uncycled
    }
}
