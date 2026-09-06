//! Analytical ray-primitive intersection tests.
//!
//! Inspired by Bevy's `bevy_shape::bounding::raycast3d`, adapted for zero-heap
//! `no_std` execution. Provides closed-form, exact intersection tests for volumetric
//! and planar primitives (Sphere, Capsule, Cylinder, AABB, Plane, and Disc) without
//! requiring triangle mesh generation.

#[cfg(feature = "aabb-cull")]
use crate::bounds::Aabb;
use crate::camera::Ray;
use nalgebra::Vector3;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// Result of an intersection test between a [`Ray`] and a geometric primitive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Distance from ray origin to intersection point along `ray.direction`:
    /// `point = ray.origin + ray.direction * distance`.
    pub distance: f32,
    /// World-space position of the hit.
    pub point: Vector3<f32>,
    /// Outward unit surface normal at the hit position.
    pub normal: Vector3<f32>,
}

/// Ray vs Sphere analytical intersection.
///
/// Returns the nearest positive intersection, or `None` if the ray misses or points away.
pub fn ray_intersects_sphere(ray: &Ray, center: Vector3<f32>, radius: f32) -> Option<RayHit> {
    if radius <= 0.0 {
        return None;
    }

    let m = ray.origin - center;
    let b = m.dot(&ray.direction);
    let c = m.dot(&m) - radius * radius;

    // Exit early if ray origin is outside sphere and pointing away
    if c > 0.0 && b > 0.0 {
        return None;
    }

    let discr = b * b - c;
    if discr < 0.0 {
        return None;
    }

    let sqrt_discr = discr.sqrt();
    let mut t = -b - sqrt_discr;

    // If t < 0, ray started inside the sphere; use the exit point
    if t < 0.0 {
        t = -b + sqrt_discr;
        if t < 0.0 {
            return None;
        }
    }

    let point = ray.origin + ray.direction * t;
    let normal = (point - center) / radius;

    Some(RayHit {
        distance: t,
        point,
        normal,
    })
}

/// Ray vs Infinite Plane analytical intersection.
///
/// Plane is defined by point `plane_point` and unit normal `plane_normal`.
pub fn ray_intersects_plane(
    ray: &Ray,
    plane_point: Vector3<f32>,
    plane_normal: Vector3<f32>,
) -> Option<RayHit> {
    let denom = ray.direction.dot(&plane_normal);
    if denom.abs() < 1e-6 {
        return None; // Ray is parallel to the plane
    }

    let t = (plane_point - ray.origin).dot(&plane_normal) / denom;
    if t < 0.0 {
        return None; // Hit is behind the ray origin
    }

    let point = ray.origin + ray.direction * t;
    let normal = if denom < 0.0 {
        plane_normal
    } else {
        -plane_normal
    };

    Some(RayHit {
        distance: t,
        point,
        normal,
    })
}

/// Ray vs Flat Disc intersection.
pub fn ray_intersects_disc(
    ray: &Ray,
    center: Vector3<f32>,
    normal: Vector3<f32>,
    radius: f32,
) -> Option<RayHit> {
    let hit = ray_intersects_plane(ray, center, normal)?;
    let offset = hit.point - center;
    if offset.dot(&offset) <= radius * radius {
        Some(hit)
    } else {
        None
    }
}

/// Ray vs Axis-Aligned Bounding Box (AABB) intersection using the Kay-Kajiya slab method.
#[cfg(feature = "aabb-cull")]
pub fn ray_intersects_aabb(ray: &Ray, aabb: &Aabb) -> Option<RayHit> {
    let min = aabb.min();
    let max = aabb.max();

    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;
    let mut hit_normal = Vector3::zeros();

    for i in 0..3 {
        let origin_i = ray.origin[i];
        let dir_i = ray.direction[i];
        let min_i = min[i];
        let max_i = max[i];

        if dir_i.abs() < 1e-6 {
            // Ray parallel to slab; if origin is outside slab, no hit
            if origin_i < min_i || origin_i > max_i {
                return None;
            }
        } else {
            let inv_d = 1.0 / dir_i;
            let mut t0 = (min_i - origin_i) * inv_d;
            let mut t1 = (max_i - origin_i) * inv_d;

            let mut normal_i = -1.0;
            if t0 > t1 {
                core::mem::swap(&mut t0, &mut t1);
                normal_i = 1.0;
            }

            if t0 > t_min {
                t_min = t0;
                hit_normal = Vector3::zeros();
                hit_normal[i] = normal_i;
            }
            t_max = t_max.min(t1);

            if t_min > t_max {
                return None;
            }
        }
    }

    let t = if t_min >= 0.0 {
        t_min
    } else if t_max >= 0.0 {
        // Ray started inside AABB
        t_max
    } else {
        return None;
    };

    Some(RayHit {
        distance: t,
        point: ray.origin + ray.direction * t,
        normal: hit_normal,
    })
}

