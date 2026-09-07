//! Post-processing and environment effect configurations (fog and dithering).

use core::fmt::Debug;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Configuration for depth-based fog effect.
#[derive(Debug, Clone, Copy)]
pub struct FogConfig {
    /// Fog color to blend towards.
    pub color: Rgb565,
    /// Near plane distance (fixed-point 16.16 format).
    pub near: u32,
    /// Far plane distance (fixed-point 16.16 format).
    pub far: u32,
}

impl FogConfig {
    /// Create a new fog configuration.
    pub fn new(color: Rgb565, near: f32, far: f32) -> Self {
        Self {
            color,
            near: (near * 65536.0) as u32,
            far: (far * 65536.0) as u32,
        }
    }

    /// Apply fog effect to a color based on depth.
    #[inline]
    pub fn apply(&self, base_color: Rgb565, depth: u32) -> Rgb565 {
        let fog_factor = if depth <= self.near {
            0u32
        } else if depth >= self.far {
            65536u32
        } else {
            let numerator = (depth - self.near) as u64;
            let denominator = (self.far - self.near) as u64;
            ((numerator * 65536) / denominator) as u32
        };

        let base_r = base_color.r() as u32;
        let base_g = base_color.g() as u32;
        let base_b = base_color.b() as u32;

        let fog_r = self.color.r() as u32;
        let fog_g = self.color.g() as u32;
        let fog_b = self.color.b() as u32;

        let r = ((base_r * (65536 - fog_factor) + fog_r * fog_factor) / 65536) as u8;
        let g = ((base_g * (65536 - fog_factor) + fog_g * fog_factor) / 65536) as u8;
        let b = ((base_b * (65536 - fog_factor) + fog_b * fog_factor) / 65536) as u8;

        Rgb565::new(r, g, b)
    }
}

/// Configuration for ordered dithering effect.
#[derive(Debug, Clone, Copy)]
pub struct DitherConfig {
    /// Dithering intensity (0-255, where 0 is no dithering).
    pub intensity: u8,
}

impl DitherConfig {
    /// 4x4 Bayer matrix for ordered dithering.
    const BAYER_MATRIX: [[u8; 4]; 4] =
        [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

    /// Create a new dither configuration.
    pub fn new(intensity: u8) -> Self {
        Self { intensity }
    }

    /// Apply dithering effect to a color based on screen position.
    #[inline]
    pub fn apply(&self, color: Rgb565, x: i32, y: i32) -> Rgb565 {
        if self.intensity == 0 {
            return color;
        }

        let matrix_x = (x & 3) as usize;
        let matrix_y = (y & 3) as usize;
        let threshold = Self::BAYER_MATRIX[matrix_y][matrix_x];

        let scaled_threshold = ((threshold as u16 * self.intensity as u16) / 15) as u8;

        let r = color.r();
        let g = color.g();
        let b = color.b();

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

/// Strategy for depth interpolation during triangle rasterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthInterpolationMode {
    /// Full per-pixel linear depth interpolation along scanlines (default).
    #[default]
    Exact,
    /// Uniform per-triangle depth using the arithmetic average of vertex depths.
    FastAverage,
    /// Uniform per-triangle depth using the maximum (farthest) vertex depth.
    FastMax,
}

impl DepthInterpolationMode {
    /// Pre-process vertex depths according to the interpolation mode.
    #[inline(always)]
    pub fn process_depths(&self, z1: f32, z2: f32, z3: f32) -> (f32, f32, f32) {
        match self {
            Self::Exact => (z1, z2, z3),
            Self::FastAverage => {
                let avg = (z1 + z2 + z3) * (1.0 / 3.0);
                (avg, avg, avg)
            }
            Self::FastMax => {
                let max_z = if z1 >= z2 && z1 >= z3 {
                    z1
                } else if z2 >= z3 {
                    z2
                } else {
                    z3
                };
                (max_z, max_z, max_z)
            }
        }
    }
}

/// Field interlace mode for temporal scanline-interlaced rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterlaceField {
    /// Progressive rendering: all scanlines drawn (default).
    #[default]
    Progressive,
    /// Even field: draws only scanlines with even y (0, 2, 4, ...).
    Even,
    /// Odd field: draws only scanlines with odd y (1, 3, 5, ...).
    Odd,
}

impl InterlaceField {
    /// Returns true if scanline `y` should be rendered in this field.
    #[inline(always)]
    pub fn includes_scanline(&self, y: i32) -> bool {
        match self {
            Self::Progressive => true,
            Self::Even => (y & 1) == 0,
            Self::Odd => (y & 1) != 0,
        }
    }

