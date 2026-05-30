//! Fixed-capacity, no-alloc particle system for embedded environments.
//!
//! Each particle is rendered as a camera-facing billboard quad whose colour
//! and size interpolate linearly between their birth and death values.
//!
//! # Example
//! ```
//! use embedded_3dgfx::particles::{ParticleSpawn, ParticleSystem};
//! use nalgebra::{Point3, Vector3};
//! use embedded_graphics_core::pixelcolor::Rgb565;
//!
//! let mut sys: ParticleSystem<64> = ParticleSystem::new();
//! sys.spawn(ParticleSpawn {
//!     position:     Point3::new(0.0, 0.0, 0.0),
//!     velocity:     Vector3::new(0.0, 2.0, 0.0),
//!     acceleration: Vector3::zeros(),
//!     color_start:  Rgb565::new(31, 40, 0),
//!     color_end:    Rgb565::new(10, 0, 0),
//!     size_start:   0.2,
//!     size_end:     0.0,
//!     lifetime:     1.5,
//! });
//! sys.update(0.016, Vector3::new(0.0, -9.81, 0.0));
//! ```

use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use nalgebra::{Point3, Vector3};

use crate::billboard::Billboard;

/// State for a single live particle.
#[derive(Clone, Copy)]
pub struct Particle {
    /// Current world-space position.
    pub position: Point3<f32>,
    /// Current velocity (world units/second).
    pub velocity: Vector3<f32>,
    /// Constant per-particle acceleration (applied in addition to gravity).
    pub acceleration: Vector3<f32>,
    /// Colour at birth (t = 0).
    pub color_start: Rgb565,
    /// Colour at death (t = 1).
    pub color_end: Rgb565,
    /// Billboard half-size at birth.
    pub size_start: f32,
    /// Billboard half-size at death.  Use `0.0` to shrink out.
    pub size_end: f32,
    /// Elapsed lifetime in seconds.
    pub age: f32,
    /// Total lifetime in seconds.
    pub lifetime: f32,
    pub(crate) active: bool,
}

