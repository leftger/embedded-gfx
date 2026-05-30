//! First-person character controller with axis-aligned box collision.
//!
//! The controller is deliberately decoupled from the physics engine so it
//! works in `no_std` environments without any allocator.  Collision is
//! resolved against a caller-supplied slice of AABB obstacles.
//!
//! # Collision format
//!
//! Each obstacle is `[min_x, min_y, min_z, max_x, max_y, max_z]` in
//! world space.  Include a floor slab so the player lands on it, a ceiling
//! slab so jumping is bounded, and a wall slab for every solid surface.
//!
//! ```no_run
//! use embedded_3dgfx::character::CharacterController;
//! use embedded_3dgfx::input::InputState;
//! use nalgebra::Point3;
//!
//! let mut ch = CharacterController::new(Point3::new(0.0, 0.0, 0.0));
//!
//! let colliders: &[[f32; 6]] = &[
//!     // floor slab  (player stands on Y=0)
//!     [-10.0, -0.5, -10.0,  10.0, 0.0, 10.0],
//!     // north wall
//!     [-10.0,  0.0, -10.5,  10.0, 4.0, -10.0],
//!     // south wall
//!     [-10.0,  0.0,  10.0,  10.0, 4.0,  10.5],
//! ];
//!
//! let input = InputState { forward: 1.0, ..Default::default() };
//! ch.tick(&input, 1.0 / 60.0, colliders);
//! ```

use core::f32::consts::PI;
use nalgebra::{Point3, Vector3};

#[cfg(not(feature = "std"))]
use micromath::F32Ext;

use crate::input::InputState;

/// Number of AABB resolution passes per tick.
///
/// More passes → more stable corner contacts; 3 is enough for typical
/// dungeon geometry.
const RESOLVE_ITERS: usize = 3;

/// First-person character controller.
///
/// `position` is the **foot point** — the lowest Y the player occupies.
/// Eyes sit at `position.y + height * eye_fraction`.
#[derive(Clone, Debug)]
pub struct CharacterController {
    /// Foot position (Y = the floor the player stands on).
    pub position: Point3<f32>,

    /// Current velocity in m/s.
    ///
    /// Horizontal components are overwritten by input every tick.
    /// The vertical component accumulates gravity between frames.
    pub velocity: Vector3<f32>,

    /// Horizontal look direction, radians.  0 = facing −Z (into the screen).
    /// Increases clockwise when viewed from above.
    pub yaw: f32,

    /// Vertical look angle, radians.  Clamped to ±~84°.
    pub pitch: f32,

    /// Standing height in metres.
    pub height: f32,

    /// Horizontal half-width of the player AABB (metres).
    pub radius: f32,

    /// Fraction of `height` at which the eye / camera sits (default 0.86).
    pub eye_fraction: f32,

    /// `true` when the character is resting on a solid surface.
    pub on_ground: bool,

    /// Walk speed (m/s, used when `InputState::sprint` is false).
    pub walk_speed: f32,

    /// Run / sprint speed (m/s).
    pub run_speed: f32,

    /// Upward velocity applied on jump (m/s).
    pub jump_speed: f32,

    /// Gravitational acceleration (m/s², positive = downward).
    pub gravity: f32,
}

impl CharacterController {
    /// Create a controller with sensible defaults.
    ///
    /// The player spawns at `pos` with feet touching the floor (Y=pos.y).
    /// Call [`tick`](Self::tick) once per frame to advance the simulation.
    pub fn new(pos: Point3<f32>) -> Self {
        Self {
            position: pos,
            velocity: Vector3::zeros(),
            yaw: 0.0,
            pitch: 0.0,
            height: 1.75,
            radius: 0.28,
            eye_fraction: 0.86,
            on_ground: false,
            walk_speed: 4.5,
            run_speed: 8.5,
            jump_speed: 5.5,
            gravity: 14.0,
        }
    }

    // ── Direction helpers ─────────────────────────────────────────────────────

