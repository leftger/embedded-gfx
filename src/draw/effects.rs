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

/// Depth bias (polygon offset) configuration to prevent Z-fighting on decals, shadows, and coplanar geometry.
///
/// In 3D rendering (similar to OpenGL `glPolygonOffset` or Vulkan depth bias), depth bias offsets
/// fragment depths before depth comparison and writing.
///
/// Depth offset calculation:
/// `offset = constant + slope_scale * max_slope`
/// where `max_slope = max(|dz/dx|, |dz/dy|)`.
///
/// Negative values bring the primitive closer to the camera (ideal for decals and overlays),
/// while positive values push it further back (ideal for shadow geometry or outlines).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DepthBias {
    /// Constant depth offset applied to fragments (negative pulls closer to camera).
    pub constant: f32,
    /// Factor scaling with the maximum depth slope (gradient) of the primitive.
    pub slope_scale: f32,
}

impl DepthBias {
    /// Zero depth bias (disabled).
    pub const ZERO: Self = Self {
        constant: 0.0,
        slope_scale: 0.0,
    };

    /// Create a new depth bias configuration.
    #[inline(always)]
    pub const fn new(constant: f32, slope_scale: f32) -> Self {
        Self {
            constant,
            slope_scale,
        }
    }

    /// Preconfigured bias for decals (bullet marks, footsteps, blood splatters)
    /// shifting geometry slightly closer to the viewer.
    #[inline(always)]
    pub const fn decal() -> Self {
        Self {
            constant: -0.005,
            slope_scale: -0.01,
        }
    }

    /// Preconfigured bias for planar drop/blob shadows.
    #[inline(always)]
    pub const fn shadow() -> Self {
        Self {
            constant: -0.002,
            slope_scale: -0.005,
        }
    }

    /// Calculate the depth offset in world/clip depth units.
    #[inline]
    pub fn compute_offset(
        &self,
        p1: nalgebra::Point2<i32>,
        p2: nalgebra::Point2<i32>,
        p3: nalgebra::Point2<i32>,
        z1: f32,
        z2: f32,
        z3: f32,
    ) -> f32 {
        if self.constant == 0.0 && self.slope_scale == 0.0 {
            return 0.0;
        }
        let dx1 = (p2.x - p1.x) as f32;
        let dy1 = (p2.y - p1.y) as f32;
        let dz1 = z2 - z1;

        let dx2 = (p3.x - p1.x) as f32;
        let dy2 = (p3.y - p1.y) as f32;
        let dz2 = z3 - z1;

        let det = dx1 * dy2 - dx2 * dy1;
        let slope = if det.abs() > 1e-6 {
            let dzdx = ((dz1 * dy2 - dz2 * dy1) / det).abs();
            let dzdy = ((dx1 * dz2 - dx2 * dz1) / det).abs();
            if dzdx > dzdy { dzdx } else { dzdy }
        } else {
            0.0
        };

        self.constant + self.slope_scale * slope
    }

    /// Apply depth bias to three vertex depths.
    #[inline]
    pub fn apply(
        &self,
        p1: nalgebra::Point2<i32>,
        p2: nalgebra::Point2<i32>,
        p3: nalgebra::Point2<i32>,
        z1: f32,
        z2: f32,
        z3: f32,
    ) -> (f32, f32, f32) {
        let offset = self.compute_offset(p1, p2, p3, z1, z2, z3);
        (
            (z1 + offset).max(0.0),
            (z2 + offset).max(0.0),
            (z3 + offset).max(0.0),
        )
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

    #[test]
    fn test_depth_bias() {
        let p1 = nalgebra::Point2::new(0, 0);
        let p2 = nalgebra::Point2::new(10, 0);
        let p3 = nalgebra::Point2::new(0, 10);

        // Coplanar flat triangle (slope = 0)
        let bias = DepthBias::new(-0.01, -0.05);
        let (z1, z2, z3) = bias.apply(p1, p2, p3, 5.0, 5.0, 5.0);
        assert!((z1 - 4.99).abs() < 1e-4);
        assert!((z2 - 4.99).abs() < 1e-4);
        assert!((z3 - 4.99).abs() < 1e-4);

        // Sloped triangle: dz/dx = (10 - 0) / 10 = 1.0
        let offset = bias.compute_offset(p1, p2, p3, 0.0, 10.0, 0.0);
        // offset = -0.01 + (-0.05 * 1.0) = -0.06
        assert!((offset - (-0.06)).abs() < 1e-4);
    }

    #[test]
    fn test_fog_and_dither_configs() {
        let fog = FogConfig::new(Rgb565::WHITE, 1.0, 3.0);
        assert_eq!(fog.apply(Rgb565::BLACK, 0), Rgb565::BLACK);
        assert_eq!(fog.apply(Rgb565::BLACK, fog.far), Rgb565::WHITE);
        let mid = fog.apply(Rgb565::BLACK, (fog.near + fog.far) / 2);
        assert!(mid.r() > 5 && mid.r() < 31);

        let zero = DitherConfig::new(0);
        let color = Rgb565::new(20, 30, 20);
        assert_eq!(zero.apply(color, 0, 0), color);
        let heavy = DitherConfig::new(255);
        assert_ne!(heavy.apply(color, 3, 3), color);
    }

    #[test]
    fn test_depth_interpolation_and_interlace() {
        let (a, b, c) = DepthInterpolationMode::Exact.process_depths(1.0, 2.0, 3.0);
        assert_eq!((a, b, c), (1.0, 2.0, 3.0));
        let (a, b, c) = DepthInterpolationMode::FastAverage.process_depths(1.0, 2.0, 3.0);
        assert!((a - 2.0).abs() < 1e-5);
        assert_eq!(a, b);
        assert_eq!(b, c);
        let (a, b, c) = DepthInterpolationMode::FastMax.process_depths(1.0, 3.0, 2.0);
        assert_eq!((a, b, c), (3.0, 3.0, 3.0));

        assert!(InterlaceField::Progressive.includes_scanline(0));
        assert!(InterlaceField::Even.includes_scanline(2));
        assert!(!InterlaceField::Even.includes_scanline(1));
        assert!(InterlaceField::Odd.includes_scanline(1));
        assert_eq!(InterlaceField::Even.toggle(), InterlaceField::Odd);
        assert_eq!(
            InterlaceField::Progressive.toggle(),
            InterlaceField::Progressive
        );
    }
}