impl Particle {
    /// Normalised age: `0.0` at birth → `1.0` at death.
    #[inline]
    pub fn t(&self) -> f32 {
        if self.lifetime > 0.0 {
            (self.age / self.lifetime).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// Current interpolated colour.
    #[inline]
    pub fn color(&self) -> Rgb565 {
        let t = self.t();
        let r = lerp_ch(self.color_start.r(), self.color_end.r(), t, 31);
        let g = lerp_ch(self.color_start.g(), self.color_end.g(), t, 63);
        let b = lerp_ch(self.color_start.b(), self.color_end.b(), t, 31);
        Rgb565::new(r, g, b)
    }

    /// Current interpolated size.
    #[inline]
    pub fn size(&self) -> f32 {
        self.size_start + self.t() * (self.size_end - self.size_start)
    }
}

#[inline]
fn lerp_ch(start: u8, end: u8, t: f32, max: u8) -> u8 {
    (start as f32 + t * (end as f32 - start as f32)).clamp(0.0, max as f32) as u8
}

/// Parameters for spawning a single particle.
#[derive(Clone, Copy)]
pub struct ParticleSpawn {
    /// Initial world-space position.
    pub position: Point3<f32>,
    /// Initial velocity (world units/second).
    pub velocity: Vector3<f32>,
    /// Per-particle constant acceleration (added to gravity in `update`).
    pub acceleration: Vector3<f32>,
    /// Colour at birth.
    pub color_start: Rgb565,
    /// Colour at death.
    pub color_end: Rgb565,
    /// Billboard size at birth.
    pub size_start: f32,
    /// Billboard size at death.
    pub size_end: f32,
    /// Lifetime in seconds.
    pub lifetime: f32,
}

/// Fixed-capacity particle system.
///
/// `N` is the maximum number of simultaneously live particles.  Slots are
/// reused in-place so spawning never allocates.
pub struct ParticleSystem<const N: usize> {
    slots: [Particle; N],
    active: usize,
}

impl<const N: usize> ParticleSystem<N> {
    /// Create a new, empty system.
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| Particle {
                position: Point3::new(0.0, 0.0, 0.0),
                velocity: Vector3::new(0.0, 0.0, 0.0),
                acceleration: Vector3::new(0.0, 0.0, 0.0),
                color_start: Rgb565::new(0, 0, 0),
                color_end: Rgb565::new(0, 0, 0),
                size_start: 0.0,
                size_end: 0.0,
                age: 0.0,
                lifetime: 0.0,
                active: false,
            }),
            active: 0,
        }
    }

    /// Spawn a particle.  Returns `true` on success, `false` if the pool is
    /// at capacity (`N` particles already live).
    pub fn spawn(&mut self, s: ParticleSpawn) -> bool {
        for slot in &mut self.slots {
            if !slot.active {
                *slot = Particle {
                    position: s.position,
                    velocity: s.velocity,
                    acceleration: s.acceleration,
                    color_start: s.color_start,
                    color_end: s.color_end,
                    size_start: s.size_start,
                    size_end: s.size_end,
                    age: 0.0,
                    lifetime: s.lifetime,
                    active: true,
                };
                self.active += 1;
                return true;
            }
        }
        false
    }

    /// Advance all particles by `dt` seconds.
    ///
    /// `gravity` is applied uniformly to every particle's velocity.  Pass
    /// `Vector3::zeros()` to disable gravity for this system.
    pub fn update(&mut self, dt: f32, gravity: Vector3<f32>) {
        let mut live = 0usize;
        for slot in &mut self.slots {
            if !slot.active {
                continue;
            }
            slot.velocity += (slot.acceleration + gravity) * dt;
            slot.position += slot.velocity * dt;
            slot.age += dt;
            if slot.age >= slot.lifetime {
                slot.active = false;
            } else {
                live += 1;
            }
        }
        self.active = live;
    }

    /// Number of currently live particles.
    #[inline]
    pub fn active_count(&self) -> usize {
        self.active
    }

    /// `true` if no particles are live.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active == 0
    }

    /// Iterate over live particles (read-only).
    pub fn iter_active(&self) -> impl Iterator<Item = &Particle> {
        self.slots.iter().filter(|p| p.active)
    }

    /// Record all live particles as camera-facing billboard quads into
    /// `commands`.
    ///
    /// Each particle emits two
    /// [`DrawPrimitive::ColoredTriangleWithDepth`][crate::DrawPrimitive::ColoredTriangleWithDepth]
    /// commands.  Billboard orientation uses world-up `(0, 1, 0)`, which is
    /// correct for all but extreme pitch angles.
    ///
    /// Returns the number of particles successfully projected and emitted.
    /// Particles that project behind the camera or outside the frustum are
    /// silently skipped.
    pub fn record<const MAX: usize>(
        &self,
        engine: &crate::K3dengine,
        commands: &mut crate::command_buffer::CommandBuffer<MAX>,
    ) -> usize {
        use crate::command_buffer::RenderCommand;

        if self.active == 0 {
            return 0;
        }

        let camera_up = Vector3::new(0.0, 1.0, 0.0);
        let vp = engine.camera.vp_matrix;
        let mut emitted = 0usize;

        for particle in self.iter_active() {
            let size = particle.size();
            if size <= 0.0 {
                continue;
            }
            let color = particle.color();

            let billboard = Billboard::new(particle.position, size, color);
            let quad = billboard.generate_quad(engine.camera.position, camera_up);

            // Triangle 1: bottom-left (0), bottom-right (1), top-right (2)
            let Some((pts1, _)) =
                engine.transform_points_with_w(&[0usize, 1, 2], &quad, vp)
            else {
                continue;
            };

            // Triangle 2: bottom-left (0), top-right (2), top-left (3)
            let Some((pts2, _)) =
                engine.transform_points_with_w(&[0usize, 2, 3], &quad, vp)
            else {
                continue;
            };

            let _ = commands.push(RenderCommand::Draw(
                crate::DrawPrimitive::ColoredTriangleWithDepth {
                    points: [pts1[0].xy(), pts1[1].xy(), pts1[2].xy()],
                    depths: [pts1[0].z as f32, pts1[1].z as f32, pts1[2].z as f32],
                    color,
                },
            ));
            let _ = commands.push(RenderCommand::Draw(
                crate::DrawPrimitive::ColoredTriangleWithDepth {
                    points: [pts2[0].xy(), pts2[1].xy(), pts2[2].xy()],
                    depths: [pts2[0].z as f32, pts2[1].z as f32, pts2[2].z as f32],
                    color,
                },
            ));
            emitted += 1;
        }

        emitted
    }
}

