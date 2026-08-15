use core::f32::consts;

#[cfg(feature = "render-layers")]
use crate::render_layers::RenderLayers;
use embedded_graphics_core::geometry::Point;
use nalgebra::{Isometry3, Perspective3, Point3, Vector3};

/// A 3D ray defined by origin and direction vectors.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Ray origin point in world space.
    pub origin: Vector3<f32>,
    /// Normalized ray direction vector.
    pub direction: Vector3<f32>,
}

impl Ray {
    /// Create a new ray with origin and normalized direction.
    pub fn new(origin: Vector3<f32>, direction: Vector3<f32>) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    /// Construct a 3D ray unprojected from 2D screen coordinates via a [`Camera`].
    pub fn from_screen_point(point: Point, camera: &Camera, width: usize, height: usize) -> Self {
        let ndc_x = (2.0 * point.x as f32 / width as f32) - 1.0;
        let ndc_y = 1.0 - (2.0 * point.y as f32 / height as f32);

        let inv_vp = camera
            .vp_matrix
            .try_inverse()
            .unwrap_or(nalgebra::Matrix4::identity());
        let near_h = inv_vp * nalgebra::Vector4::new(ndc_x, ndc_y, -1.0, 1.0);
        let far_h = inv_vp * nalgebra::Vector4::new(ndc_x, ndc_y, 1.0, 1.0);

        let near_pt = Vector3::new(near_h.x, near_h.y, near_h.z) / near_h.w.abs().max(1e-6);
        let far_pt = Vector3::new(far_h.x, far_h.y, far_h.z) / far_h.w.abs().max(1e-6);

        let direction = (far_pt - near_pt).normalize();
        Self {
            origin: camera.position.coords,
            direction,
        }
    }
}

pub struct Camera {
    pub position: Point3<f32>,
    fov: f32,
    pub near: f32,
    pub far: f32,
    pub view_matrix: nalgebra::Matrix4<f32>,
    projection_matrix: nalgebra::Matrix4<f32>,
    pub vp_matrix: nalgebra::Matrix4<f32>,
    target: Point3<f32>,
    aspect_ratio: f32,
    /// Visibility layers this camera sees. Default: layer 0.
    #[cfg(feature = "render-layers")]
    pub layers: RenderLayers,
}

impl Camera {
    pub fn new(aspect_ratio: f32) -> Camera {
        let mut ret = Camera {
            position: Point3::new(0.0, 0.0, 0.0),
            fov: consts::PI / 2.0,
            view_matrix: nalgebra::Matrix4::identity(),
            projection_matrix: nalgebra::Matrix4::identity(),
            vp_matrix: nalgebra::Matrix4::identity(),
            target: Point3::new(0.0, 0.0, 0.0),
            aspect_ratio,
            near: 0.4,
            far: 20.0,
            #[cfg(feature = "render-layers")]
            layers: RenderLayers::DEFAULT,
        };

        ret.update_projection();

        ret
    }

    pub fn set_position(&mut self, pos: Point3<f32>) {
        self.position = pos;

        self.update_view();
    }

    pub fn set_fovy(&mut self, fovy: f32) {
        self.fov = fovy;

        self.update_projection();
    }

    /// Vertical field of view in radians.
    pub fn fovy(&self) -> f32 {
        self.fov
    }

    /// Restrict which mesh layers this camera sees.
    #[cfg(feature = "render-layers")]
    pub fn set_layers(&mut self, layers: RenderLayers) {
        self.layers = layers;
    }

    pub fn set_near(&mut self, near: f32) {
        self.near = near;

        self.update_projection();
    }

    pub fn set_far(&mut self, far: f32) {
        self.far = far;

        self.update_projection();
    }

    /// Set both near and far planes (for better Z-buffer precision)
    ///
    /// **Important**: Keep the near/far ratio as small as possible to reduce Z-fighting.
    /// A ratio of 20:1 or less is recommended. For example:
    /// - Small scene (0.5-10 units): near=0.5, far=10.0 (20:1)
    /// - Medium scene (1-15 units): near=1.0, far=15.0 (15:1)
    /// - Large scene (2-20 units): near=2.0, far=20.0 (10:1)
    ///
    /// See `ZBUFFER_TUNING.md` for detailed guidance.
    pub fn set_near_far(&mut self, near: f32, far: f32) {
        self.near = near;
        self.far = far;

        self.update_projection();
    }

    /// Get the current near/far ratio (lower is better for Z-buffer precision)
    pub fn get_near_far_ratio(&self) -> f32 {
        self.far / self.near
    }

    pub fn set_target(&mut self, target: Point3<f32>) {
        self.target = target;
        self.update_view();
    }

