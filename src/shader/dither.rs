use core::fmt::Debug;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Configuration for ordered dithering effect
#[derive(Debug, Clone, Copy)]
pub struct DitherConfig {
    /// Dithering intensity (0-255, where 0 is no dithering)
    pub intensity: u8,
}

impl DitherConfig {
    /// 4x4 Bayer matrix for ordered dithering
    /// Values are in range [0, 15] and will be scaled by intensity
    const BAYER_MATRIX: [[u8; 4]; 4] =
        [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

    /// Create a new dither configuration
    pub fn new(intensity: u8) -> Self {
        Self { intensity }
    }

    /// Apply dithering effect to a color based on screen position
    #[inline]
    pub fn apply(&self, color: Rgb565, x: i32, y: i32) -> Rgb565 {
        if self.intensity == 0 {
            return color;
        }

        // Get threshold from Bayer matrix (tiles every 4x4 pixels)
        let matrix_x = (x & 3) as usize;
        let matrix_y = (y & 3) as usize;
        let threshold = Self::BAYER_MATRIX[matrix_y][matrix_x];

        // Scale threshold by intensity
        // threshold is 0-15, intensity is 0-255
        // Combined threshold is in range 0-255
        let scaled_threshold = ((threshold as u16 * self.intensity as u16) / 15) as u8;

        // Apply threshold to each color channel
        let r = color.r();
        let g = color.g();
        let b = color.b();

        // Add dithering noise (can increase or decrease based on threshold)
        let r = if r > scaled_threshold {
            r.saturating_sub(scaled_threshold / 2)
        } else {
            r.saturating_add(scaled_threshold / 2)
        };

        let g = if g > scaled_threshold {
            g.saturating_sub(scaled_threshold / 2)
        } else {
            g.saturating_add(scaled_threshold / 2)
        };

        let b = if b > scaled_threshold {
            b.saturating_sub(scaled_threshold / 2)
        } else {
            b.saturating_add(scaled_threshold / 2)
        };

        Rgb565::new(r, g, b)
    }
}

/// Fragment shader decorator that applies Bayer 4x4 dithering.
#[derive(Debug, Clone, Copy)]
pub struct DitherShader<'a, S> {
    pub inner: S,
    pub dither: &'a DitherConfig,
}

impl<'a, S: super::FragmentShader> super::FragmentShader for DitherShader<'a, S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: crate::ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base = self.inner.shade(x, y, z, interps);
        self.dither.apply(base, x, y)
    }
}
