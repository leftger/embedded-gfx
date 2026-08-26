//! Axis-aligned bounding boxes for frustum culling and raycast broadphase.
//!
//! Center + half-extents layout with a relative-radius plane test for
//! oriented (model-transformed) boxes.

use nalgebra::{Matrix3, Matrix4, Point3, Vector3};

#[allow(unused_imports)]
use nalgebra::ComplexField;

/// Model-space axis-aligned bounding box (center + half extents).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub center: Vector3<f32>,
    pub half_extents: Vector3<f32>,
}

impl Aabb {
    pub const ZERO: Self = Self {
        center: Vector3::new(0.0, 0.0, 0.0),
        half_extents: Vector3::new(0.0, 0.0, 0.0),
    };

    #[inline]
    pub fn from_min_max(minimum: Vector3<f32>, maximum: Vector3<f32>) -> Self {
        let center = (maximum + minimum) * 0.5;
        let half_extents = (maximum - minimum) * 0.5;
        Self {
            center,
            half_extents,
        }
    }

    /// Smallest AABB enclosing `points`. Returns `None` if the iterator is empty.
    pub fn enclosing<'a, I>(points: I) -> Option<Self>
    where
        I: IntoIterator<Item = &'a [f32; 3]>,
    {
        let mut iter = points.into_iter();
        let first = iter.next()?;
        let mut min = Vector3::new(first[0], first[1], first[2]);
        let mut max = min;
        for p in iter {
            min.x = min.x.min(p[0]);
            min.y = min.y.min(p[1]);
            min.z = min.z.min(p[2]);
            max.x = max.x.max(p[0]);
            max.y = max.y.max(p[1]);
            max.z = max.z.max(p[2]);
        }
        Some(Self::from_min_max(min, max))
    }

    #[inline]
    pub fn min(&self) -> Vector3<f32> {
        self.center - self.half_extents
    }

    #[inline]
    pub fn max(&self) -> Vector3<f32> {
        self.center + self.half_extents
    }

    /// Squared radius of the bounding sphere that encloses this AABB (from center).
    #[inline]
    pub fn radius_sq(&self) -> f32 {
        self.half_extents.norm_squared()
    }

    #[inline]
    pub fn radius(&self) -> f32 {
        self.half_extents.norm()
    }

    /// Union of two AABBs.
    #[inline]
    pub fn merge(self, other: Self) -> Self {
        Self::from_min_max(
            Vector3::new(
                self.min().x.min(other.min().x),
                self.min().y.min(other.min().y),
                self.min().z.min(other.min().z),
            ),
            Vector3::new(
                self.max().x.max(other.max().x),
                self.max().y.max(other.max().y),
                self.max().z.max(other.max().z),
            ),
        )
    }

    /// Relative radius of this AABB projected onto a plane normal after a
    /// linear (3×3) model transform.
    #[inline]
    pub fn relative_radius(
        &self,
        plane_normal: &Vector3<f32>,
        world_from_local: &Matrix3<f32>,
    ) -> f32 {
        let n_arr = [plane_normal.x, plane_normal.y, plane_normal.z];
        let x_dot = crate::simd_dsp::dot3_f32(
            n_arr,
            [
                world_from_local[(0, 0)],
                world_from_local[(1, 0)],
                world_from_local[(2, 0)],
            ],
        );
        let y_dot = crate::simd_dsp::dot3_f32(
            n_arr,
            [
                world_from_local[(0, 1)],
                world_from_local[(1, 1)],
                world_from_local[(2, 1)],
            ],
        );
        let z_dot = crate::simd_dsp::dot3_f32(
            n_arr,
            [
                world_from_local[(0, 2)],
                world_from_local[(1, 2)],
                world_from_local[(2, 2)],
            ],
        );
        crate::simd_dsp::dot3_f32(
            [x_dot.abs(), y_dot.abs(), z_dot.abs()],
            [
                self.half_extents.x,
                self.half_extents.y,
                self.half_extents.z,
            ],
        )
    }

    /// Signed distance from plane `(n·x + d = 0)` to the AABB center in world
    /// space, minus the projected half-extent. Negative means fully outside
    /// the positive half-space (cull candidate when `< 0` for all-outside tests
    /// that use `dist < -r` with spheres).
    ///
    /// Plane coefficients are expected unnormalized; `n_len` is `||n||`.
    #[inline]
    pub fn plane_signed_overshoot(
        &self,
        plane_a: f32,
        plane_b: f32,
        plane_c: f32,
        plane_d: f32,
        n_len: f32,
        model_matrix: &Matrix4<f32>,
    ) -> f32 {
        let rot = model_matrix.fixed_view::<3, 3>(0, 0).into_owned();
        let world_center = model_matrix.transform_point(&Point3::from(self.center));
        let dot = crate::simd_dsp::dot3_f32(
            [plane_a, plane_b, plane_c],
            [
                world_center.coords.x,
                world_center.coords.y,
                world_center.coords.z,
            ],
        );
        let dist = (dot + plane_d) / n_len;
        let inv_n_len = 1.0 / n_len;
        let n_norm = Vector3::new(
            plane_a * inv_n_len,
            plane_b * inv_n_len,
            plane_c * inv_n_len,
        );
        let r = self.relative_radius(&n_norm, &rot);
        dist + r
    }

