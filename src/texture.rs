//! Texture mapping support for embedded 3D graphics
//!
//! This module provides texture storage, sampling, and management for UV-mapped
//! 3D rendering. It uses static texture data and power-of-2 dimensions for
//! efficient wrapping without divisions.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use heapless::Vec as HeaplessVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgb565,
    Palettized8,
    Palettized4,
}

/// A 2D texture with RGB565 pixel data
///
/// Textures must have power-of-2 dimensions (8, 16, 32, 64, 128, 256, etc.)
/// for efficient wrapping using bit masks instead of modulo operations.
#[derive(Debug, Clone, Copy)]
pub struct Texture {
    /// Texture pixel data in RGB565 format
    pub data: &'static [Rgb565],
    /// Width of the texture (must be power of 2)
    pub width: u32,
    /// Height of the texture (must be power of 2)
    pub height: u32,
    /// Bit mask for wrapping width (width - 1)
    width_mask: u32,
    /// Bit mask for wrapping height (height - 1)
    height_mask: u32,
    /// Palette colors (for palettized modes)
    pub palette: &'static [Rgb565],
    /// Pixel index data (for palettized modes)
    pub indices: &'static [u8],
    /// Texture format (RGB565, 8-bit palettized, or 4-bit palettized)
    pub format: TextureFormat,
}

impl Texture {
    /// Create a new texture
    ///
    /// # Arguments
    /// * `data` - Static RGB565 pixel array (must be width × height elements)
    /// * `width` - Texture width (must be power of 2)
    /// * `height` - Texture height (must be power of 2)
    ///
    /// # Panics
    /// Panics if width or height is not a power of 2, or if data length doesn't match dimensions
    pub fn new(data: &'static [Rgb565], width: u32, height: u32) -> Self {
        assert!(width.is_power_of_two(), "Texture width must be power of 2");
        assert!(
            height.is_power_of_two(),
            "Texture height must be power of 2"
        );
        assert_eq!(
            data.len(),
            (width * height) as usize,
            "Texture data length must match width × height"
        );

        Self {
            data,
            width,
            height,
            width_mask: width - 1,
            height_mask: height - 1,
            palette: &[],
            indices: &[],
            format: TextureFormat::Rgb565,
        }
    }

    /// Create a new 8-bit palettized texture
    pub fn new_palettized8(
        indices: &'static [u8],
        palette: &'static [Rgb565],
        width: u32,
        height: u32,
    ) -> Self {
        assert!(width.is_power_of_two(), "Texture width must be power of 2");
        assert!(
            height.is_power_of_two(),
            "Texture height must be power of 2"
        );
        assert_eq!(
            indices.len(),
            (width * height) as usize,
            "Indices length must match width × height"
        );
        assert!(palette.len() <= 256, "Palette cannot exceed 256 colors");

        Self {
            data: &[],
            width,
            height,
            width_mask: width - 1,
            height_mask: height - 1,
            palette,
            indices,
            format: TextureFormat::Palettized8,
        }
    }

    /// Create a new 4-bit palettized texture
    pub fn new_palettized4(
        indices: &'static [u8],
        palette: &'static [Rgb565],
        width: u32,
        height: u32,
    ) -> Self {
        assert!(width.is_power_of_two(), "Texture width must be power of 2");
        assert!(
            height.is_power_of_two(),
            "Texture height must be power of 2"
        );
        assert_eq!(
            indices.len(),
            ((width * height + 1) / 2) as usize,
            "Indices length must match packed width × height / 2"
        );
        assert!(palette.len() <= 16, "Palette cannot exceed 16 colors");

        Self {
            data: &[],
            width,
            height,
            width_mask: width - 1,
            height_mask: height - 1,
            palette,
            indices,
            format: TextureFormat::Palettized4,
        }
    }

    /// Lookup pixel directly based on current texture format
    #[inline(always)]
    pub fn lookup_pixel(&self, tex_x: u32, tex_y: u32) -> Rgb565 {
        match self.format {
            TextureFormat::Rgb565 => self.data[(tex_y * self.width + tex_x) as usize],
            TextureFormat::Palettized8 => {
                let idx = self.indices[(tex_y * self.width + tex_x) as usize] as usize;
                if idx < self.palette.len() {
                    self.palette[idx]
                } else {
                    Rgb565::new(0, 0, 0)
                }
            }
            TextureFormat::Palettized4 => {
                let pixel_idx = (tex_y * self.width + tex_x) as usize;
                let byte_idx = pixel_idx / 2;
                let shift = if pixel_idx % 2 == 0 { 4 } else { 0 };
                let val = self.indices[byte_idx];
                let palette_idx = ((val >> shift) & 0x0F) as usize;
                if palette_idx < self.palette.len() {
                    self.palette[palette_idx]
                } else {
                    Rgb565::new(0, 0, 0)
                }
            }
        }
    }

