//! Camera controllers inspired by `bevy_camera_controller`.
//!
//! Provides orbit (arcball) and first-person camera controllers designed for
//! microcontrollers, touchscreens, D-pads, and analog thumbsticks.

use crate::camera::Camera;
use core::f32::consts::FRAC_PI_2;
use nalgebra::{Point3, Vector3};

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// Orbit / Arcball camera controller that orbits around a focal point.
///
/// Ideal for model viewers, strategy games, third-person character cameras,
/// and touch-drag / analog-stick rotation on embedded screens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCameraController {
    /// Focal point around which the camera orbits.
    pub target: Point3<f32>,
    /// Distance from the camera to the target.
    pub distance: f32,
    /// Azimuthal angle (horizontal rotation) in radians.
    pub yaw: f32,
    /// Polar angle (vertical rotation) in radians. Clamped to prevent flipping.
    pub pitch: f32,
    /// Minimum allowed distance (zoom limit).
    pub min_distance: f32,
    /// Maximum allowed distance.
    pub max_distance: f32,
    /// Minimum pitch angle in radians (default: ~ -89°).
    pub min_pitch: f32,
    /// Maximum pitch angle in radians (default: ~ +89°).
    pub max_pitch: f32,
}

impl OrbitCameraController {
    /// Create a new orbit camera controller looking at `target` from `distance`.
    pub fn new(target: Point3<f32>, distance: f32) -> Self {
        Self {
            target,
            distance: distance.max(0.1),
            yaw: 0.0,
            pitch: 0.0,
            min_distance: 0.5,
            max_distance: 100.0,
            min_pitch: -FRAC_PI_2 + 0.01,
            max_pitch: FRAC_PI_2 - 0.01,
        }
    }

    /// Builder for customizing zoom distance boundaries.
    pub fn with_distance_limits(mut self, min: f32, max: f32) -> Self {
        self.min_distance = min;
        self.max_distance = max;
        self.distance = self.distance.clamp(min, max);
        self
    }

    /// Builder for customizing pitch clamping limits (in radians).
    pub fn with_pitch_limits(mut self, min: f32, max: f32) -> Self {
        self.min_pitch = min;
        self.max_pitch = max;
        self.pitch = self.pitch.clamp(min, max);
        self
    }

    /// Rotate around the target by delta yaw and pitch (in radians).
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(self.min_pitch, self.max_pitch);
    }

    /// Zoom in (negative delta) or out (positive delta).
    pub fn zoom(&mut self, delta_distance: f32) {
        self.distance =
            (self.distance + delta_distance).clamp(self.min_distance, self.max_distance);
    }

    /// Pan the focal target in camera-local horizontal and vertical directions.
    pub fn pan(&mut self, delta_right: f32, delta_up: f32) {
        let cos_yaw = self.yaw.cos();
        let sin_yaw = self.yaw.sin();
        let right = Vector3::new(cos_yaw, 0.0, -sin_yaw);
        let up = Vector3::new(0.0, 1.0, 0.0);

        let offset = right * delta_right + up * delta_up;
        self.target += offset;
    }

    /// Calculate the camera eye position in world space.
    #[inline]
    pub fn eye_position(&self) -> Point3<f32> {
        let cos_pitch = self.pitch.cos();
        let sin_pitch = self.pitch.sin();
        let cos_yaw = self.yaw.cos();
        let sin_yaw = self.yaw.sin();

        let x = self.target.x + self.distance * cos_pitch * sin_yaw;
        let y = self.target.y + self.distance * sin_pitch;
        let z = self.target.z + self.distance * cos_pitch * cos_yaw;

        Point3::new(x, y, z)
    }

    /// Synchronize state to an engine [`Camera`].
    pub fn update_camera(&self, camera: &mut Camera) {
        camera.set_target(self.target);
        camera.set_position(self.eye_position());
    }
}

