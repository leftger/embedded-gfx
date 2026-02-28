//! Soft body physics using mass-spring systems and position-based dynamics.
//!
//! Provides deformable objects like cloth, jelly, and soft bodies with:
//! - Particle systems with constraints
//! - Spring-damper networks
//! - Collision detection and response
//! - Pressure/volume preservation
//! - Integration with rigid body physics
//!
//! # Example
//! ```
//! use embedded_3dgfx::softbody::{SoftBody, Particle};
//! use nalgebra::Vector3;
//!
//! let mut soft_body = SoftBody::<64, 128>::new();
//!
//! // Create a simple 2x2 cloth grid
//! for y in 0..2 {
//!     for x in 0..2 {
//!         let pos = Vector3::new(x as f32, y as f32, 0.0);
//!         soft_body.add_particle(Particle::new(pos, 1.0)).unwrap();
//!     }
//! }
//!
//! // Add springs between particles
//! soft_body.add_spring(0, 1, 1.0, 100.0, 0.5).unwrap(); // Horizontal spring
//! soft_body.add_spring(0, 2, 1.0, 100.0, 0.5).unwrap(); // Vertical spring
//!
//! // Pin top corners
//! soft_body.get_particle_mut(0).unwrap().pinned = true;
//! soft_body.get_particle_mut(1).unwrap().pinned = true;
//!
//! // Simulate
//! soft_body.step(0.016);
//! ```

use heapless::Vec;
use nalgebra::Vector3;

#[allow(unused_imports)]
use nalgebra::ComplexField;

/// A particle in a soft body system.
///
/// Particles have mass, position, velocity, and can be pinned in place.
#[derive(Debug, Clone)]
pub struct Particle {
    /// Current position
    pub position: Vector3<f32>,

    /// Previous position (for Verlet integration)
    pub previous_position: Vector3<f32>,

    /// Current velocity
    pub velocity: Vector3<f32>,

    /// Mass of the particle
    pub mass: f32,

    /// Inverse mass (0.0 for infinite mass/pinned particles)
    pub inv_mass: f32,

    /// If true, particle won't move (infinite mass)
    pub pinned: bool,

    /// Accumulated forces for this timestep
    pub force: Vector3<f32>,

    /// Radius for collision detection
    pub radius: f32,
}

impl Particle {
    /// Create a new particle at the given position with the given mass.
    pub fn new(position: Vector3<f32>, mass: f32) -> Self {
        Self {
            position,
            previous_position: position,
            velocity: Vector3::zeros(),
            mass,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            pinned: false,
            force: Vector3::zeros(),
            radius: 0.1,
        }
    }

    /// Create a pinned particle (infinite mass, won't move).
    pub fn new_pinned(position: Vector3<f32>) -> Self {
        Self {
            position,
            previous_position: position,
            velocity: Vector3::zeros(),
            mass: 0.0,
            inv_mass: 0.0,
            pinned: true,
            force: Vector3::zeros(),
            radius: 0.1,
        }
    }

    /// Apply a force to this particle.
    pub fn apply_force(&mut self, force: Vector3<f32>) {
        if !self.pinned {
            self.force += force;
        }
    }

    /// Set the particle's radius for collision detection.
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// A spring constraint between two particles.
///
/// Springs create restoring forces based on distance deviation from rest length.
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    /// Index of first particle
    pub particle_a: usize,

    /// Index of second particle
    pub particle_b: usize,

    /// Rest length of the spring
    pub rest_length: f32,

    /// Spring stiffness (higher = stiffer)
    pub stiffness: f32,

    /// Damping coefficient (higher = more damping)
    pub damping: f32,

    /// Whether this spring is enabled
    pub enabled: bool,
}

impl Spring {
    /// Create a new spring between two particles.
    ///
    /// # Arguments
    /// * `particle_a` - Index of first particle
    /// * `particle_b` - Index of second particle
    /// * `rest_length` - Natural length of the spring
    /// * `stiffness` - Spring constant (force per unit extension)
    /// * `damping` - Damping coefficient (velocity damping)
    pub fn new(
        particle_a: usize,
        particle_b: usize,
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    ) -> Self {
        Self {
            particle_a,
            particle_b,
            rest_length,
            stiffness,
            damping,
            enabled: true,
        }
    }