    /// Sample the texture at UV coordinates
    ///
    /// Uses nearest-neighbor sampling with wrapping (repeat mode).
    /// UV coordinates are in the range [0.0, 1.0] where:
    /// - (0, 0) is the top-left corner
    /// - (1, 1) is the bottom-right corner
    ///
    /// # Arguments
    /// * `u` - Horizontal texture coordinate (0.0-1.0+, wraps)
    /// * `v` - Vertical texture coordinate (0.0-1.0+, wraps)
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> Rgb565 {
        // Convert UV [0.0, 1.0] to texture coordinates [0, width/height)
        let tex_x = (u * self.width as f32) as u32;
        let tex_y = (v * self.height as f32) as u32;

        // Wrap coordinates using bit masks (fast for power-of-2 dimensions)
        let tex_x = tex_x & self.width_mask;
        let tex_y = tex_y & self.height_mask;

        // Lookup pixel
        self.lookup_pixel(tex_x, tex_y)
    }

    /// Sample the texture at UV coordinates (integer version for performance)
    ///
    /// Uses fixed-point UV coordinates (16.16 format) for faster inner loops.
    ///
    /// # Arguments
    /// * `u_fixed` - Horizontal texture coordinate in 16.16 fixed-point
    /// * `v_fixed` - Vertical texture coordinate in 16.16 fixed-point
    #[inline]
    pub fn sample_fixed(&self, u_fixed: u32, v_fixed: u32) -> Rgb565 {
        // Convert from 16.16 fixed-point to texture coordinates
        // Shift right by 16 to get integer part, then multiply by width/height
        let tex_x = ((u_fixed >> 16) * self.width) >> 16;
        let tex_y = ((v_fixed >> 16) * self.height) >> 16;

        // Wrap coordinates
        let tex_x = tex_x & self.width_mask;
        let tex_y = tex_y & self.height_mask;

        self.lookup_pixel(tex_x, tex_y)
    }

    /// Sample the texture at Q16.16 fixed-point UV coordinates using fast affine indexing.
    ///
    /// `u_q16` and `v_q16` are 16.16 fixed-point numbers where 65536 represents 1.0.
    #[inline(always)]
    pub fn sample_affine_q16(&self, u_q16: u32, v_q16: u32) -> Rgb565 {
        let tex_x = (((u_q16 as u64 * self.width as u64) >> 16) as u32) & self.width_mask;
        let tex_y = (((v_q16 as u64 * self.height as u64) >> 16) as u32) & self.height_mask;
        self.lookup_pixel(tex_x, tex_y)
    }

    /// Sample the texture at Q16.16 fixed-point UV coordinates using 2xSSAA (4-sample sub-pixel anti-aliasing).
    ///
    /// Offsets 4 sub-pixel samples by ±0.25 in Q16 (±0x4000)
    /// and averages their colors to reduce texture aliasing at non-rectilinear angles.
    #[inline]
    pub fn sample_affine_2xssaa_q16(&self, u_q16: u32, v_q16: u32) -> Rgb565 {
        const OFF: u32 = 0x4000; // 0.25 in Q16.16
        let p0 = self.sample_affine_q16(u_q16.wrapping_sub(OFF), v_q16.wrapping_sub(OFF));
        let p1 = self.sample_affine_q16(u_q16.wrapping_add(OFF), v_q16.wrapping_sub(OFF));
        let p2 = self.sample_affine_q16(u_q16.wrapping_sub(OFF), v_q16.wrapping_add(OFF));
        let p3 = self.sample_affine_q16(u_q16.wrapping_add(OFF), v_q16.wrapping_add(OFF));

        let r = (p0.r() as u32 + p1.r() as u32 + p2.r() as u32 + p3.r() as u32 + 2) >> 2;
        let g = (p0.g() as u32 + p1.g() as u32 + p2.g() as u32 + p3.g() as u32 + 2) >> 2;
        let b = (p0.b() as u32 + p1.b() as u32 + p2.b() as u32 + p3.b() as u32 + 2) >> 2;

        Rgb565::new(r as u8, g as u8, b as u8)
    }