/// Ray vs 3D Capsule (line segment between `a` and `b` dilated by `radius`).
pub fn ray_intersects_capsule(
    ray: &Ray,
    a: Vector3<f32>,
    b: Vector3<f32>,
    radius: f32,
) -> Option<RayHit> {
    if radius <= 0.0 {
        return None;
    }

    let ab = b - a;
    let ab_len_sq = ab.dot(&ab);
    if ab_len_sq < 1e-6 {
        return ray_intersects_sphere(ray, a, radius);
    }

    // Check caps (spheres at A and B)
    let mut closest_hit: Option<RayHit> = None;

    if let Some(hit) = ray_intersects_sphere(ray, a, radius) {
        if (hit.point - a).dot(&ab) <= 0.0 {
            closest_hit = Some(hit);
        }
    }

    if let Some(hit) = ray_intersects_sphere(ray, b, radius) {
        if (hit.point - b).dot(&ab) >= 0.0 {
            if closest_hit.is_none() || hit.distance < closest_hit.unwrap().distance {
                closest_hit = Some(hit);
            }
        }
    }

    // Cylinder body test between parallel planes at A and B
    let ao = ray.origin - a;
    let ab_unit = ab / ab_len_sq.sqrt();

    let d_proj = ray.direction - ab_unit * ray.direction.dot(&ab_unit);
    let o_proj = ao - ab_unit * ao.dot(&ab_unit);

    let quad_a = d_proj.dot(&d_proj);
    let quad_b = 2.0 * d_proj.dot(&o_proj);
    let quad_c = o_proj.dot(&o_proj) - radius * radius;

    if quad_a > 1e-6 {
        let discr = quad_b * quad_b - 4.0 * quad_a * quad_c;
        if discr >= 0.0 {
            let sqrt_discr = discr.sqrt();
            let mut t = (-quad_b - sqrt_discr) / (2.0 * quad_a);
            if t < 0.0 {
                t = (-quad_b + sqrt_discr) / (2.0 * quad_a);
            }

            if t >= 0.0 {
                let hit_pt = ray.origin + ray.direction * t;
                let seg_t = (hit_pt - a).dot(&ab) / ab_len_sq;
                if (0.0..=1.0).contains(&seg_t) {
                    let seg_pt = a + ab * seg_t;
                    let norm = (hit_pt - seg_pt) / radius;
                    let body_hit = RayHit {
                        distance: t,
                        point: hit_pt,
                        normal: norm,
                    };
                    if closest_hit.is_none() || body_hit.distance < closest_hit.unwrap().distance {
                        closest_hit = Some(body_hit);
                    }
                }
            }
        }
    }

    closest_hit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_sphere_hit() {
        let ray = Ray::new(Vector3::new(0.0, 0.0, -5.0), Vector3::new(0.0, 0.0, 1.0));
        let hit = ray_intersects_sphere(&ray, Vector3::new(0.0, 0.0, 0.0), 1.0);
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert!((h.distance - 4.0).abs() < 1e-4);
        assert!((h.point - Vector3::new(0.0, 0.0, -1.0)).norm() < 1e-4);
        assert!((h.normal - Vector3::new(0.0, 0.0, -1.0)).norm() < 1e-4);
    }

    #[test]
    fn test_ray_sphere_miss() {
        let ray = Ray::new(Vector3::new(0.0, 5.0, -5.0), Vector3::new(0.0, 0.0, 1.0));
        let hit = ray_intersects_sphere(&ray, Vector3::new(0.0, 0.0, 0.0), 1.0);
        assert!(hit.is_none());
    }

    #[test]
    fn test_ray_plane() {
        let ray = Ray::new(Vector3::new(0.0, 10.0, 0.0), Vector3::new(0.0, -1.0, 0.0));
        let hit = ray_intersects_plane(
            &ray,
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
        );
        assert!(hit.is_some());
        assert!((hit.unwrap().distance - 10.0).abs() < 1e-4);
    }

    #[cfg(feature = "aabb-cull")]
    #[test]
    fn test_ray_aabb() {
        let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        let ray = Ray::new(Vector3::new(0.0, 0.0, -5.0), Vector3::new(0.0, 0.0, 1.0));
        let hit = ray_intersects_aabb(&ray, &aabb);
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert!((h.distance - 4.0).abs() < 1e-4);
        assert!((h.normal - Vector3::new(0.0, 0.0, -1.0)).norm() < 1e-4);
    }

    #[test]
    fn test_ray_capsule() {
        let ray = Ray::new(Vector3::new(0.0, 0.0, -5.0), Vector3::new(0.0, 0.0, 1.0));
        let hit = ray_intersects_capsule(
            &ray,
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            0.5,
        );
        assert!(hit.is_some());
        assert!((hit.unwrap().distance - 4.5).abs() < 1e-3);
    }
}