    /// Compute spring force on particle A (force on B is negated).
    pub fn compute_force(
        &self,
        pos_a: Vector3<f32>,
        vel_a: Vector3<f32>,
        pos_b: Vector3<f32>,
        vel_b: Vector3<f32>,
    ) -> Vector3<f32> {
        if !self.enabled {
            return Vector3::zeros();
        }

        let delta = pos_b - pos_a;
        let distance = delta.norm();

        if distance < 0.0001 {
            return Vector3::zeros();
        }

        let direction = delta / distance;

        // Spring force: F = -k * (x - x0)
        let extension = distance - self.rest_length;
        let spring_force = direction * (self.stiffness * extension);

        // Damping force: F = -c * v_rel
        let relative_velocity = vel_b - vel_a;
        let damping_force = direction * (self.damping * relative_velocity.dot(&direction));

        spring_force + damping_force
    }
}

/// Configuration for soft body pressure/volume preservation.
///
/// Helps maintain the volume of enclosed soft bodies (like balloons or jelly).
#[derive(Debug, Clone, Copy)]
pub struct PressureConfig {
    /// Target volume to maintain
    pub target_volume: f32,

    /// Pressure coefficient (higher = stronger volume preservation)
    pub pressure_coefficient: f32,

    /// Whether pressure is enabled
    pub enabled: bool,
}

impl PressureConfig {
    /// Create a new pressure configuration.
    pub fn new(target_volume: f32, pressure_coefficient: f32) -> Self {
        Self {
            target_volume,
            pressure_coefficient,
            enabled: true,
        }
    }
}

impl Default for PressureConfig {
    fn default() -> Self {
        Self {
            target_volume: 1.0,
            pressure_coefficient: 1.0,
            enabled: false,
        }
    }
}

/// A soft body made of particles connected by springs.
///
/// Generic parameters:
/// * `P` - Maximum number of particles
/// * `S` - Maximum number of springs
#[derive(Debug, Clone)]
pub struct SoftBody<const P: usize, const S: usize> {
    /// Particles in the soft body
    pub particles: Vec<Particle, P>,

    /// Spring constraints between particles
    pub springs: Vec<Spring, S>,

    /// Gravity acceleration applied to all particles
    pub gravity: Vector3<f32>,

    /// Global damping factor (0.0 = no damping, 1.0 = full damping)
    pub damping: f32,

    /// Pressure/volume preservation configuration
    pub pressure_config: PressureConfig,

    /// Ground plane height (particles bounce off this)
    pub ground_plane: Option<f32>,

    /// Ground restitution (bounciness of ground collisions)
    pub ground_restitution: f32,

    /// Ground friction coefficient
    pub ground_friction: f32,
}

