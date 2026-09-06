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
use nalgebra::{Point3, Vector3};

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

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
        Rgb565::new(
            crate::simd_dsp::clamp_u5(r as i32),
            crate::simd_dsp::clamp_u6(g as i32),
            crate::simd_dsp::clamp_u5(b as i32),
        )
    }

    /// Accumulate light contribution using inline Reinhard highlight compression.
    ///
    /// `half_exposure`: the accumulated intensity value where the channel compresses
    /// to 50% brightness (e.g. 31 or 63). Highlights roll off smoothly rather
    /// than hard-clamping to pure white.
    pub fn accumulate_tonemapped(&self, world_pos: Point3<f32>, half_exposure: u32) -> Rgb565 {
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        for light in &self.lights {
            let c = light.contribution_at(world_pos);
            r += c.r() as u32;
            g += c.g() as u32;
            b += c.b() as u32;
        }
        tonemap_reinhard_rgb565(r, g, b, half_exposure)
    }
}

impl<const N: usize> Default for PointLightSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute physically-windowed distance attenuation:
/// `(1 - (d/r)^4)^2 / max(d^2, 1.0)`
/// Drops smoothly to exactly zero at `radius` without clipping seams.
#[inline]
pub fn windowed_distance_attenuation(dist_sq: f32, radius: f32) -> f32 {
    let r_sq = radius * radius;
    if dist_sq >= r_sq || radius <= 1e-4 {
        return 0.0;
    }
    let ratio_sq = dist_sq / r_sq;
    let ratio_4 = ratio_sq * ratio_sq;
    let window = (1.0 - ratio_4).max(0.0);
    (window * window) / dist_sq.max(1.0)
}

/// A directional spotlight with inner/outer cone angles and distance falloff.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotLight {
    /// World-space position of the spotlight apex.
    pub position: Point3<f32>,
    /// Normalized direction vector the spotlight points towards.
    pub direction: Vector3<f32>,
    /// Light color.
    pub color: Rgb565,
    /// Maximum range. Points at or beyond receive zero light.
    pub range: f32,
    /// Brightness multiplier.
    pub intensity: f32,
    /// Half-angle of inner cone in radians (full brightness within this cone).
    pub inner_angle: f32,
    /// Half-angle of outer cone in radians (smoothly falls off to zero at outer edge).
    pub outer_angle: f32,
}

impl SpotLight {
    /// Create a new spotlight.
    pub fn new(
        position: Point3<f32>,
        direction: Vector3<f32>,
        color: Rgb565,
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    ) -> Self {
        Self {
            position,
            direction: direction.normalize(),
            color,
            range,
            intensity: 1.0,
            inner_angle,
            outer_angle,
        }
    }

