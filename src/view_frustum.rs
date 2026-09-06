//! Gribb-Hartmann 6-plane View Frustum extraction and culling.
//!
//! Inspired by Bevy's `bevy_shape::view_frustum`, adapted for `no_std` execution
//! with `nalgebra`. Extracts the 6 frustum half-spaces directly from the 4x4 View-Projection
//! matrix without matrix inversions, and provides branch-light Sphere and AABB culling tests.

#[cfg(feature = "aabb-cull")]
use crate::bounds::Aabb;
use nalgebra::{Matrix4, Vector3};

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// A 3D half-space (plane) defined by unit normal and signed distance `d`:
/// `normal.dot(p) + d = 0`.
///
/// In this convention, points with `signed_distance >= 0.0` lie on the interior side of the half-space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfSpace {
    pub normal: Vector3<f32>,
    pub d: f32,
}

impl HalfSpace {
    /// Construct a normalized half-space from plane equation `(a, b, c, d)`:
    /// `a * x + b * y + c * z + d = 0`.
    #[inline]
    pub fn new(a: f32, b: f32, c: f32, d: f32) -> Self {
        let len = (a * a + b * b + c * c).sqrt();
        if len > 1e-6 {
            let inv_len = 1.0 / len;
            Self {
                normal: Vector3::new(a * inv_len, b * inv_len, c * inv_len),
                d: d * inv_len,
            }
        } else {
            Self {
                normal: Vector3::new(0.0, 1.0, 0.0),
                d: 0.0,
            }
        }
    }

    /// Signed distance from the plane to `point`.
    /// Positive if inside, negative if outside.
    #[inline]
    pub fn signed_distance(&self, point: &Vector3<f32>) -> f32 {
        self.normal.dot(point) + self.d
    }

    /// Returns `true` if `point` is on the positive / interior side of the plane.
    #[inline]
    pub fn contains_point(&self, point: &Vector3<f32>) -> bool {
        self.signed_distance(point) >= 0.0
    }
}

/// A 6-plane view frustum enclosing the visible camera volume.
///
/// Planes are ordered: [Left, Right, Bottom, Top, Near, Far].
/// All plane normals point towards the inside of the frustum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewFrustum {
    pub planes: [HalfSpace; 6],
}

impl ViewFrustum {
    pub const LEFT_PLANE: usize = 0;
    pub const RIGHT_PLANE: usize = 1;
    pub const BOTTOM_PLANE: usize = 2;
    pub const TOP_PLANE: usize = 3;
    pub const NEAR_PLANE: usize = 4;
    pub const FAR_PLANE: usize = 5;

    /// Extract the 6 frustum planes directly from a View-Projection matrix using the Gribb-Hartmann method.
    ///
    /// Works with perspective, orthographic, and oblique cameras.
    pub fn from_view_projection(vp: &Matrix4<f32>) -> Self {
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);

        // Plane equations from matrix rows:
        // Left:   r3 + r0
        // Right:  r3 - r0
        // Bottom: r3 + r1
        // Top:    r3 - r1
        // Near:   r3 + r2
        // Far:    r3 - r2
        let p_left = r3 + r0;
        let p_right = r3 - r0;
        let p_bottom = r3 + r1;
        let p_top = r3 - r1;
        let p_near = r3 + r2;
        let p_far = r3 - r2;