    /// Transform this model-space AABB to a (conservative) world-space AABB
    /// via Arvo's method (Graphics Gems, 1990).
    #[inline]
    pub fn transformed(&self, model_matrix: &Matrix4<f32>) -> Self {
        let m = model_matrix.fixed_view::<3, 3>(0, 0);
        let t = Vector3::new(
            model_matrix[(0, 3)],
            model_matrix[(1, 3)],
            model_matrix[(2, 3)],
        );
        let mut wc = t;
        let mut we = Vector3::zeros();
        for i in 0..3 {
            for j in 0..3 {
                wc[i] += m[(i, j)] * self.center[j];
                we[i] += m[(i, j)].abs() * self.half_extents[j];
            }
        }
        Self {
            center: wc,
            half_extents: we,
        }
    }

    /// Slab test: does the ray hit this AABB within `[0, max_distance]`?
    ///
    /// Returns `(t_enter, t_exit)` on hit.
    #[inline]
    pub fn intersect_ray(
        &self,
        origin: Vector3<f32>,
        dir: Vector3<f32>,
        max_distance: f32,
    ) -> Option<(f32, f32)> {
        let min = self.min();
        let max = self.max();
        let mut tmin = 0.0f32;
        let mut tmax = max_distance;

        for i in 0..3 {
            let o = origin[i];
            let d = dir[i];
            let mn = min[i];
            let mx = max[i];
            if d.abs() < 1e-8 {
                if o < mn || o > mx {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let mut t0 = (mn - o) * inv;
                let mut t1 = (mx - o) * inv;
                if t0 > t1 {
                    core::mem::swap(&mut t0, &mut t1);
                }
                tmin = tmin.max(t0);
                tmax = tmax.min(t1);
                if tmin > tmax {
                    return None;
                }
            }
        }
        Some((tmin, tmax))
    }

    /// Eight corners of the AABB in local space (for debug wireframes).
    pub fn corners(&self) -> [Vector3<f32>; 8] {
        let c = self.center;
        let h = self.half_extents;
        [
            Vector3::new(c.x - h.x, c.y - h.y, c.z - h.z),
            Vector3::new(c.x + h.x, c.y - h.y, c.z - h.z),
            Vector3::new(c.x + h.x, c.y + h.y, c.z - h.z),
            Vector3::new(c.x - h.x, c.y + h.y, c.z - h.z),
            Vector3::new(c.x - h.x, c.y - h.y, c.z + h.z),
            Vector3::new(c.x + h.x, c.y - h.y, c.z + h.z),
            Vector3::new(c.x + h.x, c.y + h.y, c.z + h.z),
            Vector3::new(c.x - h.x, c.y + h.y, c.z + h.z),
        ]
    }
}

impl Default for Aabb {
    fn default() -> Self {
        Self::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosing_unit_cube() {
        let verts = [[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0], [1.0, -1.0, -1.0]];
        let aabb = Aabb::enclosing(verts.iter()).unwrap();
        assert!((aabb.center.x - 0.0).abs() < 1e-5);
        assert!((aabb.half_extents.x - 1.0).abs() < 1e-5);
        assert!((aabb.half_extents.y - 1.0).abs() < 1e-5);
        assert!((aabb.half_extents.z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn ray_hits_and_misses() {
        let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        let hit = aabb.intersect_ray(
            Vector3::new(0.0, 0.0, -5.0),
            Vector3::new(0.0, 0.0, 1.0),
            100.0,
        );
        assert!(hit.is_some());
        let miss = aabb.intersect_ray(
            Vector3::new(3.0, 0.0, -5.0),
            Vector3::new(0.0, 0.0, 1.0),
            100.0,
        );
        assert!(miss.is_none());
    }

    #[test]
    fn transformed_translation() {
        let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        let m = Matrix4::new_translation(&Vector3::new(5.0, 0.0, 0.0));
        let w = aabb.transformed(&m);
        assert!((w.center.x - 5.0).abs() < 1e-5);
        assert!((w.half_extents.x - 1.0).abs() < 1e-5);
    }
}