impl<const N: usize> Default for ParticleSystem<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::WebColors;

    fn default_spawn(lifetime: f32) -> ParticleSpawn {
        ParticleSpawn {
            position: Point3::new(0.0, 0.0, 0.0),
            velocity: Vector3::new(0.0, 1.0, 0.0),
            acceleration: Vector3::zeros(),
            color_start: Rgb565::CSS_RED,
            color_end: Rgb565::CSS_BLUE,
            size_start: 1.0,
            size_end: 0.0,
            lifetime,
        }
    }

    #[test]
    fn test_spawn_increments_count() {
        let mut sys: ParticleSystem<4> = ParticleSystem::new();
        assert_eq!(sys.active_count(), 0);
        assert!(sys.spawn(default_spawn(2.0)));
        assert_eq!(sys.active_count(), 1);
    }

    #[test]
    fn test_pool_full_returns_false() {
        let mut sys: ParticleSystem<2> = ParticleSystem::new();
        assert!(sys.spawn(default_spawn(2.0)));
        assert!(sys.spawn(default_spawn(2.0)));
        assert!(!sys.spawn(default_spawn(2.0)));
    }

    #[test]
    fn test_update_kills_expired_particles() {
        let mut sys: ParticleSystem<4> = ParticleSystem::new();
        sys.spawn(default_spawn(0.1));
        sys.update(0.2, Vector3::zeros());
        assert_eq!(sys.active_count(), 0);
    }

    #[test]
    fn test_update_moves_particle_with_gravity() {
        let mut sys: ParticleSystem<4> = ParticleSystem::new();
        sys.spawn(default_spawn(5.0));
        let y0 = sys.iter_active().next().unwrap().position.y;
        sys.update(0.1, Vector3::new(0.0, -9.81, 0.0));
        let y1 = sys.iter_active().next().unwrap().position.y;
        // Gravity slows the upward velocity but initial velocity dominates briefly
        assert!(y1 != y0);
    }

    #[test]
    fn test_slot_reused_after_expiry() {
        let mut sys: ParticleSystem<1> = ParticleSystem::new();
        assert!(sys.spawn(default_spawn(0.05)));
        sys.update(0.1, Vector3::zeros());
        assert_eq!(sys.active_count(), 0);
        assert!(sys.spawn(default_spawn(2.0)));
        assert_eq!(sys.active_count(), 1);
    }

    #[test]
    fn test_color_at_birth_equals_start() {
        let mut sys: ParticleSystem<4> = ParticleSystem::new();
        sys.spawn(default_spawn(2.0));
        let p = sys.iter_active().next().unwrap();
        assert_eq!(p.t(), 0.0);
        assert_eq!(p.color(), Rgb565::CSS_RED);
    }

    #[test]
    fn test_size_shrinks_over_time() {
        let mut sys: ParticleSystem<4> = ParticleSystem::new();
        sys.spawn(default_spawn(2.0));
        let s0 = sys.iter_active().next().unwrap().size();
        sys.update(1.0, Vector3::zeros());
        let s1 = sys.iter_active().next().unwrap().size();
        assert!(s1 < s0);
    }
}