        Self {
            planes: [
                HalfSpace::new(p_left[0], p_left[1], p_left[2], p_left[3]),
                HalfSpace::new(p_right[0], p_right[1], p_right[2], p_right[3]),
                HalfSpace::new(p_bottom[0], p_bottom[1], p_bottom[2], p_bottom[3]),
                HalfSpace::new(p_top[0], p_top[1], p_top[2], p_top[3]),
                HalfSpace::new(p_near[0], p_near[1], p_near[2], p_near[3]),
                HalfSpace::new(p_far[0], p_far[1], p_far[2], p_far[3]),
            ],
        }
    }

    /// Fast Sphere vs Frustum test.
    ///
    /// Returns `true` if the sphere intersects or is inside the frustum;
    /// `false` if the sphere is completely outside any plane.
    #[inline]
    pub fn intersects_sphere(&self, center: &Vector3<f32>, radius: f32) -> bool {
        for plane in &self.planes {
            if plane.signed_distance(center) < -radius {
                return false;
            }
        }
        true
    }

    /// Fast AABB vs Frustum culling test using Arvo's positive vertex evaluation.
    ///
    /// Returns `true` if the AABB intersects or is inside the frustum;
    /// `false` if the AABB is completely outside any plane.
    #[cfg(feature = "aabb-cull")]
    #[inline]
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        let min = aabb.min();
        let max = aabb.max();

        for plane in &self.planes {
            // Find positive vertex (the corner furthest along the plane normal)
            let p_x = if plane.normal.x >= 0.0 { max.x } else { min.x };
            let p_y = if plane.normal.y >= 0.0 { max.y } else { min.y };
            let p_z = if plane.normal.z >= 0.0 { max.z } else { min.z };

            if plane.signed_distance(&Vector3::new(p_x, p_y, p_z)) < 0.0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_half_space_distance() {
        let plane = HalfSpace::new(0.0, 1.0, 0.0, 5.0); // y + 5 = 0
        assert_eq!(plane.signed_distance(&Vector3::new(0.0, 0.0, 0.0)), 5.0);
        assert_eq!(plane.signed_distance(&Vector3::new(0.0, -5.0, 0.0)), 0.0);
        assert_eq!(plane.signed_distance(&Vector3::new(0.0, -10.0, 0.0)), -5.0);
    }

    #[test]
    fn test_frustum_sphere_culling() {
        // Simple orthographic-like frustum: box [-10, 10] along all axes
        let frustum = ViewFrustum {
            planes: [
                HalfSpace::new(1.0, 0.0, 0.0, 10.0),  // Left: x >= -10
                HalfSpace::new(-1.0, 0.0, 0.0, 10.0), // Right: x <= 10
                HalfSpace::new(0.0, 1.0, 0.0, 10.0),  // Bottom: y >= -10
                HalfSpace::new(0.0, -1.0, 0.0, 10.0), // Top: y <= 10
                HalfSpace::new(0.0, 0.0, 1.0, 10.0),  // Near: z >= -10
                HalfSpace::new(0.0, 0.0, -1.0, 10.0), // Far: z <= 10
            ],
        };

        // Center inside
        assert!(frustum.intersects_sphere(&Vector3::zeros(), 1.0));
        // Outside to the right (x = 15, radius = 2 -> reaches down to x = 13 > 10)
        assert!(!frustum.intersects_sphere(&Vector3::new(15.0, 0.0, 0.0), 2.0));
        // Straddling right plane (x = 11, radius = 2 -> reaches down to x = 9 < 10)
        assert!(frustum.intersects_sphere(&Vector3::new(11.0, 0.0, 0.0), 2.0));
    }

    #[cfg(feature = "aabb-cull")]
    #[test]
    fn test_frustum_aabb_culling() {
        let frustum = ViewFrustum {
            planes: [
                HalfSpace::new(1.0, 0.0, 0.0, 10.0),
                HalfSpace::new(-1.0, 0.0, 0.0, 10.0),
                HalfSpace::new(0.0, 1.0, 0.0, 10.0),
                HalfSpace::new(0.0, -1.0, 0.0, 10.0),
                HalfSpace::new(0.0, 0.0, 1.0, 10.0),
                HalfSpace::new(0.0, 0.0, -1.0, 10.0),
            ],
        };

        let inside =
            Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        assert!(frustum.intersects_aabb(&inside));

        let outside =
            Aabb::from_min_max(Vector3::new(12.0, 0.0, 0.0), Vector3::new(15.0, 2.0, 2.0));
        assert!(!frustum.intersects_aabb(&outside));
    }
}