    /// Sample a horizontal scanline using Q16.16 fixed-point affine UV stepping.
    pub fn sample_affine_scanline_q16(
        &self,
        mut u_q16: u32,
        mut v_q16: u32,
        du_q16: u32,
        dv_q16: u32,
        out_buffer: &mut [Rgb565],
    ) {
        for pixel in out_buffer.iter_mut() {
            *pixel = self.sample_affine_q16(u_q16, v_q16);
            u_q16 = u_q16.wrapping_add(du_q16);
            v_q16 = v_q16.wrapping_add(dv_q16);
        }
    }

    /// Get texture dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

const ANIMATION_ID_FLAG: u32 = 0x8000_0000;
const ANIMATION_ID_MASK: u32 = !ANIMATION_ID_FLAG;
const MAX_ANIMATIONS: usize = 16;
const MAX_ANIMATION_FRAMES: usize = 8;

#[derive(Debug, Clone, Copy)]
struct TextureAnimation {
    frames: [u32; MAX_ANIMATION_FRAMES],
    frame_count: u8,
    ticks_per_frame: u16,
    tick_accum: u16,
    current_frame: u8,
    looping: bool,
}

impl TextureAnimation {
    fn new(frame_ids: &[u32], ticks_per_frame: u16, looping: bool) -> Option<Self> {
        if frame_ids.is_empty() || frame_ids.len() > MAX_ANIMATION_FRAMES {
            return None;
        }
        let mut frames = [0u32; MAX_ANIMATION_FRAMES];
        for (i, frame_id) in frame_ids.iter().copied().enumerate() {
            frames[i] = frame_id;
        }
        Some(Self {
            frames,
            frame_count: frame_ids.len() as u8,
            ticks_per_frame: ticks_per_frame.max(1),
            tick_accum: 0,
            current_frame: 0,
            looping,
        })
    }

    #[inline]
    fn current_texture_id(&self) -> u32 {
        self.frames[self.current_frame as usize]
    }

    fn tick(&mut self, ticks: u16) {
        if self.frame_count <= 1 {
            return;
        }
        let mut accum = self.tick_accum.saturating_add(ticks);
        while accum >= self.ticks_per_frame {
            accum -= self.ticks_per_frame;
            if self.current_frame + 1 < self.frame_count {
                self.current_frame += 1;
            } else if self.looping {
                self.current_frame = 0;
            } else {
                // Clamp to final frame for non-looping animations.
                accum = 0;
                break;
            }
        }
        self.tick_accum = accum;
    }
}

/// Texture manager for storing multiple textures
///
/// Uses a fixed-size heapless vector for no_std compatibility.
/// The capacity N determines how many textures can be stored.
pub struct TextureManager<const N: usize> {
    textures: HeaplessVec<Texture, N>,
    animations: HeaplessVec<TextureAnimation, MAX_ANIMATIONS>,
}

impl<const N: usize> TextureManager<N> {
    /// Create a new empty texture manager
    pub fn new() -> Self {
        Self {
            textures: HeaplessVec::new(),
            animations: HeaplessVec::new(),
        }
    }

    /// Add a texture to the manager
    ///
    /// Returns the texture ID (index) that can be used to reference this texture.
    ///
    /// # Returns
    /// `Some(texture_id)` if successful, `None` if the manager is full
    pub fn add_texture(&mut self, texture: Texture) -> Option<u32> {
        self.textures.push(texture).ok()?;
        Some((self.textures.len() - 1) as u32)
    }

    /// Get a texture by ID
    ///
    /// # Arguments
    /// * `id` - Texture ID returned by `add_texture()`
    ///
    /// # Returns
    /// `Some(&Texture)` if the ID is valid, `None` otherwise
    pub fn get(&self, id: u32) -> Option<&Texture> {
        let resolved = self.resolve_texture_id(id)?;
        self.textures.get(resolved as usize)
    }

    /// Add an animated texture sequence.
    ///
    /// Returns an animation ID that can be used anywhere a texture ID is accepted.
    pub fn add_animation(
        &mut self,
        frame_ids: &[u32],
        ticks_per_frame: u16,
        looping: bool,
    ) -> Option<u32> {
        // Ensure all frame IDs resolve to concrete texture slots.
        for frame_id in frame_ids {
            let resolved = self.resolve_texture_id(*frame_id)?;
            if resolved as usize >= self.textures.len() {
                return None;
            }
        }

        let animation = TextureAnimation::new(frame_ids, ticks_per_frame, looping)?;
        self.animations.push(animation).ok()?;
        let index = (self.animations.len() - 1) as u32;
        Some(ANIMATION_ID_FLAG | (index & ANIMATION_ID_MASK))
    }