impl<const P: usize, const S: usize> SoftBody<P, S> {
    /// Create a new empty soft body.
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            springs: Vec::new(),
            gravity: Vector3::new(0.0, -9.81, 0.0),
            damping: 0.99,
            pressure_config: PressureConfig::default(),
            ground_plane: Some(0.0),
            ground_restitution: 0.3,
            ground_friction: 0.5,
        }
    }

    /// Add a particle to the soft body.
    pub fn add_particle(&mut self, particle: Particle) -> Result<usize, ()> {
        let id = self.particles.len();
        self.particles.push(particle).map_err(|_| ())?;
        Ok(id)
    }

    /// Add a spring constraint between two particles.
    pub fn add_spring(
        &mut self,
        particle_a: usize,
        particle_b: usize,
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    ) -> Result<(), ()> {
        let spring = Spring::new(particle_a, particle_b, rest_length, stiffness, damping);
        self.springs.push(spring).map_err(|_| ())
    }

    /// Get a particle by index.
    pub fn get_particle(&self, index: usize) -> Option<&Particle> {
        self.particles.get(index)
    }

    /// Get a mutable reference to a particle by index.
    pub fn get_particle_mut(&mut self, index: usize) -> Option<&mut Particle> {
        self.particles.get_mut(index)
    }

    /// Set the gravity vector for all particles.
    pub fn set_gravity(&mut self, gravity: Vector3<f32>) {
        self.gravity = gravity;
    }

    /// Advance the soft body simulation by one timestep.
    ///
    /// Uses semi-implicit Euler integration with spring forces.
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        // Apply gravity to all particles
        for particle in self.particles.iter_mut() {
            if !particle.pinned {
                particle.apply_force(self.gravity * particle.mass);
            }
        }

        // Compute and apply spring forces
        for i in 0..self.springs.len() {
            let spring = &self.springs[i];
            if !spring.enabled {
                continue;
            }

            let (idx_a, idx_b) = (spring.particle_a, spring.particle_b);
            if idx_a >= self.particles.len() || idx_b >= self.particles.len() {
                continue;
            }

            // Get particle data (must do this carefully to avoid borrow checker issues)
            let (pos_a, vel_a, pos_b, vel_b) = {
                let pa = &self.particles[idx_a];
                let pb = &self.particles[idx_b];
                (pa.position, pa.velocity, pb.position, pb.velocity)
            };

            let force = spring.compute_force(pos_a, vel_a, pos_b, vel_b);

            // Apply forces
            self.particles[idx_a].apply_force(force);
            self.particles[idx_b].apply_force(-force);
        }

        // Apply pressure forces if enabled
        if self.pressure_config.enabled {
            self.apply_pressure_forces();
        }

        // Integrate particle motion
        for particle in self.particles.iter_mut() {
            if particle.pinned {
                particle.force = Vector3::zeros();
                continue;
            }

            // Semi-implicit Euler: v' = v + a*dt, x' = x + v'*dt
            let acceleration = particle.force * particle.inv_mass;
            particle.velocity += acceleration * dt;

            // Apply damping
            particle.velocity *= self.damping;

            // Update position
            particle.previous_position = particle.position;
            particle.position += particle.velocity * dt;

            // Clear forces
            particle.force = Vector3::zeros();
        }

        // Apply constraints (ground collision, etc.)
        self.apply_constraints();
    }

    /// Apply ground plane collisions.
    fn apply_constraints(&mut self) {
        if let Some(ground_y) = self.ground_plane {
            for particle in self.particles.iter_mut() {
                if particle.pinned {
                    continue;
                }

                // Check if particle is below ground
                if particle.position.y - particle.radius < ground_y {
                    // Position correction
                    particle.position.y = ground_y + particle.radius;

                    // Velocity response with restitution
                    if particle.velocity.y < 0.0 {
                        particle.velocity.y *= -self.ground_restitution;
                    }

                    // Apply friction to horizontal velocity
                    let horizontal_vel =
                        Vector3::new(particle.velocity.x, 0.0, particle.velocity.z);
                    let friction_impulse = horizontal_vel * -self.ground_friction;
                    particle.velocity.x += friction_impulse.x;
                    particle.velocity.z += friction_impulse.z;
                }
            }
        }
    }

    /// Apply pressure forces to maintain volume (for enclosed soft bodies).
    fn apply_pressure_forces(&mut self) {
        // Simple pressure model: push particles away from center of mass
        if self.particles.is_empty() {
            return;
        }

        // Compute center of mass
        let mut center = Vector3::zeros();
        let mut total_mass = 0.0;

        for particle in self.particles.iter() {
            center += particle.position * particle.mass;
            total_mass += particle.mass;
        }

        if total_mass > 0.0 {
            center /= total_mass;
        }

        // Apply radial pressure forces
        for particle in self.particles.iter_mut() {
            if particle.pinned {
                continue;
            }

            let to_particle = particle.position - center;
            let distance = to_particle.norm();

            if distance > 0.001 {
                let direction = to_particle / distance;
                let pressure_force = direction * self.pressure_config.pressure_coefficient;
                particle.apply_force(pressure_force);
            }
        }
    }

    /// Get vertex positions for rendering.
    ///
    /// Converts particle positions to vertex array format.
    pub fn get_vertex_positions(&self, output: &mut [[f32; 3]]) -> usize {
        let count = self.particles.len().min(output.len());

        for i in 0..count {
            let pos = self.particles[i].position;
            output[i] = [pos.x, pos.y, pos.z];
        }

        count
    }

    /// Reset all forces on particles.
    pub fn clear_forces(&mut self) {
        for particle in self.particles.iter_mut() {
            particle.force = Vector3::zeros();
        }
    }

    /// Apply an external force to all particles.
    pub fn apply_global_force(&mut self, force: Vector3<f32>) {
        for particle in self.particles.iter_mut() {
            particle.apply_force(force);
        }
    }
}