    /// Unit vector pointing forward in the horizontal plane (ignores pitch).
    pub fn forward_flat(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    /// Unit vector pointing right in the horizontal plane.
    pub fn right_flat(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.cos(), 0.0, self.yaw.sin())
    }

    /// Full 3-D look direction (yaw + pitch).
    pub fn look_dir(&self) -> Vector3<f32> {
        let cp = self.pitch.cos();
        Vector3::new(cp * self.yaw.sin(), self.pitch.sin(), -cp * self.yaw.cos())
    }

    /// Camera / eye position in world space.
    pub fn eye_position(&self) -> Point3<f32> {
        Point3::new(
            self.position.x,
            self.position.y + self.height * self.eye_fraction,
            self.position.z,
        )
    }

    /// Point the camera should target (`eye_position + look_dir`).
    pub fn look_target(&self) -> Point3<f32> {
        self.eye_position() + self.look_dir()
    }

    /// Write the eye position and look target into the engine camera.
    pub fn apply_to_camera(&self, engine: &mut crate::K3dengine) {
        engine.camera.set_position(self.eye_position());
        engine.camera.set_target(self.look_target());
    }

    // ── Simulation ────────────────────────────────────────────────────────────

    /// Advance one frame of simulation.
    ///
    /// # Arguments
    /// * `input`     — this frame's input (direction, look deltas, jump, sprint)
    /// * `dt`        — elapsed time in seconds (clamp to ≤ 0.05 to avoid
    ///                 tunnelling on lag spikes)
    /// * `colliders` — world geometry as `[min_x, min_y, min_z, max_x, max_y,
    ///                 max_z]` boxes.  Include floor, ceiling, and all walls.
    pub fn tick(&mut self, input: &InputState, dt: f32, colliders: &[[f32; 6]]) {
        let dt = dt.min(0.05);

        // ── Orientation ───────────────────────────────────────────────────────
        self.yaw += input.look_yaw;
        if self.yaw > PI {
            self.yaw -= 2.0 * PI;
        }
        if self.yaw < -PI {
            self.yaw += 2.0 * PI;
        }
        // ±84° pitch limit (avoids gimbal singularities)
        self.pitch = (self.pitch + input.look_pitch).clamp(-1.466, 1.466);

        // ── Horizontal velocity from input ────────────────────────────────────
        let speed = if input.sprint {
            self.run_speed
        } else {
            self.walk_speed
        };
        let fwd = self.forward_flat();
        let rgt = self.right_flat();
        self.velocity.x = (fwd.x * input.forward + rgt.x * input.strafe) * speed;
        self.velocity.z = (fwd.z * input.forward + rgt.z * input.strafe) * speed;

        // ── Vertical velocity (gravity / jump) ────────────────────────────────
        if self.on_ground {
            if input.jump {
                self.velocity.y = self.jump_speed;
                self.on_ground = false;
            } else {
                self.velocity.y = 0.0;
            }
        } else {
            self.velocity.y -= self.gravity * dt;
        }

        // ── Integrate ─────────────────────────────────────────────────────────
        let mut pos = self.position + self.velocity * dt;
        self.on_ground = false;

        // ── AABB collision resolution ─────────────────────────────────────────
        // Multiple passes let corner contacts settle properly.
        for _ in 0..RESOLVE_ITERS {
            for &[cx0, cy0, cz0, cx1, cy1, cz1] in colliders {
                // Player AABB (foot-to-head, square in XZ)
                let px0 = pos.x - self.radius;
                let px1 = pos.x + self.radius;
                let py0 = pos.y;
                let py1 = pos.y + self.height;
                let pz0 = pos.z - self.radius;
                let pz1 = pos.z + self.radius;

                // Broad-phase
                if px1 <= cx0 || px0 >= cx1 || py1 <= cy0 || py0 >= cy1 || pz1 <= cz0 || pz0 >= cz1
                {
                    continue;
                }

                // Penetration depths along each axis
                let ox_a = px1 - cx0; // push player left  (−X)
                let ox_b = cx1 - px0; // push player right (+X)
                let oy_a = py1 - cy0; // push player down  (−Y, ceiling)
                let oy_b = cy1 - py0; // push player up    (+Y, floor)
                let oz_a = pz1 - cz0; // push player −Z
                let oz_b = cz1 - pz0; // push player +Z

                let ox = ox_a.min(ox_b);
                let oy = oy_a.min(oy_b);
                let oz = oz_a.min(oz_b);

                if oy <= ox && oy <= oz {
                    // Vertical contact
                    if oy_b < oy_a {
                        // Floor: push player up
                        pos.y += oy_b;
                        self.on_ground = true;
                        if self.velocity.y < 0.0 {
                            self.velocity.y = 0.0;
                        }
                    } else {
                        // Ceiling: push player down
                        pos.y -= oy_a;
                        if self.velocity.y > 0.0 {
                            self.velocity.y = 0.0;
                        }
                    }
                } else if ox <= oz {
                    // X-axis wall contact
                    if ox_a < ox_b {
                        pos.x -= ox_a;
                    } else {
                        pos.x += ox_b;
                    }
                    self.velocity.x = 0.0;
                } else {
                    // Z-axis wall contact
                    if oz_a < oz_b {
                        pos.z -= oz_a;
                    } else {
                        pos.z += oz_b;
                    }
                    self.velocity.z = 0.0;
                }
            }
        }

        self.position = pos;
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn no_colliders() -> &'static [[f32; 6]] {
        &[]
    }

