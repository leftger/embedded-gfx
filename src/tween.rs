//! Time-based interpolation and easing for boot sequences and UI motion.
//!
//! All functions are `no_std` and heap-free. Pair with [`crate::transform_anim::AnimationPlayer`]
//! for keyframed motion, or drive [`K3dMesh`](crate::mesh::K3dMesh) transforms directly each frame.

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Easing curve applied to normalized time `t` in \[0, 1\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    #[default]
    Linear,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    Smoothstep,
}

/// Map raw progress `t` (clamped to \[0, 1\]) through an easing curve.
#[inline]
pub fn apply_easing(t: f32, easing: Easing) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseInCubic => t * t * t,
        Easing::EaseOutCubic => {
            let u = 1.0 - t;
            1.0 - u * u * u
        }
        Easing::EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let u = -2.0 * t + 2.0;
                1.0 - u * u * u * 0.5
            }
        }
        Easing::Smoothstep => t * t * (3.0 - 2.0 * t),
    }
}

/// Linear interpolation between two scalars.
#[inline(always)]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation between two 3D points.
#[inline(always)]
pub fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

/// Scalar tween from `from` to `to` over `duration` seconds.
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub easing: Easing,
}

impl Tween {
    /// Create a tween. `duration` must be positive for motion; zero duration snaps to `to` immediately.
    pub const fn new(from: f32, to: f32, duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: 0.0,
            easing,
        }
    }

    /// Reset elapsed time to zero (replay animation).
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    /// Advance the clock by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        if dt > 0.0 {
            self.elapsed += dt;
        }
    }

    /// Normalized progress in \[0, 1\] before easing.
    #[inline]
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (self.elapsed / self.duration).min(1.0)
    }

    /// Current interpolated value.
    #[inline]
    pub fn value(&self) -> f32 {
        let t = apply_easing(self.progress(), self.easing);
        lerp(self.from, self.to, t)
    }

    /// `true` when elapsed time has reached `duration`.
    #[inline]
    pub fn is_done(&self) -> bool {
        self.duration <= 0.0 || self.elapsed >= self.duration
    }
}

/// 3D position (or any vec3) tween.
#[derive(Debug, Clone, Copy)]
pub struct Tween3 {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub duration: f32,
    pub elapsed: f32,
    pub easing: Easing,
}

impl Tween3 {
    pub const fn new(from: [f32; 3], to: [f32; 3], duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: 0.0,
            easing,
        }
    }

    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    pub fn advance(&mut self, dt: f32) {
        if dt > 0.0 {
            self.elapsed += dt;
        }
    }

    #[inline]
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (self.elapsed / self.duration).min(1.0)
    }

    #[inline]
    pub fn value(&self) -> [f32; 3] {
        let t = apply_easing(self.progress(), self.easing);
        lerp3(self.from, self.to, t)
    }

    #[inline]
    pub fn is_done(&self) -> bool {
        self.duration <= 0.0 || self.elapsed >= self.duration
    }
}