    /// Builder-style intensity override.
    pub fn with_intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Compute the additive RGB565 contribution of this spotlight at `world_pos`.
    #[inline]
    pub fn contribution_at(&self, world_pos: Point3<f32>) -> Rgb565 {
        let to_pos = world_pos - self.position;
        let dist_sq = to_pos.dot(&to_pos);
        let range_sq = self.range * self.range;
        if dist_sq >= range_sq || dist_sq < 1e-6 {
            return Rgb565::new(0, 0, 0);
        }

        let dist = dist_sq.sqrt();
        let dir_to_pos = to_pos / dist;

        // Cosine of angle between spotlight forward vector and direction to surface point
        let cos_theta = dir_to_pos.dot(&self.direction);
        let cos_outer = self.outer_angle.cos();
        let cos_inner = self.inner_angle.cos();

        if cos_theta <= cos_outer {
            return Rgb565::new(0, 0, 0);
        }

        // Spot cone factor (smooth cosine falloff)
        let spot_factor = if cos_inner > cos_outer {
            ((cos_theta - cos_outer) / (cos_inner - cos_outer)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let spot_factor = spot_factor * spot_factor;

        // Distance falloff
        let dist_factor = (1.0 - dist_sq / range_sq).max(0.0);

        let factor = spot_factor * dist_factor * self.intensity;

        let r = ((self.color.r() as f32) * factor).min(31.0) as u8;
        let g = ((self.color.g() as f32) * factor).min(63.0) as u8;
        let b = ((self.color.b() as f32) * factor).min(31.0) as u8;
        Rgb565::new(r, g, b)
    }
}

/// A fixed-capacity collection of [`SpotLight`]s.
pub struct SpotLightSet<const N: usize> {
    pub lights: heapless::Vec<SpotLight, N>,
}

impl<const N: usize> SpotLightSet<N> {
    /// Create an empty set.
    pub const fn new() -> Self {
        Self {
            lights: heapless::Vec::new(),
        }
    }

    /// Add a spotlight. Returns `true` on success, `false` if full.
    pub fn add(&mut self, light: SpotLight) -> bool {
        self.lights.push(light).is_ok()
    }

    /// Remove all spotlights.
    pub fn clear(&mut self) {
        self.lights.clear();
    }

    /// Number of registered spotlights.
    pub fn len(&self) -> usize {
        self.lights.len()
    }

    /// `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.lights.is_empty()
    }

    /// Accumulate total RGB565 contribution at `world_pos`.
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
        Rgb565::new(
            crate::simd_dsp::clamp_u5(r as i32),
            crate::simd_dsp::clamp_u6(g as i32),
            crate::simd_dsp::clamp_u5(b as i32),
        )
    }

    /// Accumulate total contribution using inline Reinhard highlight compression.
    pub fn accumulate_tonemapped(&self, world_pos: Point3<f32>, half_exposure: u32) -> Rgb565 {
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        for light in &self.lights {
            let c = light.contribution_at(world_pos);
            r += c.r() as u32;
            g += c.g() as u32;
            b += c.b() as u32;
        }
        tonemap_reinhard_rgb565(r, g, b, half_exposure)
    }
}

/// Standard Reinhard rational tonemapping: `x / (1.0 + x)`.
#[inline]
pub fn reinhard_tonemap(val: f32) -> f32 {
    if val <= 0.0 { 0.0 } else { val / (1.0 + val) }
}

/// Extended Reinhard tonemapping preserving user-defined maximum white point:
/// `val * (1.0 + val / (white_point^2)) / (1.0 + val)`.
#[inline]
pub fn reinhard_extended_tonemap(val: f32, white_point: f32) -> f32 {
    if val <= 0.0 {
        return 0.0;
    }
    let w_sq = white_point * white_point;
    if w_sq <= 1e-4 {
        return val;
    }
    (val * (1.0 + val / w_sq)) / (1.0 + val)
}

/// Rational exposure tonemapping: `(exposure * val) / (1.0 + exposure * val)`.
///
/// Hardware-friendly on microcontrollers without an FPU; scales exposure
/// without computing transcendental `exp()` functions.
#[inline]
pub fn rational_exposure_tonemap(val: f32, exposure: f32) -> f32 {
    if val <= 0.0 || exposure <= 0.0 {
        return 0.0;
    }
    let scaled = val * exposure;
    scaled / (1.0 + scaled)
}

/// Fast integer Reinhard compression for multi-light accumulation into [`Rgb565`].
///
/// `half_exposure` is the accumulated channel value that yields 50% max channel output.
/// Ensures accumulated multi-light contributions compress smoothly to display range
/// without hue-shifting hard clipping artifacts.
#[inline]
pub fn tonemap_reinhard_rgb565(raw_r: u32, raw_g: u32, raw_b: u32, half_exposure: u32) -> Rgb565 {
    let half_exposure = half_exposure.max(1);
    let r = ((raw_r * 31) / (raw_r + half_exposure)).min(31) as u8;
    let g = ((raw_g * 63) / (raw_g + half_exposure)).min(63) as u8;
    let b = ((raw_b * 31) / (raw_b + half_exposure)).min(31) as u8;
    Rgb565::new(r, g, b)
}

