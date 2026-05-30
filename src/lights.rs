//! Dynamic point lights for runtime illumination.
//!
//! Point lights are evaluated at face/vertex granularity (not per-pixel) to
//! keep cost tractable on microcontrollers.  A typical embedded scene uses
//! 4–16 lights; the engine stores up to 16 internally.
//!
//! # Example
//! ```
//! use embedded_3dgfx::lights::{PointLight, PointLightSet};
//! use nalgebra::Point3;
//! use embedded_graphics_core::pixelcolor::Rgb565;
//!
//! let mut lights: PointLightSet<8> = PointLightSet::new();
//! lights.add(PointLight::new(Point3::new(0.0, 3.0, 0.0), Rgb565::new(31, 63, 31), 5.0));
//! let tint = lights.accumulate(Point3::new(0.0, 0.0, 0.0));
//! ```

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use nalgebra::Point3;

/// A dynamic point light in world space.
///
/// Attenuation uses a squared-distance falloff that avoids a `sqrt` call:
/// `factor = (1 − d²/r²) × intensity`
/// giving a smooth curve from full brightness at the source to zero at
/// `radius`.
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    /// World-space position of the light source.
    pub position: Point3<f32>,
    /// Light colour.  Channels are scaled by `intensity` at sample time.
    pub color: Rgb565,
    /// Influence radius in world units.  Surfaces at or beyond this distance
    /// receive no contribution.
    pub radius: f32,
    /// Brightness multiplier.  `1.0` = full colour at the source centre.
    pub intensity: f32,
}

impl PointLight {
    /// Construct a new point light with `intensity = 1.0`.
    pub fn new(position: Point3<f32>, color: Rgb565, radius: f32) -> Self {
        Self {
            position,
            color,
            radius,
            intensity: 1.0,
        }
    }

    /// Builder-style intensity override.
    pub fn with_intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Compute the additive RGB565 contribution of this light at `world_pos`.
    ///
    /// Returns `Rgb565::new(0, 0, 0)` when `world_pos` is outside the
    /// influence radius.
    #[inline]
    pub fn contribution_at(&self, world_pos: Point3<f32>) -> Rgb565 {
        let diff = world_pos - self.position;
        let dist_sq = diff.dot(&diff);
        let r_sq = self.radius * self.radius;
        if dist_sq >= r_sq {
            return Rgb565::new(0, 0, 0);
        }
        let t = 1.0 - dist_sq / r_sq;
        let factor = t * self.intensity;
        let r = ((self.color.r() as f32) * factor).min(31.0) as u8;
        let g = ((self.color.g() as f32) * factor).min(63.0) as u8;
        let b = ((self.color.b() as f32) * factor).min(31.0) as u8;
        Rgb565::new(r, g, b)
    }
}

/// A fixed-capacity collection of [`PointLight`]s.
///
/// `N` is the maximum number of simultaneous lights (8–16 is typical for
/// embedded targets).
pub struct PointLightSet<const N: usize> {
    pub lights: heapless::Vec<PointLight, N>,
}

impl<const N: usize> PointLightSet<N> {
    /// Create an empty set.
    pub const fn new() -> Self {
        Self {
            lights: heapless::Vec::new(),
        }
    }

    /// Add a light.  Returns `true` on success, `false` if the set is full.
    pub fn add(&mut self, light: PointLight) -> bool {
        self.lights.push(light).is_ok()
    }

    /// Remove all lights.
    pub fn clear(&mut self) {
        self.lights.clear();
    }

    /// Number of lights currently in the set.
    pub fn len(&self) -> usize {
        self.lights.len()
    }

    /// `true` if no lights are registered.
    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Accumulate the additive RGB565 contribution of all lights at
    /// `world_pos`.  Each channel is summed and saturated to its maximum
    /// (R: 31, G: 63, B: 31).
    pub fn accumulate(&self, world_pos: Point3<f32>) -> Rgb565 {
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        for light in &self.lights {
            let c = light.contribution_at(world_pos);
            r += c.r() as u32;
            g += c.g() as u32;
            b += c.b() as u32;
        }
        Rgb565::new(r.min(31) as u8, g.min(63) as u8, b.min(31) as u8)
    }
}

impl<const N: usize> Default for PointLightSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::WebColors;

    #[test]
    fn test_point_light_full_at_center() {
        let light = PointLight::new(Point3::new(0.0, 0.0, 0.0), Rgb565::CSS_WHITE, 5.0);
        let tint = light.contribution_at(Point3::new(0.0, 0.0, 0.0));
        assert_eq!(tint.r(), 31);
        assert_eq!(tint.g(), 63);
        assert_eq!(tint.b(), 31);
    }

    #[test]
    fn test_point_light_zero_outside_radius() {
        let light = PointLight::new(Point3::new(0.0, 0.0, 0.0), Rgb565::CSS_WHITE, 1.0);
        let tint = light.contribution_at(Point3::new(2.0, 0.0, 0.0));
        assert_eq!(tint.r(), 0);
        assert_eq!(tint.g(), 0);
        assert_eq!(tint.b(), 0);
    }

    #[test]
    fn test_point_light_falloff() {
        let light = PointLight::new(Point3::new(0.0, 0.0, 0.0), Rgb565::CSS_WHITE, 10.0);
        let near = light.contribution_at(Point3::new(1.0, 0.0, 0.0));
        let far = light.contribution_at(Point3::new(5.0, 0.0, 0.0));
        assert!(near.r() > far.r());
    }

    #[test]
    fn test_point_light_set_accumulates() {
        let mut set: PointLightSet<4> = PointLightSet::new();
        set.add(PointLight::new(
            Point3::new(0.0, 0.0, 0.0),
            Rgb565::new(10, 20, 10),
            5.0,
        ));
        set.add(PointLight::new(
            Point3::new(0.0, 0.0, 0.0),
            Rgb565::new(5, 10, 5),
            5.0,
        ));
        let tint = set.accumulate(Point3::new(0.0, 0.0, 0.0));
        assert!(tint.r() >= 10);
    }

    #[test]
    fn test_point_light_set_empty() {
        let set: PointLightSet<4> = PointLightSet::new();
        let tint = set.accumulate(Point3::new(0.0, 0.0, 0.0));
        assert_eq!(tint.r(), 0);
        assert_eq!(tint.g(), 0);
        assert_eq!(tint.b(), 0);
    }

    #[test]
    fn test_point_light_set_saturation() {
        let mut set: PointLightSet<4> = PointLightSet::new();
        // Two max-brightness lights at the same point saturate all channels
        set.add(PointLight::new(
            Point3::new(0.0, 0.0, 0.0),
            Rgb565::CSS_WHITE,
            5.0,
        ));
        set.add(PointLight::new(
            Point3::new(0.0, 0.0, 0.0),
            Rgb565::CSS_WHITE,
            5.0,
        ));
        let tint = set.accumulate(Point3::new(0.0, 0.0, 0.0));
        assert_eq!(tint.r(), 31);
        assert_eq!(tint.g(), 63);
        assert_eq!(tint.b(), 31);
    }
}
