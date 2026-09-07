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

/// Cycling direction for retro animated palette slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleDirection {
    /// Forward looping cycle (0 -> 1 -> 2 -> ... -> N-1 -> 0).
    #[default]
    Forward,
    /// Reverse looping cycle (N-1 -> N-2 -> ... -> 0 -> N-1).
    Reverse,
    /// Ping-pong alternating cycle (0 -> 1 -> 2 -> 1 -> 0).
    PingPong,
}

/// A cycling slice configuration within an indexed color palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteSlice {
    /// Start index in palette (inclusive).
    pub start: usize,
    /// End index in palette (exclusive).
    pub end: usize,
    /// Cycling rate in cycles/steps per second (Hz).
    pub rate_hz: f32,
    /// Cycling direction.
    pub direction: CycleDirection,
    /// Accumulated time in seconds.
    pub timer: f32,
    /// Current step offset within the slice.
    pub current_step: usize,
    /// Ping-pong traversal state (true = forward, false = backward).
    pub ping_pong_forward: bool,
}

impl PaletteSlice {
    /// Create a new cycling palette slice.
    pub const fn new(start: usize, end: usize, rate_hz: f32, direction: CycleDirection) -> Self {
        Self {
            start,
            end,
            rate_hz,
            direction,
            timer: 0.0,
            current_step: 0,
            ping_pong_forward: true,
        }
    }

    /// Advance time by `dt` seconds. Returns `true` if the cycle stepped.
    pub fn advance(&mut self, dt: f32) -> bool {
        let span = self.end.saturating_sub(self.start);
        if span <= 1 || self.rate_hz <= 0.0 {
            return false;
        }

        self.timer += dt;
        let period = 1.0 / self.rate_hz;
        let mut changed = false;

        while self.timer >= period {
            self.timer -= period;
            changed = true;

            match self.direction {
                CycleDirection::Forward => {
                    self.current_step = (self.current_step + 1) % span;
                }
                CycleDirection::Reverse => {
                    self.current_step = if self.current_step == 0 {
                        span - 1
                    } else {
                        self.current_step - 1
                    };
                }
                CycleDirection::PingPong => {
                    if self.ping_pong_forward {
                        if self.current_step + 1 >= span {
                            self.ping_pong_forward = false;
                            self.current_step = self.current_step.saturating_sub(1);
                        } else {
                            self.current_step += 1;
                        }
                    } else if self.current_step == 0 {
                        self.ping_pong_forward = true;
                        self.current_step = 1.min(span - 1);
                    } else {
                        self.current_step -= 1;
                    }
                }
            }
        }

        changed
    }

    /// Maps an indexed palette color index through this slice's current animation step.
    #[inline]
    pub fn map_index(&self, index: usize) -> usize {
        let span = self.end.saturating_sub(self.start);
        if span <= 1 || index < self.start || index >= self.end {
            return index;
        }
        self.start + (index - self.start + self.current_step) % span
    }
}

/// Multi-slice palette cycler for zero-allocation, zero-geometry retro animations.
///
/// Cycles indexed color ranges in real time (e.g. water cascades, fire glows, force field pulses,
/// and neon flashing) by shifting indices or updating palette tables without modifying mesh buffers.
#[derive(Debug, Clone, Copy)]
pub struct PaletteCycler<const MAX_SLICES: usize = 4> {
    /// Active animation slices.
    pub slices: [Option<PaletteSlice>; MAX_SLICES],
    /// Number of configured slices.
    pub slice_count: usize,
}

impl<const MAX_SLICES: usize> Default for PaletteCycler<MAX_SLICES> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const MAX_SLICES: usize> PaletteCycler<MAX_SLICES> {
    /// Create a new empty palette cycler.
    pub const fn new() -> Self {
        Self {
            slices: [None; MAX_SLICES],
            slice_count: 0,
        }
    }

    /// Add an animated slice. Returns `true` if added successfully, `false` if full.
    pub fn add_slice(&mut self, slice: PaletteSlice) -> bool {
        if self.slice_count < MAX_SLICES {
            self.slices[self.slice_count] = Some(slice);
            self.slice_count += 1;
            true
        } else {
            false
        }
    }