    /// Advance all registered texture animations by `ticks`.
    pub fn tick(&mut self, ticks: u16) {
        for animation in &mut self.animations {
            animation.tick(ticks);
        }
    }

    /// Check whether an ID represents an animation handle.
    #[inline]
    pub fn is_animation_id(id: u32) -> bool {
        (id & ANIMATION_ID_FLAG) != 0
    }

    /// Resolve an ID to a concrete texture slot.
    ///
    /// For static textures this returns the same ID. For animation IDs it
    /// returns the current frame's texture ID.
    pub fn resolve_texture_id(&self, id: u32) -> Option<u32> {
        if !Self::is_animation_id(id) {
            return Some(id);
        }
        let anim_idx = (id & ANIMATION_ID_MASK) as usize;
        let animation = self.animations.get(anim_idx)?;
        Some(animation.current_texture_id())
    }

    /// Get the number of stored textures
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Check if the manager is empty
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Check if the manager is full
    pub fn is_full(&self) -> bool {
        self.textures.len() >= N
    }
}

impl<const N: usize> Default for TextureManager<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::{Rgb565, WebColors};

    #[test]
    fn test_texture_creation() {
        static DATA: [Rgb565; 64] = [Rgb565::CSS_RED; 64];
        let texture = Texture::new(&DATA, 8, 8);

        assert_eq!(texture.width, 8);
        assert_eq!(texture.height, 8);
        assert_eq!(texture.dimensions(), (8, 8));
    }

    #[test]
    #[should_panic(expected = "width must be power of 2")]
    fn test_texture_non_power_of_2_width() {
        static DATA: [Rgb565; 60] = [Rgb565::CSS_RED; 60];
        let _texture = Texture::new(&DATA, 10, 6); // 10 is not power of 2
    }

    #[test]
    #[should_panic(expected = "height must be power of 2")]
    fn test_texture_non_power_of_2_height() {
        static DATA: [Rgb565; 48] = [Rgb565::CSS_RED; 48];
        let _texture = Texture::new(&DATA, 8, 6); // 6 is not power of 2
    }

    #[test]
    #[should_panic(expected = "length must match")]
    fn test_texture_wrong_data_length() {
        static DATA: [Rgb565; 60] = [Rgb565::CSS_RED; 60];
        let _texture = Texture::new(&DATA, 8, 8); // Should be 64 elements
    }

    #[test]
    fn test_texture_sampling() {
        static DATA: [Rgb565; 16] = [
            Rgb565::CSS_RED,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
            Rgb565::CSS_YELLOW,
            Rgb565::CSS_CYAN,
            Rgb565::CSS_MAGENTA,
            Rgb565::CSS_WHITE,
            Rgb565::CSS_BLACK,
            Rgb565::CSS_RED,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
            Rgb565::CSS_YELLOW,
            Rgb565::CSS_CYAN,
            Rgb565::CSS_MAGENTA,
            Rgb565::CSS_WHITE,
            Rgb565::CSS_BLACK,
        ];

        let texture = Texture::new(&DATA, 4, 4);

        // Sample at corners
        let tl = texture.sample(0.0, 0.0);
        assert_eq!(tl, Rgb565::CSS_RED);

        // Sample in middle (0.5, 0.5) -> (2, 2) -> index 10
        let mid = texture.sample(0.5, 0.5);
        assert_eq!(mid, Rgb565::CSS_BLUE);
    }

    #[test]
    fn test_texture_wrapping() {
        static DATA: [Rgb565; 16] = [Rgb565::CSS_RED; 16];
        let texture = Texture::new(&DATA, 4, 4);

        // Sample beyond 1.0 should wrap
        let wrapped = texture.sample(1.5, 1.5);
        assert_eq!(wrapped, Rgb565::CSS_RED);
    }

