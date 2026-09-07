//! Fast ordered Bayer dithering for low-color embedded displays.
//!
//! Microcontroller TFTs and OLEDs predominantly display 16-bit RGB565 or 8-bit
//! RGB332 color. Smooth Gouraud shading, fog, spotlights, and gradients suffer
//! from visible color banding (contouring artifacts).
//!
//! This module provides $O(1)$ spatial Bayer threshold dithering that diffuses
//! quantization error without runtime memory allocation, visibly doubling the
//! perceived color depth on small displays.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// 4×4 Bayer matrix (16 threshold levels: 0..15).
pub struct Bayer4x4;

impl Bayer4x4 {
    pub const MATRIX: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

    /// Spatial threshold lookup in `[0, 15]` using screen coordinate modulo.
    #[inline(always)]
    pub const fn threshold(x: u32, y: u32) -> u8 {
        Self::MATRIX[(y & 3) as usize][(x & 3) as usize]
    }
}

/// 8×8 Bayer matrix (64 threshold levels: 0..63).
pub struct Bayer8x8;

impl Bayer8x8 {
    pub const MATRIX: [[u8; 8]; 8] = [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ];

    /// Spatial threshold lookup in `[0, 63]` using screen coordinate modulo.
    #[inline(always)]
    pub const fn threshold(x: u32, y: u32) -> u8 {
        Self::MATRIX[(y & 7) as usize][(x & 7) as usize]
    }
}

/// Quantize 24-bit RGB888 channels down to 16-bit [`Rgb565`] with 8×8 Bayer dithering.
///
/// Uses sub-step remainder comparison to distribute quantization error across
/// spatial neighbors. Endpoints (pure black and pure white) are strictly preserved.
#[inline]
pub fn dither_rgb888_to_rgb565(x: u32, y: u32, r: u8, g: u8, b: u8) -> Rgb565 {
    let t = Bayer8x8::threshold(x, y);

    // Red: 8-bit -> 5-bit (step size = 8, 32 levels)
    let r5 = r >> 3;
    let r_rem = r & 0x07;
    let r_out = if r_rem > (t >> 3) && r5 < 31 {
        r5 + 1
    } else {
        r5
    };

    // Green: 8-bit -> 6-bit (step size = 4, 64 levels)
    let g6 = g >> 2;
    let g_rem = g & 0x03;
    let g_out = if g_rem > (t >> 4) && g6 < 63 {
        g6 + 1
    } else {
        g6
    };

    // Blue: 8-bit -> 5-bit (step size = 8, 32 levels)
    let b5 = b >> 3;
    let b_rem = b & 0x07;
    let b_out = if b_rem > (t >> 3) && b5 < 31 {
        b5 + 1
    } else {
        b5
    };

    Rgb565::new(r_out, g_out, b_out)
}

/// Quantize 24-bit RGB888 channels down to 8-bit RGB332 byte with 8×8 Bayer dithering.
#[inline]
pub fn dither_rgb888_to_rgb332(x: u32, y: u32, r: u8, g: u8, b: u8) -> u8 {
    let t = Bayer8x8::threshold(x, y);

    // Red: 8-bit -> 3-bit (step size = 32, 8 levels)
    let r3 = r >> 5;
    let r_rem = r & 0x1F;
    let r_out = if r_rem > (t >> 1) && r3 < 7 {
        r3 + 1
    } else {
        r3
    };

    // Green: 8-bit -> 3-bit (step size = 32, 8 levels)
    let g3 = g >> 5;
    let g_rem = g & 0x1F;
    let g_out = if g_rem > (t >> 1) && g3 < 7 {
        g3 + 1
    } else {
        g3
    };

    // Blue: 8-bit -> 2-bit (step size = 64, 4 levels)
    let b2 = b >> 6;
    let b_rem = b & 0x3F;
    let b_out = if b_rem > t && b2 < 3 { b2 + 1 } else { b2 };

    (r_out << 5) | (g_out << 2) | b_out
}

/// Apply subtle 4×4 spatial dithering directly to an existing [`Rgb565`] pixel.
///
/// Useful as a post-process effect on rendered scanlines to break up flat shading bands.
#[inline]
pub fn dither_rgb565(x: u32, y: u32, color: Rgb565) -> Rgb565 {
    let t = Bayer4x4::threshold(x, y);
    // Threshold in [0, 15] centered around 8: [-1, 0, +1]
    let offset = if t > 10 {
        1i8
    } else if t < 5 {
        -1i8
    } else {
        0i8
    };

    let r = ((color.r() as i8) + offset).clamp(0, 31) as u8;
    let g = ((color.g() as i8) + offset).clamp(0, 63) as u8;
    let b = ((color.b() as i8) + offset).clamp(0, 31) as u8;

    Rgb565::new(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bayer_matrices_bounds() {
        for y in 0..4 {
            for x in 0..4 {
                assert!(Bayer4x4::threshold(x, y) <= 15);
            }
        }
        for y in 0..8 {
            for x in 0..8 {
                assert!(Bayer8x8::threshold(x, y) <= 63);
            }
        }
    }

    #[test]
    fn test_rgb888_to_rgb565_endpoints() {
        // Pure black stays black across all coordinates
        for y in 0..8 {
            for x in 0..8 {
                let c = dither_rgb888_to_rgb565(x, y, 0, 0, 0);
                assert_eq!(c.r(), 0);
                assert_eq!(c.g(), 0);
                assert_eq!(c.b(), 0);
            }
        }

        // Pure white stays white across all coordinates
        for y in 0..8 {
            for x in 0..8 {
                let c = dither_rgb888_to_rgb565(x, y, 255, 255, 255);
                assert_eq!(c.r(), 31);
                assert_eq!(c.g(), 63);
                assert_eq!(c.b(), 31);
            }
        }
    }

    #[test]
    fn test_midpoint_dither_diffusion() {
        // Midpoint value 4 (halfway between 0 and 8 for 5-bit step)
        let mut count_high = 0;
        let mut count_low = 0;
        for y in 0..8 {
            for x in 0..8 {
                let c = dither_rgb888_to_rgb565(x, y, 4, 0, 0);
                if c.r() == 1 {
                    count_high += 1;
                } else if c.r() == 0 {
                    count_low += 1;
                }
            }
        }
        // Exactly half of the 64 pixels should round up and half stay low
        assert_eq!(count_high, 32);
        assert_eq!(count_low, 32);
    }

    #[test]
    fn test_rgb332_endpoints_and_rgb565_dither() {
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(dither_rgb888_to_rgb332(x, y, 0, 0, 0), 0);
                assert_eq!(dither_rgb888_to_rgb332(x, y, 255, 255, 255), 0xFF);
                let _ = dither_rgb565(x, y, Rgb565::new(20, 40, 20));
            }
        }

        let changes = (0..4)
            .flat_map(|y| (0..4).map(move |x| dither_rgb565(x, y, Rgb565::new(31, 63, 31))))
            .filter(|c| *c != Rgb565::new(31, 63, 31))
            .count();
        assert!(changes > 0);
    }
}
