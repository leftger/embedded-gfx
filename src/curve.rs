//! Piecewise spline curve evaluation for animations, tweens, and particle parameter ramps.
//!
//! Inspired by Fyrox's `fyrox-math::curve::Curve`, adapted for `no_std` embedded systems.
//!
//! # Example
//! ```
//! use embedded_3dgfx::curve::{Curve, CurveInterpolation, CurveKey};
//!
//! static BOUNCE_CURVE: Curve<'static, f32> = Curve::new(&[
//!     CurveKey::new(0.0, 0.0, CurveInterpolation::Hermite { in_tangent: 0.0, out_tangent: 2.0 }),
//!     CurveKey::new(0.5, 1.0, CurveInterpolation::Hermite { in_tangent: 0.0, out_tangent: 0.0 }),
//!     CurveKey::new(1.0, 0.0, CurveInterpolation::Hermite { in_tangent: -2.0, out_tangent: 0.0 }),
//! ]);
//!
//! let val = BOUNCE_CURVE.sample(0.25);
//! ```

/// Interpolation mode between curve keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CurveInterpolation {
    /// Holds the constant value of the key until the next key.
    Step,
    /// Linear interpolation between keys.
    #[default]
    Linear,
    /// Cubic Hermite spline interpolation with incoming and outgoing tangents.
    Hermite {
        /// Incoming tangent slope.
        in_tangent: f32,
        /// Outgoing tangent slope.
        out_tangent: f32,
    },
}

/// A keyframe on a parametric curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveKey<T> {
    /// Time/position along the curve.
    pub time: f32,
    /// Value at this keyframe.
    pub value: T,
    /// Interpolation curve to the next keyframe.
    pub interpolation: CurveInterpolation,
}

impl<T> CurveKey<T> {
    /// Create a new curve keyframe.
    pub const fn new(time: f32, value: T, interpolation: CurveInterpolation) -> Self {
        Self {
            time,
            value,
            interpolation,
        }
    }

    /// Create a linear keyframe.
    pub const fn linear(time: f32, value: T) -> Self {
        Self {
            time,
            value,
            interpolation: CurveInterpolation::Linear,
        }
    }

    /// Create a step keyframe.
    pub const fn step(time: f32, value: T) -> Self {
        Self {
            time,
            value,
            interpolation: CurveInterpolation::Step,
        }
    }
}

/// A piecewise parametric curve backed by a sorted slice of keyframes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve<'a, T> {
    keys: &'a [CurveKey<T>],
}

impl<'a, T> Curve<'a, T> {
    /// Create a new curve from a slice of keys.
    pub const fn new(keys: &'a [CurveKey<T>]) -> Self {
        Self { keys }
    }

    /// Returns the number of keyframes.
    #[inline]
    pub const fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns true if the curve has no keyframes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl<'a> Curve<'a, f32> {
    /// Sample the curve at the specified `time`.
    pub fn sample(&self, time: f32) -> f32 {
        if self.keys.is_empty() {
            return 0.0;
        }
        if self.keys.len() == 1 {
            return self.keys[0].value;
        }

        // Before first key
        if time <= self.keys[0].time {
            return self.keys[0].value;
        }
        // At or after last key
        if time >= self.keys[self.keys.len() - 1].time {
            return self.keys[self.keys.len() - 1].value;
        }

        // Find surrounding key segment
        for i in 0..self.keys.len() - 1 {
            let left = &self.keys[i];
            let right = &self.keys[i + 1];

            if time >= left.time && time <= right.time {
                let dt = right.time - left.time;
                if dt <= 1e-6 {
                    return left.value;
                }

                let t = (time - left.time) / dt;

                return match left.interpolation {
                    CurveInterpolation::Step => left.value,
                    CurveInterpolation::Linear => left.value + t * (right.value - left.value),
                    CurveInterpolation::Hermite { out_tangent, .. } => {
                        let in_tangent = match right.interpolation {
                            CurveInterpolation::Hermite { in_tangent, .. } => in_tangent,
                            _ => 0.0,
                        };

                        // Cubic Hermite basis functions
                        let t2 = t * t;
                        let t3 = t2 * t;

                        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                        let h10 = t3 - 2.0 * t2 + t;
                        let h01 = -2.0 * t3 + 3.0 * t2;
                        let h11 = t3 - t2;

                        h00 * left.value
                            + h10 * dt * out_tangent
                            + h01 * right.value
                            + h11 * dt * in_tangent
                    }
                };
            }
        }

        self.keys[self.keys.len() - 1].value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_curve() {
        let keys = [
            CurveKey::linear(0.0, 0.0),
            CurveKey::linear(1.0, 10.0),
            CurveKey::linear(2.0, 5.0),
        ];
        let curve = Curve::new(&keys);

        assert_eq!(curve.sample(0.0), 0.0);
        assert_eq!(curve.sample(0.5), 5.0);
        assert_eq!(curve.sample(1.0), 10.0);
        assert_eq!(curve.sample(1.5), 7.5);
        assert_eq!(curve.sample(2.0), 5.0);
    }

    #[test]
    fn test_hermite_curve() {
        let keys = [
            CurveKey::new(
                0.0,
                0.0,
                CurveInterpolation::Hermite {
                    in_tangent: 0.0,
                    out_tangent: 0.0,
                },
            ),
            CurveKey::new(
                1.0,
                1.0,
                CurveInterpolation::Hermite {
                    in_tangent: 0.0,
                    out_tangent: 0.0,
                },
            ),
        ];
        let curve = Curve::new(&keys);

        assert_eq!(curve.sample(0.0), 0.0);
        assert_eq!(curve.sample(1.0), 1.0);
        // Smooth S-curve at midpoint should be 0.5
        assert!((curve.sample(0.5) - 0.5).abs() < 1e-5);
    }
}