impl<const N: usize> Default for SpotLightSet<N> {
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

    #[test]
    fn test_point_light_analytical_falloff_math() {
        let radius = 10.0f32;
        let light = PointLight::new(Point3::new(0.0, 0.0, 0.0), Rgb565::new(31, 63, 31), radius)
            .with_intensity(1.0);

        // At d = 0.5 * radius (5.0 units away), d^2 / R^2 = 0.25 => factor = 0.75
        let pos_half = Point3::new(5.0, 0.0, 0.0);
        let tint = light.contribution_at(pos_half);

        let expected_r = (31.0 * 0.75) as u8; // 23
        let expected_g = (63.0 * 0.75) as u8; // 47
        let expected_b = (31.0 * 0.75) as u8; // 23

        assert_eq!(tint.r(), expected_r);
        assert_eq!(tint.g(), expected_g);
        assert_eq!(tint.b(), expected_b);
    }

    #[test]
    fn test_windowed_distance_attenuation() {
        assert_eq!(windowed_distance_attenuation(100.0, 10.0), 0.0);
        assert_eq!(windowed_distance_attenuation(0.0, 10.0), 1.0);
        assert!(windowed_distance_attenuation(25.0, 10.0) > 0.0);
    }

    #[test]
    fn test_spot_light_inner_and_outer_cone() {
        let spot = SpotLight::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Rgb565::CSS_WHITE,
            10.0,
            core::f32::consts::FRAC_PI_6, // 30 deg inner
            core::f32::consts::FRAC_PI_4, // 45 deg outer
        );

        // Point straight ahead within range and inner cone
        let tint_center = spot.contribution_at(Point3::new(0.0, 0.0, 2.0));
        assert!(tint_center.r() > 0);

        // Point behind spotlight
        let tint_behind = spot.contribution_at(Point3::new(0.0, 0.0, -2.0));
        assert_eq!(tint_behind.r(), 0);

        // Point far outside 45 deg cone (at 90 deg)
        let tint_side = spot.contribution_at(Point3::new(2.0, 0.0, 0.0));
        assert_eq!(tint_side.r(), 0);
    }

    #[test]
    fn test_tonemapping_curves() {
        assert_eq!(reinhard_tonemap(0.0), 0.0);
        assert!((reinhard_tonemap(1.0) - 0.5).abs() < 1e-4);
        assert!(reinhard_tonemap(100.0) < 1.0);

        assert_eq!(reinhard_extended_tonemap(0.0, 2.0), 0.0);
        // When input equals the maximum white point, output maps to exactly 1.0
        assert!((reinhard_extended_tonemap(2.0, 2.0) - 1.0).abs() < 1e-4);

        assert_eq!(rational_exposure_tonemap(0.0, 1.0), 0.0);
        assert_eq!(rational_exposure_tonemap(1.0, 0.0), 0.0);
        assert!((rational_exposure_tonemap(1.0, 1.0) - 0.5).abs() < 1e-4);
        assert!((rational_exposure_tonemap(2.0, 0.5) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_tonemap_reinhard_rgb565() {
        // Zero light
        let zero = tonemap_reinhard_rgb565(0, 0, 0, 31);
        assert_eq!(zero.r(), 0);
        assert_eq!(zero.g(), 0);
        assert_eq!(zero.b(), 0);

        // Half exposure
        let half = tonemap_reinhard_rgb565(31, 31, 31, 31);
        assert_eq!(half.r(), 15);
        assert_eq!(half.g(), 31);
        assert_eq!(half.b(), 15);

        // Very high light compresses smoothly up to 31 / 63
        let high = tonemap_reinhard_rgb565(1000, 1000, 1000, 31);
        assert_eq!(high.r(), 30);
        assert_eq!(high.g(), 61);
        assert_eq!(high.b(), 30);
    }
}
