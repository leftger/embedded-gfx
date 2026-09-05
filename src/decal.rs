//! Decal projection volumes and grounded blob shadows for no_std embedded systems.
//!
//! Inspired by Fyrox's `fyrox-impl::scene::decal::Decal`, adapted for zero-heap-allocation
//! MCU rendering.
//!
//! Decals project textures (bullet holes, footstep marks, blood splatters, scorch marks)
//! onto existing 3D geometry without modifying the source mesh.
//!
//! # Example
//! ```
//! use embedded_3dgfx::decal::{BlobShadow, DecalProjector};
//! use nalgebra::{Point3, Vector3};
//!
//! let projector = DecalProjector::new(
//!     Point3::new(0.0, 1.0, 0.0),
//!     Vector3::new(0.5, 0.5, 0.5),
//!     Vector3::new(0.0, -1.0, 0.0), // projecting downward
//! );
//!
//! let shadow = BlobShadow::new(Point3::new(0.0, 0.05, 0.0), 0.4);
//! ```

use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::{Point3, Vector3};

/// A decal projection volume defining an oriented bounding box.
#[derive(Debug, Clone, Copy)]
pub struct DecalProjector {
    /// World-space center of projection volume.
    pub position: Point3<f32>,
    /// Half-extents `(half_width, half_height, half_depth)`.
    pub half_extents: Vector3<f32>,
    /// Projection direction (e.g. `(0, -1, 0)` for floor decals).
    pub direction: Vector3<f32>,
}

impl DecalProjector {
    /// Create a new decal projector volume.
    pub fn new(position: Point3<f32>, half_extents: Vector3<f32>, direction: Vector3<f32>) -> Self {
        let dir = if direction.norm_squared() > 1e-6 {
            direction.normalize()
        } else {
            Vector3::new(0.0, -1.0, 0.0)
        };
        Self {
            position,
            half_extents,
            direction: dir,
        }
    }

    /// Check if a 3D point is inside the decal bounding volume.
    pub fn contains_point(&self, p: Point3<f32>) -> bool {
        let diff = p - self.position;
        diff.x.abs() <= self.half_extents.x
            && diff.y.abs() <= self.half_extents.y
            && diff.z.abs() <= self.half_extents.z
    }

    /// Calculate projected UV coordinates `[0.0, 1.0]` for a point inside the volume.
    pub fn project_uv(&self, p: Point3<f32>) -> [f32; 2] {
        let diff = p - self.position;
        let u = ((diff.x / (2.0 * self.half_extents.x)) + 0.5).clamp(0.0, 1.0);
        let v = ((diff.z / (2.0 * self.half_extents.z)) + 0.5).clamp(0.0, 1.0);
        [u, v]
    }
}

/// A circular grounded blob shadow quad for character/object grounding.
#[derive(Debug, Clone, Copy)]
pub struct BlobShadow {
    /// Center of shadow on the ground plane.
    pub center: Point3<f32>,
    /// Radius of shadow.
    pub radius: f32,
    /// Shadow color (typically dark/black or dark tint).
    pub color: Rgb565,
}

impl BlobShadow {
    /// Create a new blob shadow.
    pub const fn new(center: Point3<f32>, radius: f32) -> Self {
        Self {
            center,
            radius,
            color: Rgb565::new(2, 4, 2),
        }
    }

    /// Set shadow color.
    pub const fn with_color(mut self, color: Rgb565) -> Self {
        self.color = color;
        self
    }

    /// Generate a 4-vertex horizontal quad in world space for this shadow.
    pub fn generate_quad_verts(&self) -> [[f32; 3]; 4] {
        let r = self.radius;
        let p = self.center;
        [
            [p.x - r, p.y, p.z - r],
            [p.x + r, p.y, p.z - r],
            [p.x + r, p.y, p.z + r],
            [p.x - r, p.y, p.z + r],
        ]
    }

    /// Generate a 2-triangle face index array.
    pub const fn generate_faces() -> [[usize; 3]; 2] {
        [[0, 1, 2], [0, 2, 3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decal_projector_containment_and_uv() {
        let projector = DecalProjector::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(0.0, -1.0, 0.0),
        );

        assert!(projector.contains_point(Point3::new(0.5, 0.5, 0.5)));
        assert!(!projector.contains_point(Point3::new(1.5, 0.0, 0.0)));

        let uv_center = projector.project_uv(Point3::new(0.0, 0.0, 0.0));
        assert_eq!(uv_center, [0.5, 0.5]);

        let uv_min = projector.project_uv(Point3::new(-1.0, 0.0, -1.0));
        assert_eq!(uv_min, [0.0, 0.0]);

        let uv_max = projector.project_uv(Point3::new(1.0, 0.0, 1.0));
        assert_eq!(uv_max, [1.0, 1.0]);
    }

    #[test]
    fn test_blob_shadow_quad_generation() {
        let shadow = BlobShadow::new(Point3::new(0.0, 0.1, 0.0), 1.0);
        let verts = shadow.generate_quad_verts();
        assert_eq!(verts.len(), 4);
        assert_eq!(verts[0], [-1.0, 0.1, -1.0]);
        assert_eq!(verts[2], [1.0, 0.1, 1.0]);
    }
}
