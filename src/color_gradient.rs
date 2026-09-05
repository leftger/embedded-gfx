//! Multi-stop color gradient sampling for no_std embedded systems.
//!
//! Inspired by Fyrox's `ColorGradient`, adapted for static flash ROM storage
//! and `Rgb565` / embedded-graphics color types.
//!
//! # Example
//! ```
//! use embedded_3dgfx::color_gradient::{ColorGradient, GradientStop};
//! use embedded_graphics_core::pixelcolor::Rgb565;
//!
//! static FIRE_GRADIENT: ColorGradient<'static, Rgb565> = ColorGradient::new(&[
//!     GradientStop::new(0.0, Rgb565::new(31, 63, 31)), // White hot
//!     GradientStop::new(0.2, Rgb565::new(31, 63, 0)),  // Yellow
//!     GradientStop::new(0.6, Rgb565::new(31, 20, 0)),  // Orange/Red
//!     GradientStop::new(1.0, Rgb565::new(4, 4, 4)),    // Dark smoke
//! ]);
//!
//! let mid_color = FIRE_GRADIENT.sample(0.5);
//! ```

use embedded_graphics_core::pixelcolor::{Rgb565, Rgb888, RgbColor};

/// A single color key in a gradient at a normalized position `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop<C> {
    /// Position along gradient `[0.0, 1.0]`.
    pub location: f32,
    /// Color at this position.
    pub color: C,
}

impl<C> GradientStop<C> {
    /// Create a new gradient stop.
    pub const fn new(location: f32, color: C) -> Self {
        Self { location, color }
    }
}

/// A multi-stop color gradient backed by a slice of [`GradientStop`].
///
/// Can be stored in Flash ROM (`&'static [GradientStop]`) with 0 RAM overhead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorGradient<'a, C> {
    stops: &'a [GradientStop<C>],
}

impl<'a, C> ColorGradient<'a, C> {
    /// Create a gradient referencing a slice of stops.
    ///
    /// Stops should be sorted by `location` in ascending order.
    pub const fn new(stops: &'a [GradientStop<C>]) -> Self {
        Self { stops }
    }

    /// Number of stops in the gradient.
    #[inline]
    pub const fn len(&self) -> usize {
        self.stops.len()
    }

    /// Returns true if the gradient has no stops.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }
}

impl<'a> ColorGradient<'a, Rgb565> {
    /// Sample the gradient at normalized position `t` (`[0.0, 1.0]`).
    pub fn sample(&self, t: f32) -> Rgb565 {
        if self.stops.is_empty() {
            return Rgb565::BLACK;
        }
        if self.stops.len() == 1 {
            return self.stops[0].color;
        }

        let t = t.clamp(0.0, 1.0);

        // Before or at first stop
        if t <= self.stops[0].location {
            return self.stops[0].color;
        }
        // At or past last stop
        if t >= self.stops[self.stops.len() - 1].location {
            return self.stops[self.stops.len() - 1].color;
        }

        // Find segment
        for i in 0..self.stops.len() - 1 {
            let left = &self.stops[i];
            let right = &self.stops[i + 1];

            if t >= left.location && t <= right.location {
                let span = right.location - left.location;
                let alpha = if span > 1e-6 {
                    (t - left.location) / span
                } else {
                    0.0
                };

                let r = lerp_u8(left.color.r(), right.color.r(), alpha, 31);
                let g = lerp_u8(left.color.g(), right.color.g(), alpha, 63);
                let b = lerp_u8(left.color.b(), right.color.b(), alpha, 31);

                return Rgb565::new(r, g, b);
            }
        }

        self.stops[self.stops.len() - 1].color
    }
}

impl<'a> ColorGradient<'a, Rgb888> {
    /// Sample the gradient with 24-bit RGB colors.
    pub fn sample(&self, t: f32) -> Rgb888 {
        if self.stops.is_empty() {
            return Rgb888::BLACK;
        }
        if self.stops.len() == 1 {
            return self.stops[0].color;
        }

        let t = t.clamp(0.0, 1.0);

        if t <= self.stops[0].location {
            return self.stops[0].color;
        }
        if t >= self.stops[self.stops.len() - 1].location {
            return self.stops[self.stops.len() - 1].color;
        }

        for i in 0..self.stops.len() - 1 {
            let left = &self.stops[i];
            let right = &self.stops[i + 1];

            if t >= left.location && t <= right.location {
                let span = right.location - left.location;
                let alpha = if span > 1e-6 {
                    (t - left.location) / span
                } else {
                    0.0
                };

                let r = lerp_u8(left.color.r(), right.color.r(), alpha, 255);
                let g = lerp_u8(left.color.g(), right.color.g(), alpha, 255);
                let b = lerp_u8(left.color.b(), right.color.b(), alpha, 255);

                return Rgb888::new(r, g, b);
            }
        }

        self.stops[self.stops.len() - 1].color
    }
}

#[inline]
fn lerp_u8(start: u8, end: u8, t: f32, max: u8) -> u8 {
    let val = start as f32 + t * (end as f32 - start as f32);
    val.clamp(0.0, max as f32) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gradient_rgb565_sampling() {
        let stops = [
            GradientStop::new(0.0, Rgb565::BLACK),
            GradientStop::new(1.0, Rgb565::WHITE),
        ];
        let grad = ColorGradient::new(&stops);

        assert_eq!(grad.sample(0.0), Rgb565::BLACK);
        assert_eq!(grad.sample(1.0), Rgb565::WHITE);

        let mid = grad.sample(0.5);
        assert_eq!(mid.r(), 15);
        assert_eq!(mid.g(), 31);
        assert_eq!(mid.b(), 15);
    }

    #[test]
    fn test_multi_stop_gradient() {
        let stops = [
            GradientStop::new(0.0, Rgb565::new(31, 0, 0)), // Red
            GradientStop::new(0.5, Rgb565::new(0, 63, 0)), // Green
            GradientStop::new(1.0, Rgb565::new(0, 0, 31)), // Blue
        ];
        let grad = ColorGradient::new(&stops);

        assert_eq!(grad.sample(0.0), Rgb565::new(31, 0, 0));
        assert_eq!(grad.sample(0.5), Rgb565::new(0, 63, 0));
        assert_eq!(grad.sample(1.0), Rgb565::new(0, 0, 31));

        // Sample between 0.0 and 0.5 (red to green)
        let sample_quarter = grad.sample(0.25);
        assert!(sample_quarter.r() > 10 && sample_quarter.r() < 31);
        assert!(sample_quarter.g() > 10 && sample_quarter.g() < 63);
        assert_eq!(sample_quarter.b(), 0);
    }
}