impl<const P: usize, const S: usize> Default for SoftBody<P, S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for creating common soft body shapes.
impl<const P: usize, const S: usize> SoftBody<P, S> {
    /// Create a cloth grid suspended from the top edge.
    ///
    /// # Arguments
    /// * `width` - Number of particles along X axis
    /// * `height` - Number of particles along Y axis
    /// * `spacing` - Distance between particles
    /// * `stiffness` - Spring stiffness
    /// * `damping` - Spring damping
    ///
    /// Returns the soft body with structural and shear springs, or an error if capacity exceeded.
    pub fn create_cloth(
        width: usize,
        height: usize,
        spacing: f32,
        stiffness: f32,
        damping: f32,
    ) -> Result<Self, ()> {
        let mut soft_body = Self::new();

        // Create particle grid
        for y in 0..height {
            for x in 0..width {
                let position = Vector3::new(x as f32 * spacing, -(y as f32 * spacing), 0.0);
                let particle = Particle::new(position, 1.0);
                soft_body.add_particle(particle)?;
            }
        }

        // Pin top row
        for x in 0..width {
            let idx = x;
            if let Some(p) = soft_body.get_particle_mut(idx) {
                p.pinned = true;
            }
        }

        // Add structural springs (horizontal and vertical)
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;

                // Horizontal spring
                if x < width - 1 {
                    soft_body.add_spring(idx, idx + 1, spacing, stiffness, damping)?;
                }

                // Vertical spring
                if y < height - 1 {
                    soft_body.add_spring(idx, idx + width, spacing, stiffness, damping)?;
                }
            }
        }

        // Add shear springs (diagonals for stability)
        for y in 0..height - 1 {
            for x in 0..width - 1 {
                let idx = y * width + x;
                let diagonal_length = spacing * 1.414; // sqrt(2)

                // Diagonal springs
                soft_body.add_spring(
                    idx,
                    idx + width + 1,
                    diagonal_length,
                    stiffness * 0.5,
                    damping,
                )?;
                soft_body.add_spring(
                    idx + 1,
                    idx + width,
                    diagonal_length,
                    stiffness * 0.5,
                    damping,
                )?;
            }
        }

        Ok(soft_body)
    }

    /// Create a soft cube/jelly block.
    ///
    /// # Arguments
    /// * `size` - Number of particles along each axis
    /// * `spacing` - Distance between particles
    /// * `stiffness` - Spring stiffness
    /// * `damping` - Spring damping
    pub fn create_jelly_cube(
        size: usize,
        spacing: f32,
        stiffness: f32,
        damping: f32,
    ) -> Result<Self, ()> {
        let mut soft_body = Self::new();

        // Create 3D grid of particles
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let position = Vector3::new(
                        x as f32 * spacing - (size as f32 * spacing) / 2.0,
                        y as f32 * spacing,
                        z as f32 * spacing - (size as f32 * spacing) / 2.0,
                    );
                    soft_body.add_particle(Particle::new(position, 1.0))?;
                }
            }
        }

        // Add springs between neighboring particles
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let idx = z * size * size + y * size + x;

                    // X-axis springs
                    if x < size - 1 {
                        soft_body.add_spring(idx, idx + 1, spacing, stiffness, damping)?;
                    }

                    // Y-axis springs
                    if y < size - 1 {
                        soft_body.add_spring(idx, idx + size, spacing, stiffness, damping)?;
                    }

                    // Z-axis springs
                    if z < size - 1 {
                        soft_body.add_spring(
                            idx,
                            idx + size * size,
                            spacing,
                            stiffness,
                            damping,
                        )?;
                    }
                }
            }
        }

        // Enable pressure to maintain volume
        soft_body.pressure_config =
            PressureConfig::new((size as f32 * spacing).powi(3), stiffness * 0.1);

        Ok(soft_body)
    }

    /// Create a soft sphere/ball.
    ///
    /// Creates a geodesic sphere approximation with springs.
    pub fn create_soft_sphere(
        radius: f32,
        _subdivisions: usize,
        stiffness: f32,
        damping: f32,
    ) -> Result<Self, ()> {
        let mut soft_body = Self::new();

        // Create icosphere vertices
        let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

        // Initial 12 vertices of icosahedron
        let initial_verts = [
            Vector3::new(-1.0, t, 0.0).normalize() * radius,
            Vector3::new(1.0, t, 0.0).normalize() * radius,
            Vector3::new(-1.0, -t, 0.0).normalize() * radius,
            Vector3::new(1.0, -t, 0.0).normalize() * radius,
            Vector3::new(0.0, -1.0, t).normalize() * radius,
            Vector3::new(0.0, 1.0, t).normalize() * radius,
            Vector3::new(0.0, -1.0, -t).normalize() * radius,
            Vector3::new(0.0, 1.0, -t).normalize() * radius,
            Vector3::new(t, 0.0, -1.0).normalize() * radius,
            Vector3::new(t, 0.0, 1.0).normalize() * radius,
            Vector3::new(-t, 0.0, -1.0).normalize() * radius,
            Vector3::new(-t, 0.0, 1.0).normalize() * radius,
        ];

        for vert in initial_verts.iter() {
            soft_body.add_particle(Particle::new(*vert, 1.0).with_radius(0.05))?;
        }

        // Add springs between connected vertices (simplified - full icosphere would need proper subdivision)
        // For now, just connect nearby particles
        for i in 0..soft_body.particles.len() {
            for j in i + 1..soft_body.particles.len() {
                let dist =
                    (soft_body.particles[i].position - soft_body.particles[j].position).norm();
                if dist < radius * 1.5 {
                    soft_body.add_spring(i, j, dist, stiffness, damping)?;
                }
            }
        }

        // Enable pressure to maintain spherical shape
        soft_body.pressure_config = PressureConfig::new(
            4.0 / 3.0 * core::f32::consts::PI * radius.powi(3),
            stiffness * 0.5,
        );

        Ok(soft_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_creation() {
        let particle = Particle::new(Vector3::new(1.0, 2.0, 3.0), 5.0);
        assert_eq!(particle.position, Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(particle.mass, 5.0);
        assert!((particle.inv_mass - 0.2).abs() < 0.001);
        assert!(!particle.pinned);
    }

    #[test]
    fn test_particle_pinned() {
        let particle = Particle::new_pinned(Vector3::zeros());
        assert!(particle.pinned);
        assert_eq!(particle.inv_mass, 0.0);
    }

    #[test]
    fn test_spring_creation() {
        let spring = Spring::new(0, 1, 1.0, 100.0, 0.5);
        assert_eq!(spring.particle_a, 0);
        assert_eq!(spring.particle_b, 1);
        assert_eq!(spring.rest_length, 1.0);
        assert_eq!(spring.stiffness, 100.0);
        assert!(spring.enabled);
    }

    #[test]
    fn test_softbody_creation() {
        let soft_body = SoftBody::<16, 32>::new();
        assert_eq!(soft_body.particles.len(), 0);
        assert_eq!(soft_body.springs.len(), 0);
        assert_eq!(soft_body.gravity, Vector3::new(0.0, -9.81, 0.0));
    }

    #[test]
    fn test_softbody_add_particle() {
        let mut soft_body = SoftBody::<16, 32>::new();
        let particle = Particle::new(Vector3::new(1.0, 2.0, 3.0), 1.0);

        let result = soft_body.add_particle(particle);
        assert!(result.is_ok());
        assert_eq!(soft_body.particles.len(), 1);
    }

    #[test]
    fn test_softbody_add_spring() {
        let mut soft_body = SoftBody::<16, 32>::new();

        soft_body
            .add_particle(Particle::new(Vector3::zeros(), 1.0))
            .unwrap();
        soft_body
            .add_particle(Particle::new(Vector3::new(1.0, 0.0, 0.0), 1.0))
            .unwrap();

        let result = soft_body.add_spring(0, 1, 1.0, 100.0, 0.5);
        assert!(result.is_ok());
        assert_eq!(soft_body.springs.len(), 1);
    }

    #[test]
    fn test_cloth_creation() {
        let cloth = SoftBody::<64, 128>::create_cloth(4, 4, 0.5, 100.0, 0.5);
        assert!(cloth.is_ok());

        let cloth = cloth.unwrap();
        assert_eq!(cloth.particles.len(), 16); // 4x4 grid
        assert!(cloth.springs.len() > 0); // Should have multiple springs
    }
}