/// First-person fly / walk camera controller.
///
/// Translates along view-aligned axes and rotates with yaw and pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FpsCameraController {
    /// World-space position of the camera eye.
    pub position: Point3<f32>,
    /// Horizontal heading angle in radians.
    pub yaw: f32,
    /// Vertical look angle in radians.
    pub pitch: f32,
    /// Movement speed units per second.
    pub move_speed: f32,
    /// Minimum pitch angle in radians (default: ~ -89°).
    pub min_pitch: f32,
    /// Maximum pitch angle in radians (default: ~ +89°).
    pub max_pitch: f32,
}

impl FpsCameraController {
    /// Create a new first-person camera controller at `position`.
    pub fn new(position: Point3<f32>) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            move_speed: 5.0,
            min_pitch: -FRAC_PI_2 + 0.01,
            max_pitch: FRAC_PI_2 - 0.01,
        }
    }

    /// Rotate view by delta yaw and pitch (in radians).
    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(self.min_pitch, self.max_pitch);
    }

    /// Forward direction vector on the horizontal plane (unit length).
    #[inline]
    pub fn horizontal_forward(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    /// Right direction vector on the horizontal plane (unit length).
    #[inline]
    pub fn horizontal_right(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.cos(), 0.0, self.yaw.sin())
    }

    /// True look direction vector including pitch.
    #[inline]
    pub fn look_direction(&self) -> Vector3<f32> {
        let cos_pitch = self.pitch.cos();
        let sin_pitch = self.pitch.sin();
        let cos_yaw = self.yaw.cos();
        let sin_yaw = self.yaw.sin();

        Vector3::new(cos_pitch * sin_yaw, sin_pitch, -cos_pitch * cos_yaw)
    }

    /// Move the camera relative to its current heading.
    ///
    /// `forward`: +1 = forward, -1 = backward
    /// `strafe`: +1 = right, -1 = left
    /// `vertical`: +1 = up, -1 = down
    pub fn move_relative(&mut self, forward: f32, strafe: f32, vertical: f32, dt: f32) {
        let fwd = self.horizontal_forward() * (forward * self.move_speed * dt);
        let right = self.horizontal_right() * (strafe * self.move_speed * dt);
        let up = Vector3::new(0.0, vertical * self.move_speed * dt, 0.0);

        self.position += fwd + right + up;
    }

    /// Synchronize state to an engine [`Camera`].
    pub fn update_camera(&self, camera: &mut Camera) {
        let target = self.position + self.look_direction();
        camera.set_target(target);
        camera.set_position(self.position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orbit_camera_eye_position() {
        let mut orbit = OrbitCameraController::new(Point3::new(0.0, 0.0, 0.0), 5.0);
        let eye = orbit.eye_position();
        assert!((eye.z - 5.0).abs() < 1e-4);
        assert!(eye.x.abs() < 1e-4);
        assert!(eye.y.abs() < 1e-4);

        // Orbit 90 degrees around Y (yaw = PI / 2)
        orbit.orbit(core::f32::consts::FRAC_PI_2, 0.0);
        let eye90 = orbit.eye_position();
        assert!((eye90.x - 5.0).abs() < 1e-4);
        assert!(eye90.z.abs() < 1e-4);
    }

    #[test]
    fn test_orbit_camera_pitch_clamping() {
        let mut orbit = OrbitCameraController::new(Point3::new(0.0, 0.0, 0.0), 5.0);
        orbit.orbit(0.0, 10.0); // Excessive pitch upwards
        assert!(orbit.pitch <= orbit.max_pitch);

        orbit.orbit(0.0, -20.0); // Excessive pitch downwards
        assert!(orbit.pitch >= orbit.min_pitch);
    }

    #[test]
    fn test_fps_camera_movement() {
        let mut fps = FpsCameraController::new(Point3::new(0.0, 0.0, 0.0));
        // Move forward 1 second at speed 5.0
        fps.move_relative(1.0, 0.0, 0.0, 1.0);
        assert!((fps.position.z - (-5.0)).abs() < 1e-4);

        // Strafe right 1 second
        fps.move_relative(0.0, 1.0, 0.0, 1.0);
        assert!((fps.position.x - 5.0).abs() < 1e-4);
    }
}
