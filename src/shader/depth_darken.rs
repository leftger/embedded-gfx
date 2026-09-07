//! Cheap depth-based darkening ("Z brightness") for software renderers.
//!
//! This is a clean-room, no-allocation shading helper inspired by the general
//! depth-darkening trick used by tiny embedded rasterisers. It darkens a
//! fragment as it moves away from the camera, giving low-poly scenes extra
//! depth without adding lights, fog buffers, or extra memory.

use core::fmt::Debug;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Configuration for the depth-darkening effect.
#[derive(Debug, Clone, Copy)]
pub struct DepthDarkenConfig {
    /// Maximum darkening amount in 0..=255. `0` disables the effect and `255`
    /// fully darkens fragments at (or beyond) [`Self::far`].
    pub max_darkness: u8,
    /// Depth at which darkening begins (fixed-point 16.16).
    pub near: u32,
    /// Depth at which maximum darkening is reached (fixed-point 16.16).
    pub far: u32,
}

impl DepthDarkenConfig {
    /// Creates a new config from scene-space distances.
    ///
    /// Distances use the same units as the scene depth buffer and are converted
    /// internally to 16.16 fixed point.
    pub fn new(max_darkness: u8, near: f32, far: f32) -> Self {
        Self {
            max_darkness,
            near: (near * 65536.0) as u32,
            far: (far * 65536.0) as u32,
        }
    }

    /// Applies depth darkening to `base_color` for a given depth value.
    #[inline]
    pub fn apply(&self, base_color: Rgb565, depth: u32) -> Rgb565 {
        if self.max_darkness == 0 {
            return base_color;
        }

        // 0 at near, 65536 at far, expressed in 16.16 fixed point.
        let t = if depth <= self.near {
            0u32
        } else if depth >= self.far {
            65536u32
        } else {
            let numerator = (depth - self.near) as u64;
            let denominator = (self.far - self.near).max(1) as u64;
            ((numerator * 65536) / denominator) as u32
        };

        // Scale = 1.0 - (t * max_darkness / 255), again in 16.16 fixed point.
        let dark = ((t as u64 * self.max_darkness as u64) / 255) as u32;
        let scale = 65536u32.saturating_sub(dark);

        let r = ((base_color.r() as u32 * scale) / 65536) as u8;
        let g = ((base_color.g() as u32 * scale) / 65536) as u8;
        let b = ((base_color.b() as u32 * scale) / 65536) as u8;

        Rgb565::new(r, g, b)
    }
}

/// Fragment shader decorator that darkens fragments by depth.
#[derive(Debug, Clone, Copy)]
pub struct DepthDarkenShader<'a, S> {
    /// Inner shader whose output is darkend.
    pub inner: S,
    /// Darkening configuration.
    pub config: &'a DepthDarkenConfig,
}

impl<'a, S: super::FragmentShader> super::FragmentShader for DepthDarkenShader<'a, S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: crate::ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base = self.inner.shade(x, y, z, interps);
        self.config.apply(base, u32::from(z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "depth-u16"))]
    use crate::shader::{FlatColorShader, FragmentShader};

    #[test]
    fn test_depth_darken_interpolation() {
        let config = DepthDarkenConfig::new(255, 1.0, 10.0);
        let color = Rgb565::WHITE;

        // Near or closer stays unchanged.
        assert_eq!(config.apply(color, (1.0 * 65536.0) as u32), color);
        assert_eq!(config.apply(color, 0), color);

        // Far or beyond becomes black at max darkness.
        assert_eq!(config.apply(color, (10.0 * 65536.0) as u32), Rgb565::BLACK);
        assert_eq!(config.apply(color, (20.0 * 65536.0) as u32), Rgb565::BLACK);

        // Halfway at 50% max darkness should be visibly between the endpoints.
        let mid = config.apply(color, (5.5 * 65536.0) as u32);
        assert!(mid.r() > 0 && mid.r() < 20);
        assert!(mid.g() > 0 && mid.g() < 40);
    }

    #[test]
    fn test_zero_max_darkness_is_identity() {
        let config = DepthDarkenConfig::new(0, 1.0, 10.0);
        let color = Rgb565::new(10, 20, 30);
        assert_eq!(config.apply(color, (100.0 * 65536.0) as u32), color);
    }

    #[test]
    #[cfg(not(feature = "depth-u16"))]
    fn test_depth_darken_shader_decorator() {
        let config = DepthDarkenConfig::new(255, 1.0, 5.0);
        let inner = FlatColorShader { color: Rgb565::RED };
        let shader = DepthDarkenShader {
            inner,
            config: &config,
        };

        let near = crate::to_zdepth(65536);
        let far = crate::to_zdepth(5 * 65536);
        assert_eq!(shader.shade(0, 0, near, ()), Rgb565::RED);
        assert_eq!(shader.shade(0, 0, far, ()), Rgb565::BLACK);
    }
}
