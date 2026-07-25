//! Billboard rendering for sprites that always face the camera
//!
//! Billboards are 2D quads that rotate to always face the camera, commonly used for:
//! - Particles (explosions, smoke, sparks)
//! - Vegetation (grass, trees at distance)
//! - UI elements in 3D space
//! - Imposters for distant objects

use embedded_graphics_core::pixelcolor::Rgb565;
#[cfg(not(feature = "std"))]
use micromath::F32Ext;
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
        let s = self.rotation.sin();
        let c = self.rotation.cos();
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

    /// Fast billboard transformation using pre-computed camera basis vectors (`right`, `up`).
    ///
    /// Avoids computing normalized vector cross-products for every particle.
    pub fn transform_billboard_fast(
        &self,
        camera_right: Vector3<f32>,
        camera_up: Vector3<f32>,
    ) -> [[f32; 3]; 4] {
        let (right, up) = if self.rotation != 0.0 {
            let s = self.rotation.sin();
            let c = self.rotation.cos();
            let r = camera_right * c + camera_up * s;
            let u = camera_up * c - camera_right * s;
            (r, u)
        } else {
            (camera_right, camera_up)
        };

        let half_size = self.size * 0.5;
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

    /// Get default Q16.16 fixed-point UV coordinates for quad corners.
    ///
    /// Returns [bottom-left, bottom-right, top-right, top-left] in 16.16 fixed point.
    pub fn default_uv_q16() -> [[u32; 2]; 4] {
        [
            [0, 0],
            [65536, 0],
            [65536, 65536],
            [0, 65536],
        ]
    }

    /// Get quad UV coordinates in Q16.16 fixed-point format.
    pub fn get_uv_q16(&self) -> [[u32; 2]; 4] {
        match self.uv {
            Some(uvs) => [
                [(uvs[0][0] * 65536.0) as u32, (uvs[0][1] * 65536.0) as u32],
                [(uvs[1][0] * 65536.0) as u32, (uvs[1][1] * 65536.0) as u32],
                [(uvs[2][0] * 65536.0) as u32, (uvs[2][1] * 65536.0) as u32],
                [(uvs[3][0] * 65536.0) as u32, (uvs[3][1] * 65536.0) as u32],
            ],
            None => Self::default_uv_q16(),
        }
    }

    /// Generate quad positions and Q16.16 fixed-point affine UV coordinates for textured quads.
    pub fn generate_textured_quad_q16(
        &self,
        camera_position: Point3<f32>,
        camera_up: Vector3<f32>,
    ) -> ([[f32; 3]; 4], [[u32; 2]; 4]) {
        let quad = self.generate_quad(camera_position, camera_up);
        let uvs_q16 = self.get_uv_q16();
        (quad, uvs_q16)
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

/// Fixed-point Q16.16 billboard structure for 3D particles and textured quads on embedded microcontrollers.
#[derive(Debug, Clone, Copy)]
pub struct BillboardQ16 {
    /// World position in Q16.16 fixed-point (x, y, z)
    pub position_q16: [i32; 3],
    /// Quad size in Q16.16 fixed-point format
    pub size_q16: u32,
    /// Color of the billboard
    pub color: Rgb565,
    /// Q16.16 fixed-point UV coordinates for quad corners
    pub uv_q16: [[u32; 2]; 4],
}

impl BillboardQ16 {
    /// Create a new Q16.16 fixed-point billboard
    pub fn new(position_q16: [i32; 3], size_q16: u32, color: Rgb565) -> Self {
        Self {
            position_q16,
            size_q16,
            color,
            uv_q16: Billboard::default_uv_q16(),
        }
    }

    /// Convert float position and size to BillboardQ16
    pub fn from_float(position: Point3<f32>, size: f32, color: Rgb565) -> Self {
        Self {
            position_q16: [
                (position.x * 65536.0) as i32,
                (position.y * 65536.0) as i32,
                (position.z * 65536.0) as i32,
            ],
            size_q16: (size * 65536.0) as u32,
            color,
            uv_q16: Billboard::default_uv_q16(),
        }
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

    #[test]
    fn test_billboard_fast_transform() {
        let pos = Point3::new(0.0, 0.0, 0.0);
        let bb = Billboard::new(pos, 2.0, Rgb565::CSS_RED);
        let right = Vector3::new(1.0, 0.0, 0.0);
        let up = Vector3::new(0.0, 1.0, 0.0);

        let quad = bb.transform_billboard_fast(right, up);
        assert_eq!(quad.len(), 4);
        assert_eq!(quad[0], [-1.0, -1.0, 0.0]);
        assert_eq!(quad[2], [1.0, 1.0, 0.0]);
    }

    #[test]
    fn test_billboard_q16() {
        let pos = Point3::new(1.0, 2.0, 3.0);
        let bb_q16 = BillboardQ16::from_float(pos, 4.0, Rgb565::CSS_BLUE);

        assert_eq!(bb_q16.position_q16, [65536, 131072, 196608]);
        assert_eq!(bb_q16.size_q16, 262144);
        assert_eq!(bb_q16.uv_q16[1], [65536, 0]);
    }
}
