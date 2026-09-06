//! Perceptual color models and standard palettes for embedded systems.
//!
//! Inspired by Bevy's `bevy_color`, adapted for `no_std` zero-allocation execution
//! with `embedded-graphics` [`Rgb565`]. Provides Hue-Saturation-Value (HSV)
//! conversions with shortest-arc hue interpolation for vibrant color gradients.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// Hue-Saturation-Value (HSV) color representation.
///
/// * `h`: Hue in degrees `[0.0, 360.0)`.
/// * `s`: Saturation in `[0.0, 1.0]`.
/// * `v`: Value / Brightness in `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    /// Create a new HSV color.
    #[inline]
    pub const fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }

    /// Convert [`Rgb565`] to [`Hsv`].
    pub fn from_rgb565(rgb: Rgb565) -> Self {
        let r = rgb.r() as f32 / 31.0;
        let g = rgb.g() as f32 / 63.0;
        let b = rgb.b() as f32 / 31.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let v = max;
        let s = if max > 1e-5 { delta / max } else { 0.0 };

        let h = if delta < 1e-5 {
            0.0
        } else if (max - r).abs() < 1e-5 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() < 1e-5 {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let h = if h < 0.0 { h + 360.0 } else { h };

        Self { h, s, v }
    }

    /// Convert [`Hsv`] to [`Rgb565`].
    pub fn to_rgb565(&self) -> Rgb565 {
        let c = self.v * self.s;
        let h_prime = (self.h.rem_euclid(360.0)) / 60.0;
        let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
        let m = self.v - c;

        let (r1, g1, b1) = if h_prime < 1.0 {
            (c, x, 0.0)
        } else if h_prime < 2.0 {
            (x, c, 0.0)
        } else if h_prime < 3.0 {
            (0.0, c, x)
        } else if h_prime < 4.0 {
            (0.0, x, c)
        } else if h_prime < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        let r = (((r1 + m) * 31.0).round() as u8).min(31);
        let g = (((g1 + m) * 63.0).round() as u8).min(63);
        let b = (((b1 + m) * 31.0).round() as u8).min(31);

        Rgb565::new(r, g, b)
    }

    /// Interpolate between two HSV colors along the shortest arc of the hue circle.
    ///
    /// Avoids the muddy grayish desaturation that occurs when interpolating directly in RGB.
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);

        // Shortest hue delta on 360-degree circle
        let mut d_h = b.h - a.h;
        if d_h > 180.0 {
            d_h -= 360.0;
        } else if d_h < -180.0 {
            d_h += 360.0;
        }

        let h = (a.h + d_h * t).rem_euclid(360.0);
        let s = a.s + (b.s - a.s) * t;
        let v = a.v + (b.v - a.v) * t;

        Self { h, s, v }
    }
}

/// Standard VGA / CSS Basic color constants for fast prototyping.
pub struct Palette;

impl Palette {
    pub const RED: Rgb565 = Rgb565::new(31, 0, 0);
    pub const GREEN: Rgb565 = Rgb565::new(0, 63, 0);
    pub const BLUE: Rgb565 = Rgb565::new(0, 0, 31);
    pub const YELLOW: Rgb565 = Rgb565::new(31, 63, 0);
    pub const CYAN: Rgb565 = Rgb565::new(0, 63, 31);
    pub const MAGENTA: Rgb565 = Rgb565::new(31, 0, 31);
    pub const WHITE: Rgb565 = Rgb565::new(31, 63, 31);
    pub const BLACK: Rgb565 = Rgb565::new(0, 0, 0);
    pub const GRAY: Rgb565 = Rgb565::new(16, 32, 16);
    pub const ORANGE: Rgb565 = Rgb565::new(31, 40, 0);
    pub const PURPLE: Rgb565 = Rgb565::new(16, 0, 16);
    pub const LIME: Rgb565 = Rgb565::new(10, 63, 10);
    pub const NAVY: Rgb565 = Rgb565::new(0, 0, 16);
    pub const TEAL: Rgb565 = Rgb565::new(0, 32, 16);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsv_roundtrip_primaries() {
        let red = Rgb565::new(31, 0, 0);
        let hsv_red = Hsv::from_rgb565(red);
        assert!((hsv_red.h - 0.0).abs() < 1e-2 || (hsv_red.h - 360.0).abs() < 1e-2);
        assert!((hsv_red.s - 1.0).abs() < 1e-2);
        assert!((hsv_red.v - 1.0).abs() < 1e-2);
        assert_eq!(hsv_red.to_rgb565(), red);

        let green = Rgb565::new(0, 63, 0);
        let hsv_green = Hsv::from_rgb565(green);
        assert!((hsv_green.h - 120.0).abs() < 1e-2);
        assert_eq!(hsv_green.to_rgb565(), green);

        let blue = Rgb565::new(0, 0, 31);
        let hsv_blue = Hsv::from_rgb565(blue);
        assert!((hsv_blue.h - 240.0).abs() < 1e-2);
        assert_eq!(hsv_blue.to_rgb565(), blue);
    }

    #[test]
    fn test_hsv_lerp_shortest_arc() {
        // 350 deg (near red) to 10 deg (near red) should go through 0 deg, not 180 deg
        let a = Hsv::new(350.0, 1.0, 1.0);
        let b = Hsv::new(10.0, 1.0, 1.0);
        let mid = Hsv::lerp(a, b, 0.5);
        assert!((mid.h - 0.0).abs() < 1e-2 || (mid.h - 360.0).abs() < 1e-2);
    }
}