    /// Add a slice by specifying parameters directly.
    pub fn add_range(
        &mut self,
        start: usize,
        end: usize,
        rate_hz: f32,
        direction: CycleDirection,
    ) -> bool {
        self.add_slice(PaletteSlice::new(start, end, rate_hz, direction))
    }

    /// Advance all slices by delta time `dt` in seconds.
    ///
    /// Returns `true` if any slice advanced a step (indicating palette redraw/sync is needed).
    pub fn advance(&mut self, dt: f32) -> bool {
        let mut any_changed = false;
        for slice in self.slices.iter_mut().take(self.slice_count).flatten() {
            if slice.advance(dt) {
                any_changed = true;
            }
        }
        any_changed
    }

    /// Map an index through all active slices.
    #[inline]
    pub fn map_index(&self, index: usize) -> usize {
        let mut mapped = index;
        for slice in self.slices.iter().take(self.slice_count).flatten() {
            if index >= slice.start && index < slice.end {
                mapped = slice.map_index(index);
                break;
            }
        }
        mapped
    }

    /// Read colors from `base` palette and write cycled colors into `dest` palette.
    pub fn cycle_palette<const N: usize>(&self, base: &[Rgb565; N], dest: &mut [Rgb565; N]) {
        for i in 0..N {
            dest[i] = base[self.map_index(i)];
        }
    }

    /// Cycle slice in-place using a small temporary buffer.
    pub fn cycle_slice(&self, base: &[Rgb565], dest: &mut [Rgb565]) {
        let count = base.len().min(dest.len());
        for i in 0..count {
            dest[i] = base[self.map_index(i)];
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

    #[test]
    fn test_palette_cycler_forward_and_reverse() {
        let mut cycler = PaletteCycler::<2>::new();
        // Forward water cycle: indices [2..5] at 10 Hz
        cycler.add_range(2, 5, 10.0, CycleDirection::Forward);

        assert_eq!(cycler.map_index(0), 0);
        assert_eq!(cycler.map_index(2), 2);
        assert_eq!(cycler.map_index(3), 3);
        assert_eq!(cycler.map_index(4), 4);
        assert_eq!(cycler.map_index(5), 5);

        // Advance 0.1s -> 1 step
        assert!(cycler.advance(0.1));
        assert_eq!(cycler.map_index(2), 3);
        assert_eq!(cycler.map_index(3), 4);
        assert_eq!(cycler.map_index(4), 2);

        // Reverse fire cycle
        let mut rev_slice = PaletteSlice::new(0, 3, 5.0, CycleDirection::Reverse);
        assert_eq!(rev_slice.current_step, 0);
        rev_slice.advance(0.2); // 1 step backward
        assert_eq!(rev_slice.current_step, 2);
    }

    #[test]
    fn test_palette_cycler_ping_pong() {
        let mut slice = PaletteSlice::new(0, 4, 10.0, CycleDirection::PingPong);
        assert_eq!(slice.current_step, 0);
        slice.advance(0.1);
        assert_eq!(slice.current_step, 1);
        slice.advance(0.1);
        assert_eq!(slice.current_step, 2);
        slice.advance(0.1);
        assert_eq!(slice.current_step, 3);
        slice.advance(0.1);
        assert_eq!(slice.current_step, 2); // Reversing
        slice.advance(0.1);
        assert_eq!(slice.current_step, 1);
        slice.advance(0.1);
        assert_eq!(slice.current_step, 0); // At start, switches to forward
        slice.advance(0.1);
        assert_eq!(slice.current_step, 1);
    }

    #[test]
    fn test_palette_cycler_cycle_palette() {
        let mut cycler = PaletteCycler::<1>::new();
        cycler.add_range(1, 3, 20.0, CycleDirection::Forward);

        let base = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE];
        let mut dest = [Rgb565::BLACK; 4];

        cycler.cycle_palette(&base, &mut dest);
        assert_eq!(dest[0], Rgb565::RED);
        assert_eq!(dest[1], Rgb565::GREEN);
        assert_eq!(dest[2], Rgb565::BLUE);
        assert_eq!(dest[3], Rgb565::WHITE);

        cycler.advance(0.05); // 1 step
        cycler.cycle_palette(&base, &mut dest);
        assert_eq!(dest[1], Rgb565::BLUE);
        assert_eq!(dest[2], Rgb565::GREEN);
    }
}
