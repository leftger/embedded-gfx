//! Billboard rendering for sprites that always face the camera
//!
//! Billboards are 2D quads that rotate to always face the camera, commonly used for:
//! - Particles (explosions, smoke, sparks)
//! - Vegetation (grass, trees at distance)
//! - UI elements in 3D space
//! - Imposters for distant objects

use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::{Point3, Vector3};

/// A billboard is a 2D quad that always faces the camera
#[derive(Debug, Clone)]
pub struct Billboard {
    /// World-space position of the billboard center
    pub position: Point3<f32>,
    /// Size of the billboard (width and height)
    pub size: f32,
    /// Color of the billboard
    pub color: Rgb565,
    /// Optional texture coordinates (for future texture mapping)
    pub uv: Option<[[f32; 2]; 4]>,
    /// In-plane spin (radians) around the camera view axis — visible rotation on screen.
    pub rotation: f32,
}

impl Billboard {
    /// Create a new billboard at the given position
    pub fn new(position: Point3<f32>, size: f32, color: Rgb565) -> Self {
        Self {
            position,
            size,
            color,
            uv: None,
            rotation: 0.0,
        }
    }

    /// Generate the four corners of the billboard quad facing the camera
    ///
    /// Returns vertices in the order: [bottom-left, bottom-right, top-right, top-left]
    pub fn generate_quad(
        &self,
        camera_position: Point3<f32>,
        camera_up: Vector3<f32>,
    ) -> [[f32; 3]; 4] {
        // Calculate direction from billboard to camera
        let to_camera = (camera_position - self.position).normalize();

        // Calculate right vector (perpendicular to both up and forward)
        let right = camera_up.cross(&to_camera).normalize();

        let up_base = to_camera.cross(&right).normalize();

        // Spin the quad in the plane facing the camera (texture/logo rotation).
        let (s, c) = self.rotation.sin_cos();
        let right = right * c + up_base * s;
        let up = up_base * c - right * s;

        let half_size = self.size * 0.5;

        // Generate quad vertices
        let bottom_left = self.position - right * half_size - up * half_size;
        let bottom_right = self.position + right * half_size - up * half_size;
        let top_right = self.position + right * half_size + up * half_size;
        let top_left = self.position - right * half_size + up * half_size;

        [
            [bottom_left.x, bottom_left.y, bottom_left.z],
            [bottom_right.x, bottom_right.y, bottom_right.z],
            [top_right.x, top_right.y, top_right.z],
            [top_left.x, top_left.y, top_left.z],
        ]
    }

    /// Get the two triangles that make up the billboard quad
    ///
    /// Returns (triangle1, triangle2) as vertex index arrays
    pub fn get_triangles() -> [[usize; 3]; 2] {
        [
            [0, 1, 2], // Bottom-left, bottom-right, top-right
            [0, 2, 3], // Bottom-left, top-right, top-left
        ]
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::WebColors;

    #[test]
    fn test_billboard_creation() {
        let pos = Point3::new(0.0, 0.0, 0.0);
        let billboard = Billboard::new(pos, 1.0, Rgb565::CSS_RED);

        assert_eq!(billboard.position, pos);
        assert_eq!(billboard.size, 1.0);
        assert_eq!(billboard.color, Rgb565::CSS_RED);
    }

    #[test]
    fn test_billboard_quad_generation() {
        let pos = Point3::new(0.0, 0.0, 0.0);
        let billboard = Billboard::new(pos, 2.0, Rgb565::CSS_RED);

        let camera_pos = Point3::new(0.0, 0.0, 5.0);
        let camera_up = Vector3::new(0.0, 1.0, 0.0);

        let quad = billboard.generate_quad(camera_pos, camera_up);

        // Verify we get 4 vertices
        assert_eq!(quad.len(), 4);

        // Verify vertices are centered around position
        let center_x = (quad[0][0] + quad[1][0] + quad[2][0] + quad[3][0]) / 4.0;
        let center_y = (quad[0][1] + quad[1][1] + quad[2][1] + quad[3][1]) / 4.0;
        let center_z = (quad[0][2] + quad[1][2] + quad[2][2] + quad[3][2]) / 4.0;

        assert!((center_x - pos.x).abs() < 0.01);
        assert!((center_y - pos.y).abs() < 0.01);
        assert!((center_z - pos.z).abs() < 0.01);
    }

    #[test]
    fn test_billboard_in_plane_rotation() {
        let pos = Point3::new(0.0, 0.0, 0.0);
        let camera_pos = Point3::new(0.0, 0.0, 5.0);
        let camera_up = Vector3::new(0.0, 1.0, 0.0);

        let flat = Billboard::new(pos, 2.0, Rgb565::CSS_RED);
        let mut spun = Billboard::new(pos, 2.0, Rgb565::CSS_RED);
        spun.rotation = core::f32::consts::FRAC_PI_2;

        let q0 = flat.generate_quad(camera_pos, camera_up);
        let q1 = spun.generate_quad(camera_pos, camera_up);
        assert_ne!(q0, q1);
    }

    #[test]
    fn test_billboard_triangles() {
        let triangles = Billboard::get_triangles();
        assert_eq!(triangles.len(), 2);
        assert_eq!(triangles[0], [0, 1, 2]);
        assert_eq!(triangles[1], [0, 2, 3]);
    }
}