    pub fn get_direction(&self) -> Vector3<f32> {
        let transpose = self.view_matrix; //.transpose();

        Vector3::new(transpose[(2, 0)], transpose[(2, 1)], transpose[(2, 2)])
    }

    pub fn get_aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    fn update_view(&mut self) {
        let view = Isometry3::look_at_rh(&self.position, &self.target, &Vector3::y());

        self.view_matrix = view.to_homogeneous();
        self.vp_matrix = self.projection_matrix * self.view_matrix;
    }

    fn update_projection(&mut self) {
        let projection = Perspective3::new(self.aspect_ratio, self.fov, self.near, self.far);
        self.projection_matrix = projection.to_homogeneous();
        self.vp_matrix = self.projection_matrix * self.view_matrix;
    }

    #[cfg(feature = "dsp")]
    /// Smoothly track target position using low-pass damping filter.
    pub fn smooth_track_dsp(&mut self, target: Point3<f32>, alpha: f32) {
        let alpha_clamped = alpha.clamp(0.01, 1.0);
        let cur = self.target;
        let smoothed_x = cur.x + (target.x - cur.x) * alpha_clamped;
        let smoothed_y = cur.y + (target.y - cur.y) * alpha_clamped;
        let smoothed_z = cur.z + (target.z - cur.z) * alpha_clamped;
        self.set_target(Point3::new(smoothed_x, smoothed_y, smoothed_z));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_creation() {
        let camera = Camera::new(16.0 / 9.0);
        assert!((camera.get_aspect_ratio() - 16.0 / 9.0).abs() < 0.001);
        assert_eq!(camera.near, 0.4);
        assert_eq!(camera.far, 20.0);
        assert_eq!(camera.position, Point3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn test_camera_set_position() {
        let mut camera = Camera::new(1.0);
        let new_pos = Point3::new(5.0, 10.0, 15.0);
        camera.set_position(new_pos);
        assert_eq!(camera.position, new_pos);
    }

    #[test]
    fn test_camera_set_target() {
        let mut camera = Camera::new(1.0);
        let target = Point3::new(1.0, 2.0, 3.0);
        camera.set_target(target);
        assert_eq!(camera.target, target);
    }

    #[test]
    fn test_camera_set_fovy() {
        let mut camera = Camera::new(1.0);
        let new_fov = core::f32::consts::PI / 4.0; // 45 degrees
        camera.set_fovy(new_fov);
        assert!((camera.fov - new_fov).abs() < 0.001);
    }

    #[test]
    fn test_camera_get_direction() {
        let mut camera = Camera::new(1.0);
        camera.set_position(Point3::new(0.0, 0.0, 5.0));
        camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let direction = camera.get_direction();
        // Direction should point roughly toward target
        assert!(direction.magnitude() > 0.0);
    }

    #[test]
    fn test_camera_vp_matrix_updates() {
        let mut camera = Camera::new(1.0);
        let initial_vp = camera.vp_matrix;

        // Change position should update VP matrix
        camera.set_position(Point3::new(5.0, 5.0, 5.0));
        assert_ne!(camera.vp_matrix, initial_vp);

        let after_pos = camera.vp_matrix;

        // Change FOV should update VP matrix
        camera.set_fovy(core::f32::consts::PI / 4.0);
        assert_ne!(camera.vp_matrix, after_pos);
    }

    #[test]
    fn test_camera_projection_target_center() {
        let mut camera = Camera::new(1.0); // 1:1 aspect ratio
        camera.set_position(Point3::new(0.0, 0.0, 5.0));
        camera.set_target(Point3::new(0.0, 0.0, 0.0));

        // Target point (0, 0, 0) transformed by VP matrix
        let p_target = nalgebra::Vector4::new(0.0, 0.0, 0.0, 1.0);
        let clip = camera.vp_matrix * p_target;

        // In homogeneous clip space, x and y must be 0.0 (centered on screen)
        assert!(clip.x.abs() < 1e-4);
        assert!(clip.y.abs() < 1e-4);
        assert!(clip.w > 0.0); // Point is in front of camera
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_camera_view_matrix_orthogonality() {
        let mut camera = Camera::new(16.0 / 9.0);
        camera.set_position(Point3::new(3.0, 4.0, 5.0));
        camera.set_target(Point3::new(0.0, 1.0, 0.0));

        // Extract upper-left 3x3 rotation matrix R from view matrix
        let R = camera.view_matrix.fixed_view::<3, 3>(0, 0);
        let I = R * R.transpose();

        // R * R^T must equal Identity for orthogonal rotation matrix
        let identity = nalgebra::Matrix3::identity();
        let diff = (I - identity).norm();
        assert!(
            diff < 1e-4,
            "View matrix rotation is not orthogonal: diff = {}",
            diff
        );
    }
}