    #[test]
    fn test_texture_manager() {
        static DATA1: [Rgb565; 16] = [Rgb565::CSS_RED; 16];
        static DATA2: [Rgb565; 64] = [Rgb565::CSS_GREEN; 64];

        let mut manager = TextureManager::<4>::new();

        assert!(manager.is_empty());
        assert!(!manager.is_full());

        let id1 = manager.add_texture(Texture::new(&DATA1, 4, 4));
        assert_eq!(id1, Some(0));
        assert_eq!(manager.len(), 1);

        let id2 = manager.add_texture(Texture::new(&DATA2, 8, 8));
        assert_eq!(id2, Some(1));
        assert_eq!(manager.len(), 2);

        // Retrieve textures
        let tex1 = manager.get(0).unwrap();
        assert_eq!(tex1.width, 4);

        let tex2 = manager.get(1).unwrap();
        assert_eq!(tex2.width, 8);
    }

    #[test]
    fn test_texture_manager_full() {
        static DATA: [Rgb565; 16] = [Rgb565::CSS_RED; 16];

        let mut manager = TextureManager::<2>::new();

        // Fill the manager
        assert!(manager.add_texture(Texture::new(&DATA, 4, 4)).is_some());
        assert!(manager.add_texture(Texture::new(&DATA, 4, 4)).is_some());
        assert!(manager.is_full());

        // Try to add one more (should fail)
        assert!(manager.add_texture(Texture::new(&DATA, 4, 4)).is_none());
    }

    #[test]
    fn test_texture_animation_looping_sequence() {
        static RED: [Rgb565; 16] = [Rgb565::CSS_RED; 16];
        static GREEN: [Rgb565; 16] = [Rgb565::CSS_GREEN; 16];
        static BLUE: [Rgb565; 16] = [Rgb565::CSS_BLUE; 16];

        let mut manager = TextureManager::<8>::new();
        let red = manager.add_texture(Texture::new(&RED, 4, 4)).unwrap();
        let green = manager.add_texture(Texture::new(&GREEN, 4, 4)).unwrap();
        let blue = manager.add_texture(Texture::new(&BLUE, 4, 4)).unwrap();

        let anim_id = manager.add_animation(&[red, green, blue], 2, true).unwrap();
        assert!(TextureManager::<8>::is_animation_id(anim_id));

        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_RED
        );
        manager.tick(2);
        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_GREEN
        );
        manager.tick(2);
        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_BLUE
        );
        manager.tick(2);
        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_RED
        );
    }

    #[test]
    fn test_texture_animation_non_looping_clamps_final_frame() {
        static RED: [Rgb565; 16] = [Rgb565::CSS_RED; 16];
        static GREEN: [Rgb565; 16] = [Rgb565::CSS_GREEN; 16];

        let mut manager = TextureManager::<4>::new();
        let red = manager.add_texture(Texture::new(&RED, 4, 4)).unwrap();
        let green = manager.add_texture(Texture::new(&GREEN, 4, 4)).unwrap();

        let anim_id = manager.add_animation(&[red, green], 1, false).unwrap();
        manager.tick(1);
        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_GREEN
        );
        manager.tick(10);
        assert_eq!(
            manager.get(anim_id).unwrap().sample(0.0, 0.0),
            Rgb565::CSS_GREEN
        );
    }

    #[test]
    fn test_texture_sample_affine_q16() {
        static DATA: [Rgb565; 16] = [
            Rgb565::CSS_RED,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
            Rgb565::CSS_YELLOW,
            Rgb565::CSS_CYAN,
            Rgb565::CSS_MAGENTA,
            Rgb565::CSS_WHITE,
            Rgb565::CSS_BLACK,
            Rgb565::CSS_RED,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
            Rgb565::CSS_YELLOW,
            Rgb565::CSS_CYAN,
            Rgb565::CSS_MAGENTA,
            Rgb565::CSS_WHITE,
            Rgb565::CSS_BLACK,
        ];
        let texture = Texture::new(&DATA, 4, 4);

        // Q16.16 for (0.5, 0.5) => (32768, 32768)
        let sample = texture.sample_affine_q16(32768, 32768);
        assert_eq!(sample, Rgb565::CSS_BLUE);
    }

    #[test]
    fn test_texture_sample_affine_scanline_q16() {
        static DATA: [Rgb565; 16] = [Rgb565::CSS_RED; 16];
        let texture = Texture::new(&DATA, 4, 4);

        let mut scanline = [Rgb565::CSS_BLACK; 4];
        texture.sample_affine_scanline_q16(0, 0, 16384, 0, &mut scanline);
        assert_eq!(scanline[0], Rgb565::CSS_RED);
    }
}
