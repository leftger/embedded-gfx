//! Fast RGB565 / RGBA8888 color blending and readback traits.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
#[cfg(feature = "aa")]
use embedded_graphics_core::prelude::Point;

// Row width configuration - features are prioritized if multiple are enabled
#[cfg(feature = "row_width_320")]
pub(crate) const MAX_ROW_WIDTH: usize = 320;
#[cfg(all(feature = "row_width_240", not(feature = "row_width_320")))]
pub(crate) const MAX_ROW_WIDTH: usize = 240;
#[cfg(all(
    feature = "row_width_160",
    not(feature = "row_width_240"),
    not(feature = "row_width_320"),
    not(feature = "row_width_96")
))]
pub(crate) const MAX_ROW_WIDTH: usize = 160;
#[cfg(all(
    feature = "row_width_96",
    not(feature = "row_width_160"),
    not(feature = "row_width_240"),
    not(feature = "row_width_320")
))]
pub(crate) const MAX_ROW_WIDTH: usize = 96;
#[cfg(not(any(
    feature = "row_width_320",
    feature = "row_width_240",
    feature = "row_width_160",
    feature = "row_width_96"
)))]
pub(crate) const MAX_ROW_WIDTH: usize = 100;

/// Framebuffer that supports reading back pixel values.
#[cfg(feature = "aa")]
pub trait ReadPixel {
    /// Returns the color currently stored at `point`.
    fn read_pixel(&self, point: Point) -> Rgb565;
}

/// Readback capability shared with the rest of the ecosystem.
pub use embedded_draw_target::PixelRead;

#[cfg(feature = "aa")]
impl<T: PixelRead<Color = Rgb565>> ReadPixel for T {
    #[inline]
    fn read_pixel(&self, point: Point) -> Rgb565 {
        self.get_pixel(point)
    }
}

use embedded_graphics_core::pixelcolor::IntoStorage;
use embedded_graphics_core::pixelcolor::raw::RawU16;

/// Fast RGB565 alpha blending using 32-bit SWAR (SIMD within a register).
///
/// Blends Red and Blue channels in parallel in one 32-bit register and Green in another,
/// eliminating per-channel bit unpack overhead and integer divisions.
#[inline(always)]
pub fn fast_blend_rgb565(bg: Rgb565, fg: Rgb565, alpha: u8) -> Rgb565 {
    if alpha == 255 {
        return fg;
    }
    if alpha == 0 {
        return bg;
    }

    let bg_raw = bg.into_storage() as u32;
    let fg_raw = fg.into_storage() as u32;
    let a = alpha as u32;

    // Mask R & B: 0b11111_000000_11111 = 0xF81F
    // Mask G:   0b00000_111111_00000 = 0x07E0
    let bg_rb = bg_raw & 0xF81F;
    let fg_rb = fg_raw & 0xF81F;
    let bg_g = bg_raw & 0x07E0;
    let fg_g = fg_raw & 0x07E0;

    // Linear interpolation with 1/256 rounding:
    // val = bg + ((fg - bg) * a + 128) >> 8
    let rb = ((fg_rb.wrapping_sub(bg_rb))
        .wrapping_mul(a)
        .wrapping_add(0x8010)
        >> 8)
        .wrapping_add(bg_rb)
        & 0xF81F;
    let g = ((fg_g.wrapping_sub(bg_g))
        .wrapping_mul(a)
        .wrapping_add(0x0400)
        >> 8)
        .wrapping_add(bg_g)
        & 0x07E0;

    Rgb565::from(RawU16::new((rb | g) as u16))
}

/// Fast RGBA8888 alpha blending with division-free 8-bit scaling.
#[inline(always)]
pub fn fast_blend_rgba8888(bg: [u8; 4], fg: [u8; 4]) -> [u8; 4] {
    let a = fg[3] as u32;
    if a == 255 {
        return fg;
    }
    if a == 0 {
        return bg;
    }
    let inv = 255 - a;
    // Division-free scaling by 255: (x + 1 + (x >> 8)) >> 8
    let blend_ch = |b: u8, f: u8| -> u8 {
        let prod = (b as u32) * inv + (f as u32) * a;
        ((prod + 1 + (prod >> 8)) >> 8) as u8
    };
    let r = blend_ch(bg[0], fg[0]);
    let g = blend_ch(bg[1], fg[1]);
    let b = blend_ch(bg[2], fg[2]);
    let out_a_prod = (bg[3] as u32) * inv;
    let out_a = fg[3] as u32 + ((out_a_prod + 1 + (out_a_prod >> 8)) >> 8);
    [r, g, b, out_a as u8]
}

/// Fast RGBA8888 to RGB565 alpha blending.
#[inline(always)]
pub fn fast_blend_rgba8888_to_rgb565(bg: Rgb565, fg_rgba: [u8; 4]) -> Rgb565 {
    let alpha = fg_rgba[3];
    if alpha == 0 {
        return bg;
    }
    let fg_r = fg_rgba[0] >> 3;
    let fg_g = fg_rgba[1] >> 2;
    let fg_b = fg_rgba[2] >> 3;
    let fg_565 = Rgb565::new(fg_r, fg_g, fg_b);
    fast_blend_rgb565(bg, fg_565, alpha)
}

/// Fast color inversion (reverse color) filter for RGB565.
#[inline(always)]
pub fn reverse_color_rgb565(c: Rgb565) -> Rgb565 {
    Rgb565::new(31 - c.r(), 63 - c.g(), 31 - c.b())
}

/// Fast color inversion (reverse color) filter for RGBA8888.
#[inline(always)]
pub fn reverse_color_rgba8888(rgba: [u8; 4]) -> [u8; 4] {
    [255 - rgba[0], 255 - rgba[1], 255 - rgba[2], rgba[3]]
}

/// Component-wise blend in 8-bit fixed-point coverage.
#[cfg(feature = "aa")]
#[inline(always)]
pub(crate) fn blend_q8(bg: Rgb565, fg: Rgb565, coverage_q8: u32) -> Rgb565 {
    let inv = 256 - coverage_q8;
    let r = (bg.r() as u32 * inv + fg.r() as u32 * coverage_q8) >> 8;
    let g = (bg.g() as u32 * inv + fg.g() as u32 * coverage_q8) >> 8;
    let b = (bg.b() as u32 * inv + fg.b() as u32 * coverage_q8) >> 8;
    Rgb565::new(r as u8, g as u8, b as u8)
}