/// Scale an RGB565 color toward black (`factor` = 0 → black, 1 → unchanged).
///
/// Useful for full-screen fade in/out on a finished framebuffer.
#[inline]
pub fn scale_rgb565(color: Rgb565, factor: f32) -> Rgb565 {
    let f = factor.clamp(0.0, 1.0);
    Rgb565::new(
        (color.r() as f32 * f) as u8,
        (color.g() as f32 * f) as u8,
        (color.b() as f32 * f) as u8,
    )
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn test_ease_out_cubic_endpoints() {
        assert!((apply_easing(0.0, Easing::EaseOutCubic) - 0.0).abs() < 1e-5);
        assert!((apply_easing(1.0, Easing::EaseOutCubic) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween_completes() {
        let mut tw = Tween::new(0.0, 10.0, 1.0, Easing::Linear);
        assert!(!tw.is_done());
        tw.advance(0.5);
        assert!((tw.value() - 5.0).abs() < 1e-5);
        tw.advance(0.5);
        assert!(tw.is_done());
        assert!((tw.value() - 10.0).abs() < 1e-5);
    }

    #[test]
    fn test_scale_rgb565() {
        let c = Rgb565::new(30, 60, 30);
        assert_eq!(scale_rgb565(c, 0.0), Rgb565::BLACK);
        assert_eq!(scale_rgb565(c, 1.0), c);
    }

    #[test]
    fn test_tween3_lerp() {
        let mut tw = Tween3::new([0.0, 0.0, 0.0], [2.0, 4.0, 6.0], 2.0, Easing::Linear);
        tw.advance(1.0);
        let v = tw.value();
        assert!((v[0] - 1.0).abs() < 1e-5);
        assert!((v[1] - 2.0).abs() < 1e-5);
        assert!((v[2] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween3_reset_and_is_done() {
        let mut tw = Tween3::new([0.0; 3], [1.0; 3], 1.0, Easing::Linear);
        assert!(!tw.is_done());
        tw.advance(1.0);
        assert!(tw.is_done());
        tw.reset();
        assert!(!tw.is_done());
    }

    #[test]
    fn test_tween3_zero_duration_snaps_to_end() {
        let tw = Tween3::new([0.0; 3], [5.0, 10.0, 15.0], 0.0, Easing::Linear);
        assert!(tw.is_done());
        let v = tw.value();
        assert!((v[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween_zero_duration() {
        let tw = Tween::new(0.0, 42.0, 0.0, Easing::Linear);
        assert!(tw.is_done());
        assert!((tw.value() - 42.0).abs() < 1e-5);
        assert!((tw.progress() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween_negative_advance_noop() {
        let mut tw = Tween::new(0.0, 10.0, 1.0, Easing::Linear);
        tw.advance(-5.0);
        assert!((tw.elapsed - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween3_negative_advance_noop() {
        let mut tw = Tween3::new([0.0; 3], [1.0; 3], 1.0, Easing::Linear);
        tw.advance(-5.0);
        assert!((tw.elapsed - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_tween_reset() {
        let mut tw = Tween::new(0.0, 10.0, 1.0, Easing::Linear);
        tw.advance(1.0);
        assert!(tw.is_done());
        tw.reset();
        assert!(!tw.is_done());
        assert!((tw.elapsed - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_all_easing_variants() {
        // EaseInCubic: t=0.5 → 0.125
        let v = apply_easing(0.5, Easing::EaseInCubic);
        assert!((v - 0.125).abs() < 1e-5);

        // EaseInOutCubic: t=0.5 → 0.5
        let v = apply_easing(0.5, Easing::EaseInOutCubic);
        assert!((v - 0.5).abs() < 1e-4);

        // EaseInOutCubic: t=0.75 (> 0.5 branch)
        let v2 = apply_easing(0.75, Easing::EaseInOutCubic);
        assert!(v2 > 0.5 && v2 < 1.0);

        // Smoothstep: t=0.5 → 0.5
        let v = apply_easing(0.5, Easing::Smoothstep);
        assert!((v - 0.5).abs() < 1e-5);

        // Clamp below 0
        assert!((apply_easing(-1.0, Easing::Linear) - 0.0).abs() < 1e-5);
        // Clamp above 1
        assert!((apply_easing(2.0, Easing::Linear) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_lerp_and_lerp3() {
        assert!((lerp(0.0, 10.0, 0.3) - 3.0).abs() < 1e-5);
        let v = lerp3([0.0, 2.0, 4.0], [10.0, 12.0, 14.0], 0.5);
        assert!((v[0] - 5.0).abs() < 1e-5);
        assert!((v[1] - 7.0).abs() < 1e-5);
        assert!((v[2] - 9.0).abs() < 1e-5);
    }

    #[test]
    fn test_scale_rgb565_half() {
        use embedded_graphics_core::pixelcolor::RgbColor;
        let c = embedded_graphics_core::pixelcolor::Rgb565::new(20, 40, 20);
        let half = scale_rgb565(c, 0.5);
        assert!(half.r() < c.r());
        assert!(half.g() < c.g());
    }
}