    #[test]
    fn forward_flat_faces_neg_z_at_yaw_zero() {
        let ch = CharacterController::new(Point3::new(0.0, 0.0, 0.0));
        let fwd = ch.forward_flat();
        assert!((fwd.x).abs() < 1e-5);
        assert!((fwd.z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn gravity_applies_when_airborne() {
        let mut ch = CharacterController::new(Point3::new(0.0, 5.0, 0.0));
        ch.on_ground = false;
        let input = InputState::default();
        ch.tick(&input, 1.0, no_colliders());
        assert!(ch.position.y < 5.0, "should have fallen");
    }

    #[test]
    fn floor_collider_stops_fall() {
        let mut ch = CharacterController::new(Point3::new(0.0, 1.0, 0.0));
        ch.on_ground = false;
        ch.velocity.y = -10.0;
        let floor = [[-5.0_f32, -0.5, -5.0, 5.0, 0.0, 5.0]];
        let input = InputState::default();
        // dt is clamped to 0.05 s internally; run enough steps to reach the floor
        for _ in 0..8 {
            ch.tick(&input, 1.0 / 60.0, &floor);
        }
        assert!(ch.on_ground, "should be on ground after collision");
        assert!(ch.position.y.abs() < 1e-2, "foot should be at y≈0");
    }

    #[test]
    fn wall_collider_stops_x_movement() {
        let mut ch = CharacterController::new(Point3::new(0.0, 0.0, 0.0));
        ch.on_ground = true;
        // Wall to the right at X=2
        let wall = [[1.9_f32, -1.0, -5.0, 10.0, 5.0, 5.0]];
        let input = InputState {
            strafe: 1.0,
            ..Default::default()
        };
        for _ in 0..30 {
            ch.tick(&input, 1.0 / 60.0, &wall);
        }
        // Should not have passed through the wall
        assert!(ch.position.x + ch.radius <= 1.9 + 0.05);
    }

    #[test]
    fn jump_increases_y_velocity() {
        let mut ch = CharacterController::new(Point3::new(0.0, 0.0, 0.0));
        ch.on_ground = true;
        let input = InputState {
            jump: true,
            ..Default::default()
        };
        ch.tick(&input, 1.0 / 60.0, no_colliders());
        assert!(
            ch.velocity.y > 0.0,
            "should have positive Y velocity after jump"
        );
    }

    #[test]
    fn pitch_is_clamped() {
        let mut ch = CharacterController::new(Point3::new(0.0, 0.0, 0.0));
        let input = InputState {
            look_pitch: 999.0,
            ..Default::default()
        };
        ch.tick(&input, 1.0 / 60.0, no_colliders());
        assert!(ch.pitch <= 1.47);
    }
}