    /// Toggles between Even and Odd field for successive frames.
    #[inline]
    pub fn toggle(&self) -> Self {
        match self {
            Self::Progressive => Self::Progressive,
            Self::Even => Self::Odd,
            Self::Odd => Self::Even,
        }
    }
}

/// Configuration for screen-door (dithered stipple) transparency.
///
/// Discards fragments deterministically against a 4x4 Bayer threshold matrix,
/// providing order-independent, zero-allocation transparency that writes directly
/// to the depth buffer without requiring sorting or frame readbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenDoorConfig {
    /// Opacity level (0 = fully transparent/discarded, 255 = fully opaque).
    pub alpha: u8,
}

impl ScreenDoorConfig {
    const BAYER_THRESHOLD: [[u8; 4]; 4] = [
        [0, 128, 32, 160],
        [192, 64, 224, 96],
        [48, 176, 16, 144],
        [240, 112, 208, 80],
    ];

    #[inline(always)]
    pub const fn new(alpha: u8) -> Self {
        Self { alpha }
    }

    /// Evaluates whether a fragment at `(x, y)` passes the screen-door threshold test.
    #[inline(always)]
    pub fn test(&self, x: i32, y: i32) -> bool {
        if self.alpha == 255 {
            return true;
        }
        if self.alpha == 0 {
            return false;
        }
        let mx = (x & 3) as usize;
        let my = (y & 3) as usize;
        self.alpha > Self::BAYER_THRESHOLD[my][mx]
    }
}

/// Checkerboard pixel parity mode for 50% fill-rate rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckerboardField {
    /// Draw all pixels (standard).
    #[default]
    Disabled,
    /// Draw pixels where `(x ^ y) & 1 == 0`.
    Even,
    /// Draw pixels where `(x ^ y) & 1 == 1`.
    Odd,
}

impl CheckerboardField {
    /// Returns true if pixel `(x, y)` should be rendered in this field.
    #[inline(always)]
    pub fn includes_pixel(&self, x: i32, y: i32) -> bool {
        match self {
            Self::Disabled => true,
            Self::Even => ((x ^ y) & 1) == 0,
            Self::Odd => ((x ^ y) & 1) != 0,
        }
    }

    /// Toggles between Even and Odd field for alternating frames.
    #[inline]
    pub fn toggle(&self) -> Self {
        match self {
            Self::Disabled => Self::Disabled,
            Self::Even => Self::Odd,
            Self::Odd => Self::Even,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_door_alpha() {
        let full = ScreenDoorConfig::new(255);
        assert!(full.test(0, 0));
        assert!(full.test(1, 2));

        let none = ScreenDoorConfig::new(0);
        assert!(!none.test(0, 0));
        assert!(!none.test(1, 2));

        let half = ScreenDoorConfig::new(128);
        // Half of pixels should pass, half should fail across 4x4 matrix
        let mut passed = 0;
        for y in 0..4 {
            for x in 0..4 {
                if half.test(x, y) {
                    passed += 1;
                }
            }
        }
        assert_eq!(passed, 8);
    }

    #[test]
    fn test_checkerboard_field() {
        let even = CheckerboardField::Even;
        assert!(even.includes_pixel(0, 0));
        assert!(!even.includes_pixel(1, 0));
        assert!(even.includes_pixel(1, 1));
        assert_eq!(even.toggle(), CheckerboardField::Odd);
    }
}
