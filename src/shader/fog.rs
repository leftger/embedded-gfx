use core::fmt::Debug;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Configuration for depth-based fog effect
#[derive(Debug, Clone, Copy)]
pub struct FogConfig {
    /// Fog color to blend towards
    pub color: Rgb565,
    /// Near plane distance (fixed-point 16.16 format)
    pub near: u32,
    /// Far plane distance (fixed-point 16.16 format)
    pub far: u32,
}

impl FogConfig {
    /// Create a new fog configuration
    ///
    /// # Arguments
    /// * `color` - The fog color
    /// * `near` - Near distance (depth values closer than this have no fog)
    /// * `far` - Far distance (depth values farther than this are fully fogged)
    pub fn new(color: Rgb565, near: f32, far: f32) -> Self {
        Self {
            color,
            near: (near * 65536.0) as u32,
            far: (far * 65536.0) as u32,
        }
    }

    /// Apply fog effect to a color based on depth
    #[inline]
    pub fn apply(&self, base_color: Rgb565, depth: u32) -> Rgb565 {
        // Calculate fog factor: 0.0 at near plane, 1.0 at far plane
        let fog_factor = if depth <= self.near {
            0u32
        } else if depth >= self.far {
            65536u32 // 1.0 in fixed-point
        } else {
            // Linear interpolation: (depth - near) / (far - near)
            let numerator = (depth - self.near) as u64;
            let denominator = (self.far - self.near) as u64;
            ((numerator * 65536) / denominator) as u32
        };

        // Blend base color with fog color
        let base_r = base_color.r() as u32;
        let base_g = base_color.g() as u32;
        let base_b = base_color.b() as u32;

        let fog_r = self.color.r() as u32;
        let fog_g = self.color.g() as u32;
        let fog_b = self.color.b() as u32;

        // fog_factor is in 16.16 fixed-point format
        let r = ((base_r * (65536 - fog_factor) + fog_r * fog_factor) / 65536) as u8;
        let g = ((base_g * (65536 - fog_factor) + fog_g * fog_factor) / 65536) as u8;
        let b = ((base_b * (65536 - fog_factor) + fog_b * fog_factor) / 65536) as u8;

        Rgb565::new(r, g, b)
    }
}

/// Fragment shader decorator that applies depth-based fog.
#[derive(Debug, Clone, Copy)]
pub struct FogShader<'a, S> {
    pub inner: S,
    pub fog: &'a FogConfig,
}

impl<'a, S: super::FragmentShader> super::FragmentShader for FogShader<'a, S> {
    type Interpolants = S::Interpolants;

    #[inline(always)]
    fn shade(&self, x: i32, y: i32, z: crate::ZDepth, interps: Self::Interpolants) -> Rgb565 {
        let base = self.inner.shade(x, y, z, interps);
        self.fog.apply(base, z as u32)
    }
}
