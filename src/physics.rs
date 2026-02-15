//! Physics engine for embedded 3D graphics.
//!
//! Provides rigid body dynamics with linear and angular motion, gravity,
//! collision detection, impulse-based response with Coulomb friction,
//! body lifecycle management, and constraint joints.
//!
//! Designed for `no_std` environments using fixed-capacity `heapless` collections.
//!
//! # Collider shapes
//! Each body can optionally have a [`Collider`] attached. Supported shapes:
//! - [`Collider::Sphere`] — radius-based, cheapest intersection test
//! - [`Collider::Aabb`] — axis-aligned bounding box, defined by half-extents
//!
//! # Example
//! ```
//! use embedded_3dgfx::physics::{PhysicsWorld, RigidBody, Collider};
//! use nalgebra::Vector3;
//!
//! let mut world = PhysicsWorld::<16>::new();
//! world.set_gravity(Vector3::new(0.0, -9.81, 0.0));
//!
//! let ball = RigidBody::new(1.0)
//!     .with_position(Vector3::new(0.0, 10.0, 0.0))
//!     .with_collider(Collider::Sphere { radius: 0.5 });
//! let id = world.add_body(ball).unwrap();
//!
//! let floor = RigidBody::new_static()
//!     .with_collider(Collider::Aabb { half_extents: Vector3::new(10.0, 0.1, 10.0) });
//! world.add_body(floor).unwrap();
//!
//! // Advance simulation — collision detection + response happen automatically
//! // The const generic `8` sets the max number of contacts per step.
//! world.step::<8>(0.016);
//! ```

use nalgebra::{Matrix3, UnitQuaternion, Vector3};

// ComplexField provides sqrt() for f32 in no_std via libm
#[allow(unused_imports)]
use nalgebra::ComplexField;

/// Unique identifier for a rigid body within a [`PhysicsWorld`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyId(usize);

/// Determines how a body participates in the simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BodyType {
    /// Fully simulated: affected by forces, gravity, and velocity.
    Dynamic,
    /// Not affected by forces or gravity. Useful for floors, walls, and platforms.
    /// Can still be repositioned manually.
    Static,
}

/// A collision shape attached to a [`RigidBody`].
///
/// The collider is centered on the body's position. For AABBs the half-extents
/// define the box dimensions along each axis from the center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Collider {
    /// A sphere defined by its radius.
    Sphere { radius: f32 },
    /// An axis-aligned bounding box defined by half-extents along each axis.
    Aabb { half_extents: Vector3<f32> },
}

/// A detected contact between two bodies.
///
/// Contains the information needed for collision response.
#[derive(Debug, Clone)]
pub struct Contact {
    /// ID of the first body.
    pub body_a: BodyId,
    /// ID of the second body.
    pub body_b: BodyId,
    /// Contact normal pointing from body A toward body B.
    pub normal: Vector3<f32>,
    /// Penetration depth (positive when overlapping).
    pub penetration: f32,
}

/// A rigid body with linear and angular dynamics.
///
/// Tracks position, velocity, orientation, angular velocity, accumulated
/// forces/torques, and mass/inertia.
#[derive(Debug, Clone)]
pub struct RigidBody {
    // -- Linear state --
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub mass: f32,
    pub inv_mass: f32,
    pub body_type: BodyType,
    pub restitution: f32,
    pub collider: Option<Collider>,

    /// Coulomb friction coefficient (0.0 = frictionless ice, 1.0 = very rough).
    /// During collision response the effective friction is the geometric mean
    /// of both bodies' coefficients: `sqrt(mu_a * mu_b)`.
    pub friction: f32,

    /// Accumulated forces applied this frame. Cleared after each `step()`.
    force_accumulator: Vector3<f32>,

    /// Linear damping factor (0.0 = no damping, 1.0 = full stop).
    /// Applied each step as `velocity *= (1.0 - damping)`.
    pub damping: f32,

    // -- Angular state --
    /// Orientation quaternion. Defaults to identity (no rotation).
    pub orientation: UnitQuaternion<f32>,

    /// Angular velocity in world-space (radians per second).
    pub angular_velocity: Vector3<f32>,

    /// Inverse of the inertia tensor in body-local space.
    /// For static bodies this is the zero matrix.
    pub inv_inertia_local: Matrix3<f32>,

    /// Accumulated torques applied this frame. Cleared after each `step()`.
    torque_accumulator: Vector3<f32>,

    /// Angular damping factor (0.0 = no damping, 1.0 = full stop).
    /// Applied each step as `angular_velocity *= (1.0 - angular_damping)`.
    pub angular_damping: f32,

    /// Whether this body is active. Inactive bodies are skipped during
    /// integration and collision detection. Set to `false` by [`PhysicsWorld::remove_body`].
    pub active: bool,
}

impl RigidBody {
    /// Create a new dynamic rigid body with the given mass (in kg).
    ///
    /// The body starts with a default point-mass inertia tensor (identity scaled
    /// by mass). Use [`with_inertia_sphere`] or [`with_inertia_box`] after
    /// attaching a collider for physically accurate rotation.
    ///
    /// # Panics
    /// Panics if `mass` is not positive and finite.
    pub fn new(mass: f32) -> Self {
        assert!(mass > 0.0 && mass.is_finite(), "mass must be positive and finite");
        // Default inertia: treat as unit sphere (I = 2/5 * m * 1^2)
        let i = 0.4 * mass;
        let inv_i = 1.0 / i;
        Self {
            position: Vector3::zeros(),
            velocity: Vector3::zeros(),
            mass,
            inv_mass: 1.0 / mass,
            body_type: BodyType::Dynamic,
            restitution: 0.5,
            collider: None,
            friction: 0.3,
            force_accumulator: Vector3::zeros(),
            damping: 0.01,
            orientation: UnitQuaternion::identity(),
            angular_velocity: Vector3::zeros(),
            inv_inertia_local: Matrix3::from_diagonal(&Vector3::new(inv_i, inv_i, inv_i)),
            torque_accumulator: Vector3::zeros(),
            angular_damping: 0.01,
            active: true,
        }
    }

    /// Create a new static rigid body (infinite mass, unaffected by forces).
    pub fn new_static() -> Self {
        Self {
            position: Vector3::zeros(),
            velocity: Vector3::zeros(),
            mass: f32::INFINITY,
            inv_mass: 0.0,
            body_type: BodyType::Static,
            restitution: 0.5,
            collider: None,
            friction: 0.5,
            force_accumulator: Vector3::zeros(),
            damping: 0.0,
            orientation: UnitQuaternion::identity(),
            angular_velocity: Vector3::zeros(),
            inv_inertia_local: Matrix3::zeros(),
            torque_accumulator: Vector3::zeros(),
            angular_damping: 0.0,
            active: true,
        }
    }

    /// Builder: set initial position.
    pub fn with_position(mut self, position: Vector3<f32>) -> Self {
        self.position = position;
        self
    }

    /// Builder: set initial velocity.
    pub fn with_velocity(mut self, velocity: Vector3<f32>) -> Self {
        self.velocity = velocity;
        self
    }

    /// Builder: set restitution (bounciness, 0.0..=1.0).
    pub fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution.clamp(0.0, 1.0);
        self
    }

    /// Builder: set linear damping (0.0..=1.0).
    pub fn with_damping(mut self, damping: f32) -> Self {
        self.damping = damping.clamp(0.0, 1.0);
        self
    }

    /// Builder: attach a collider for collision detection.
    pub fn with_collider(mut self, collider: Collider) -> Self {
        self.collider = Some(collider);
        self
    }

    /// Builder: set friction coefficient (0.0..=1.0).
    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction.clamp(0.0, 1.0);
        self
    }

    /// Builder: set initial angular velocity (in radians per second).
    pub fn with_angular_velocity(mut self, angular_velocity: Vector3<f32>) -> Self {
        self.angular_velocity = angular_velocity;
        self
    }

    /// Builder: set angular damping (0.0..=1.0).
    pub fn with_angular_damping(mut self, damping: f32) -> Self {
        self.angular_damping = damping.clamp(0.0, 1.0);
        self
    }

    /// Builder: set the inertia tensor for a solid sphere of given radius.
    ///
    /// Inertia: `I = (2/5) * m * r²` (uniform along all axes).
    pub fn with_inertia_sphere(mut self, radius: f32) -> Self {
        if self.body_type == BodyType::Dynamic {
            let i = 0.4 * self.mass * radius * radius;
            let inv_i = 1.0 / i;
            self.inv_inertia_local = Matrix3::from_diagonal(&Vector3::new(inv_i, inv_i, inv_i));
        }
        self
    }

    /// Builder: set the inertia tensor for a solid box with given half-extents.
    ///
    /// For a box of full dimensions `(2*hx, 2*hy, 2*hz)`:
    /// - `Ixx = (1/12) * m * (4*hy² + 4*hz²)`
    /// - `Iyy = (1/12) * m * (4*hx² + 4*hz²)`
    /// - `Izz = (1/12) * m * (4*hx² + 4*hy²)`
    pub fn with_inertia_box(mut self, half_extents: Vector3<f32>) -> Self {
        if self.body_type == BodyType::Dynamic {
            let hx2 = 4.0 * half_extents.x * half_extents.x;
            let hy2 = 4.0 * half_extents.y * half_extents.y;
            let hz2 = 4.0 * half_extents.z * half_extents.z;
            let k = self.mass / 12.0;
            let ix = k * (hy2 + hz2);
            let iy = k * (hx2 + hz2);
            let iz = k * (hx2 + hy2);
            self.inv_inertia_local = Matrix3::from_diagonal(&Vector3::new(1.0 / ix, 1.0 / iy, 1.0 / iz));
        }
        self
    }

    /// Apply a force (in Newtons) to this body. Forces accumulate until the next `step()`.
    #[inline]
    pub fn apply_force(&mut self, force: Vector3<f32>) {
        self.force_accumulator += force;
    }

    /// Apply an instantaneous impulse (change in momentum) to this body.
    /// Directly modifies velocity: `delta_v = impulse / mass`.
    #[inline]
    pub fn apply_impulse(&mut self, impulse: Vector3<f32>) {
        if self.body_type == BodyType::Dynamic {
            self.velocity += impulse * self.inv_mass;
        }
    }

    /// Apply a torque (in N·m) to this body. Torques accumulate until the next `step()`.
    #[inline]
    pub fn apply_torque(&mut self, torque: Vector3<f32>) {
        self.torque_accumulator += torque;
    }

    /// Apply an instantaneous angular impulse (change in angular momentum).
    /// Directly modifies angular velocity: `delta_ω = I⁻¹ * impulse`.
    #[inline]
    pub fn apply_angular_impulse(&mut self, impulse: Vector3<f32>) {
        if self.body_type == BodyType::Dynamic {
            let inv_inertia_world = self.inv_inertia_world();
            self.angular_velocity += inv_inertia_world * impulse;
        }
    }

    /// Compute the world-space inverse inertia tensor from the local one and
    /// the current orientation: `I⁻¹_world = R * I⁻¹_local * Rᵀ`.
    #[inline]
    pub fn inv_inertia_world(&self) -> Matrix3<f32> {
        let r = self.orientation.to_rotation_matrix();
        r.matrix() * self.inv_inertia_local * r.matrix().transpose()
    }

    /// Returns the current speed (magnitude of velocity).
    #[inline]
    pub fn speed(&self) -> f32 {
        self.velocity.norm()
    }

    /// Returns the kinetic energy of this body: `0.5 * m * v^2`.
    #[inline]
    pub fn kinetic_energy(&self) -> f32 {
        0.5 * self.mass * self.velocity.norm_squared()
    }

    /// Integrate this body forward by `dt` seconds using semi-implicit Euler.
    ///
    /// Semi-implicit Euler updates velocity first, then position/orientation,
    /// which provides better energy conservation than explicit (forward) Euler.
    fn integrate(&mut self, dt: f32, gravity: Vector3<f32>) {
        if self.body_type != BodyType::Dynamic {
            self.force_accumulator = Vector3::zeros();
            self.torque_accumulator = Vector3::zeros();
            return;
        }

        // --- Linear ---
        let acceleration = gravity + self.force_accumulator * self.inv_mass;
        self.velocity += acceleration * dt;
        self.velocity *= 1.0 - self.damping;
        self.position += self.velocity * dt;

        // --- Angular ---
        let inv_inertia_world = self.inv_inertia_world();
        let angular_acceleration = inv_inertia_world * self.torque_accumulator;
        self.angular_velocity += angular_acceleration * dt;
        self.angular_velocity *= 1.0 - self.angular_damping;

        // Integrate orientation: q' = q + 0.5 * dt * ω * q
        // where ω * q is the quaternion product with ω encoded as (0, ωx, ωy, ωz)
        let w = &self.angular_velocity;
        let half_dt = 0.5 * dt;
        let dq = nalgebra::Quaternion::new(
            0.0,
            w.x * half_dt,
            w.y * half_dt,
            w.z * half_dt,
        );
        let q = self.orientation.into_inner();
        let new_q = q + dq * q;
        // Renormalize to prevent drift
        self.orientation = UnitQuaternion::new_normalize(new_q);

        // Clear accumulators
        self.force_accumulator = Vector3::zeros();
        self.torque_accumulator = Vector3::zeros();
    }
}

// ---------------------------------------------------------------------------
// Collision detection
// ---------------------------------------------------------------------------

/// Test two colliders for intersection and generate a contact if overlapping.
///
/// Returns `None` if the shapes are not overlapping.
/// The returned normal points from A toward B.
fn collide(
    pos_a: &Vector3<f32>,
    col_a: &Collider,
    pos_b: &Vector3<f32>,
    col_b: &Collider,
) -> Option<(Vector3<f32>, f32)> {
    match (col_a, col_b) {
        (Collider::Sphere { radius: ra }, Collider::Sphere { radius: rb }) => {
            collide_sphere_sphere(pos_a, *ra, pos_b, *rb)
        }
        (Collider::Aabb { half_extents: ha }, Collider::Aabb { half_extents: hb }) => {
            collide_aabb_aabb(pos_a, ha, pos_b, hb)
        }
        (Collider::Sphere { radius }, Collider::Aabb { half_extents }) => {
            collide_sphere_aabb(pos_a, *radius, pos_b, half_extents)
        }
        (Collider::Aabb { half_extents }, Collider::Sphere { radius }) => {
            // Flip: run sphere-aabb with swapped order, negate normal
            let result = collide_sphere_aabb(pos_b, *radius, pos_a, half_extents);
            result.map(|(normal, pen)| (-normal, pen))
        }
    }
}

/// Sphere vs Sphere intersection test.
///
/// Returns `(normal_a_to_b, penetration_depth)` or `None`.
fn collide_sphere_sphere(
    pos_a: &Vector3<f32>,
    radius_a: f32,
    pos_b: &Vector3<f32>,
    radius_b: f32,
) -> Option<(Vector3<f32>, f32)> {
    let diff = pos_b - pos_a;
    let dist_sq = diff.norm_squared();
    let sum_r = radius_a + radius_b;

    if dist_sq >= sum_r * sum_r {
        return None;
    }

    let dist = dist_sq.sqrt();
    let penetration = sum_r - dist;

    let normal = if dist > 1e-6 {
        diff / dist
    } else {
        // Perfectly overlapping — pick an arbitrary separation axis
        Vector3::new(0.0, 1.0, 0.0)
    };

    Some((normal, penetration))
}

/// AABB vs AABB intersection test using the Separating Axis Theorem.
///
/// Returns `(normal_a_to_b, penetration_depth)` or `None`.
fn collide_aabb_aabb(
    pos_a: &Vector3<f32>,
    half_a: &Vector3<f32>,
    pos_b: &Vector3<f32>,
    half_b: &Vector3<f32>,
) -> Option<(Vector3<f32>, f32)> {
    let diff = pos_b - pos_a;

    let overlap_x = half_a.x + half_b.x - diff.x.abs();
    if overlap_x <= 0.0 {
        return None;
    }
    let overlap_y = half_a.y + half_b.y - diff.y.abs();
    if overlap_y <= 0.0 {
        return None;
    }
    let overlap_z = half_a.z + half_b.z - diff.z.abs();
    if overlap_z <= 0.0 {
        return None;
    }

    // Pick the axis with minimum penetration (least overlap)
    if overlap_x <= overlap_y && overlap_x <= overlap_z {
        let sign = if diff.x >= 0.0 { 1.0 } else { -1.0 };
        Some((Vector3::new(sign, 0.0, 0.0), overlap_x))
    } else if overlap_y <= overlap_z {
        let sign = if diff.y >= 0.0 { 1.0 } else { -1.0 };
        Some((Vector3::new(0.0, sign, 0.0), overlap_y))
    } else {
        let sign = if diff.z >= 0.0 { 1.0 } else { -1.0 };
        Some((Vector3::new(0.0, 0.0, sign), overlap_z))
    }
}

/// Sphere vs AABB intersection test.
///
/// Finds the closest point on the AABB to the sphere center, then checks distance.
/// Returns `(normal_sphere_to_aabb, penetration_depth)` or `None`.
fn collide_sphere_aabb(
    sphere_pos: &Vector3<f32>,
    sphere_radius: f32,
    aabb_pos: &Vector3<f32>,
    aabb_half: &Vector3<f32>,
) -> Option<(Vector3<f32>, f32)> {
    // Find the closest point on the AABB to the sphere center
    let aabb_min = aabb_pos - aabb_half;
    let aabb_max = aabb_pos + aabb_half;

    let closest = Vector3::new(
        sphere_pos.x.clamp(aabb_min.x, aabb_max.x),
        sphere_pos.y.clamp(aabb_min.y, aabb_max.y),
        sphere_pos.z.clamp(aabb_min.z, aabb_max.z),
    );

    let diff = sphere_pos - closest;
    let dist_sq = diff.norm_squared();

    if dist_sq >= sphere_radius * sphere_radius {
        return None;
    }

    let dist = dist_sq.sqrt();

    if dist > 1e-6 {
        // Sphere center is outside the AABB — normal points from closest to sphere center
        // We want normal from sphere toward AABB, so negate
        let normal = -diff / dist;
        let penetration = sphere_radius - dist;
        Some((normal, penetration))
    } else {
        // Sphere center is inside the AABB — find the axis of least penetration
        let dx_pos = aabb_max.x - sphere_pos.x;
        let dx_neg = sphere_pos.x - aabb_min.x;
        let dy_pos = aabb_max.y - sphere_pos.y;
        let dy_neg = sphere_pos.y - aabb_min.y;
        let dz_pos = aabb_max.z - sphere_pos.z;
        let dz_neg = sphere_pos.z - aabb_min.z;

        let mut min_dist = dx_pos;
        let mut normal = Vector3::new(1.0, 0.0, 0.0);

        if dx_neg < min_dist {
            min_dist = dx_neg;
            normal = Vector3::new(-1.0, 0.0, 0.0);
        }
        if dy_pos < min_dist {
            min_dist = dy_pos;
            normal = Vector3::new(0.0, 1.0, 0.0);
        }
        if dy_neg < min_dist {
            min_dist = dy_neg;
            normal = Vector3::new(0.0, -1.0, 0.0);
        }
        if dz_pos < min_dist {
            min_dist = dz_pos;
            normal = Vector3::new(0.0, 0.0, 1.0);
        }
        if dz_neg < min_dist {
            min_dist = dz_neg;
            normal = Vector3::new(0.0, 0.0, -1.0);
        }

        let penetration = sphere_radius + min_dist;
        Some((normal, penetration))
    }
}

// ---------------------------------------------------------------------------
// Constraints & Joints
// ---------------------------------------------------------------------------

/// Unique identifier for a constraint within a [`PhysicsWorld`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstraintId(usize);

/// A constraint (joint) between two bodies.
///
/// Constraints restrict the relative motion of bodies. Each variant stores
/// body-local anchor points so the constraint follows the bodies as they move
/// and rotate.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Keeps two anchor points at a fixed distance (like a rigid rod).
    ///
    /// - `rest_length = 0`: the anchors are pinned together (point-to-point).
    /// - `compliance > 0`: soft spring-like behaviour (higher = softer).
    Distance {
        body_a: BodyId,
        body_b: BodyId,
        /// Anchor offset in body A's local space.
        anchor_a: Vector3<f32>,
        /// Anchor offset in body B's local space.
        anchor_b: Vector3<f32>,
        /// Target distance between the two anchor points.
        rest_length: f32,
        /// Compliance (inverse stiffness). 0.0 = perfectly rigid.
        compliance: f32,
    },
    /// Ball-socket joint: constrains two body-local points to coincide.
    ///
    /// Equivalent to a distance constraint with `rest_length = 0`, but
    /// named for clarity.
    BallSocket {
        body_a: BodyId,
        body_b: BodyId,
        /// Anchor offset in body A's local space.
        anchor_a: Vector3<f32>,
        /// Anchor offset in body B's local space.
        anchor_b: Vector3<f32>,
        /// Compliance (inverse stiffness). 0.0 = perfectly rigid.
        compliance: f32,
    },
    /// Fixed (weld) joint: locks relative position and orientation.
    ///
    /// Stores the initial relative orientation so it can be enforced.
    Fixed {
        body_a: BodyId,
        body_b: BodyId,
        /// Anchor offset in body A's local space.
        anchor_a: Vector3<f32>,
        /// Anchor offset in body B's local space.
        anchor_b: Vector3<f32>,
        /// Target relative orientation: `q_b * q_a⁻¹` at rest.
        relative_orientation: UnitQuaternion<f32>,
        /// Compliance (inverse stiffness). 0.0 = perfectly rigid.
        compliance: f32,
    },
}

/// The physics simulation world.
///
/// Manages a fixed-capacity set of rigid bodies and constraints, and steps
/// the simulation forward.
///
/// # Type Parameters
/// * `N` - Maximum number of bodies (compile-time capacity).
/// * `M` - Maximum number of constraints/joints (compile-time capacity).
///
/// # Example
/// ```
/// use embedded_3dgfx::physics::{PhysicsWorld, RigidBody, Constraint};
/// use nalgebra::Vector3;
///
/// let mut world = PhysicsWorld::<8, 4>::new();
/// world.set_gravity(Vector3::new(0.0, -9.81, 0.0));
///
/// let body = RigidBody::new(2.0)
///     .with_position(Vector3::new(0.0, 5.0, 0.0));
/// let id = world.add_body(body).unwrap();
///
/// world.step::<8>(1.0 / 60.0);
/// ```
pub struct PhysicsWorld<const N: usize, const M: usize = 0> {
    bodies: heapless::Vec<RigidBody, N>,
    constraints: heapless::Vec<Constraint, M>,
    gravity: Vector3<f32>,
    /// Number of iterations for the constraint solver per substep.
    pub solver_iterations: u32,
}

impl<const N: usize, const M: usize> PhysicsWorld<N, M> {
    /// Create a new physics world with no gravity.
    pub fn new() -> Self {
        Self {
            bodies: heapless::Vec::new(),
            constraints: heapless::Vec::new(),
            gravity: Vector3::zeros(),
            solver_iterations: 4,
        }
    }

    /// Set the gravity vector (e.g., `Vector3::new(0.0, -9.81, 0.0)`).
    pub fn set_gravity(&mut self, gravity: Vector3<f32>) {
        self.gravity = gravity;
    }

    /// Returns the current gravity vector.
    pub fn gravity(&self) -> Vector3<f32> {
        self.gravity
    }

    /// Add a body to the world. Returns its [`BodyId`], or `None` if at capacity.
    pub fn add_body(&mut self, body: RigidBody) -> Option<BodyId> {
        let id = BodyId(self.bodies.len());
        self.bodies.push(body).ok()?;
        Some(id)
    }

    /// Get an immutable reference to a body by its ID.
    pub fn body(&self, id: BodyId) -> Option<&RigidBody> {
        self.bodies.get(id.0)
    }

    /// Get a mutable reference to a body by its ID.
    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut RigidBody> {
        self.bodies.get_mut(id.0)
    }

    /// Returns the total number of bodies in the world (including inactive).
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Returns the number of active bodies in the world.
    pub fn active_body_count(&self) -> usize {
        self.bodies.iter().filter(|b| b.active).count()
    }

    /// Deactivate a body, effectively removing it from the simulation.
    ///
    /// The body remains in the world (its slot is preserved) but it is skipped
    /// during integration and collision detection. This avoids invalidating
    /// existing [`BodyId`]s.
    ///
    /// Returns `true` if the body was found and deactivated, `false` if the ID
    /// was out of bounds or the body was already inactive.
    pub fn remove_body(&mut self, id: BodyId) -> bool {
        if let Some(body) = self.bodies.get_mut(id.0) {
            if body.active {
                body.active = false;
                body.velocity = Vector3::zeros();
                body.angular_velocity = Vector3::zeros();
                body.force_accumulator = Vector3::zeros();
                body.torque_accumulator = Vector3::zeros();
                return true;
            }
        }
        false
    }

    /// Set whether a body is active. Inactive bodies are skipped during
    /// integration and collision detection.
    ///
    /// Returns `true` if the body exists, `false` otherwise.
    pub fn set_active(&mut self, id: BodyId, active: bool) -> bool {
        if let Some(body) = self.bodies.get_mut(id.0) {
            body.active = active;
            true
        } else {
            false
        }
    }

    /// Iterate over all bodies immutably.
    pub fn bodies(&self) -> impl Iterator<Item = (BodyId, &RigidBody)> {
        self.bodies.iter().enumerate().map(|(i, b)| (BodyId(i), b))
    }

    /// Iterate over all bodies mutably.
    pub fn bodies_mut(&mut self) -> impl Iterator<Item = (BodyId, &mut RigidBody)> {
        self.bodies.iter_mut().enumerate().map(|(i, b)| (BodyId(i), b))
    }

    // -- Constraint management --

    /// Add a constraint to the world. Returns its [`ConstraintId`], or `None`
    /// if at capacity.
    pub fn add_constraint(&mut self, constraint: Constraint) -> Option<ConstraintId> {
        let id = ConstraintId(self.constraints.len());
        self.constraints.push(constraint).ok()?;
        Some(id)
    }

    /// Get an immutable reference to a constraint by its ID.
    pub fn constraint(&self, id: ConstraintId) -> Option<&Constraint> {
        self.constraints.get(id.0)
    }

    /// Get a mutable reference to a constraint by its ID.
    pub fn constraint_mut(&mut self, id: ConstraintId) -> Option<&mut Constraint> {
        self.constraints.get_mut(id.0)
    }

    /// Remove a constraint by ID (swap-removes; invalidates the last ID).
    ///
    /// Returns `true` if a constraint was removed, `false` if the ID was
    /// out of bounds.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> bool {
        if id.0 < self.constraints.len() {
            self.constraints.swap_remove(id.0);
            true
        } else {
            false
        }
    }

    /// Returns the number of constraints in the world.
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Helper: create a distance constraint between two bodies.
    ///
    /// Anchors are in body-local space. The rest length is computed
    /// automatically from the current body positions.
    pub fn add_distance_constraint(
        &mut self,
        body_a: BodyId,
        anchor_a: Vector3<f32>,
        body_b: BodyId,
        anchor_b: Vector3<f32>,
        compliance: f32,
    ) -> Option<ConstraintId> {
        let world_a = self.body_to_world(body_a, &anchor_a)?;
        let world_b = self.body_to_world(body_b, &anchor_b)?;
        let rest_length = (world_b - world_a).norm();

        self.add_constraint(Constraint::Distance {
            body_a,
            body_b,
            anchor_a,
            anchor_b,
            rest_length,
            compliance,
        })
    }

    /// Helper: create a ball-socket joint between two bodies.
    ///
    /// Anchors are in body-local space. The two anchor points will be
    /// constrained to coincide (distance = 0).
    pub fn add_ball_socket(
        &mut self,
        body_a: BodyId,
        anchor_a: Vector3<f32>,
        body_b: BodyId,
        anchor_b: Vector3<f32>,
        compliance: f32,
    ) -> Option<ConstraintId> {
        self.add_constraint(Constraint::BallSocket {
            body_a,
            body_b,
            anchor_a,
            anchor_b,
            compliance,
        })
    }

    /// Helper: create a fixed (weld) joint between two bodies.
    ///
    /// Stores the current relative orientation as the target.
    pub fn add_fixed_joint(
        &mut self,
        body_a: BodyId,
        anchor_a: Vector3<f32>,
        body_b: BodyId,
        anchor_b: Vector3<f32>,
        compliance: f32,
    ) -> Option<ConstraintId> {
        let qa = self.bodies.get(body_a.0)?.orientation;
        let qb = self.bodies.get(body_b.0)?.orientation;
        let relative_orientation = qb * qa.inverse();

        self.add_constraint(Constraint::Fixed {
            body_a,
            body_b,
            anchor_a,
            anchor_b,
            relative_orientation,
            compliance,
        })
    }

    /// Convert a body-local point to world space.
    fn body_to_world(&self, id: BodyId, local_point: &Vector3<f32>) -> Option<Vector3<f32>> {
        let body = self.bodies.get(id.0)?;
        Some(body.position + body.orientation * local_point)
    }

    /// Detect all collisions between bodies with colliders.
    ///
    /// Returns contacts in a fixed-capacity buffer. The `C` const generic sets the
    /// maximum number of contacts per detection pass. For `N` bodies, worst case is
    /// `N*(N-1)/2` pairs, so choose `C` accordingly.
    ///
    /// Pairs where both bodies lack a collider are skipped.
    pub fn detect_collisions<const C: usize>(&self) -> heapless::Vec<Contact, C> {
        let mut contacts = heapless::Vec::new();
        let len = self.bodies.len();

        for i in 0..len {
            let body_a = &self.bodies[i];
            if !body_a.active {
                continue;
            }
            let col_a = match &body_a.collider {
                Some(c) => c,
                None => continue,
            };

            for j in (i + 1)..len {
                let body_b = &self.bodies[j];
                if !body_b.active {
                    continue;
                }
                let col_b = match &body_b.collider {
                    Some(c) => c,
                    None => continue,
                };

                // Skip pairs where both are static
                if body_a.body_type == BodyType::Static && body_b.body_type == BodyType::Static {
                    continue;
                }

                if let Some((normal, penetration)) =
                    collide(&body_a.position, col_a, &body_b.position, col_b)
                {
                    let _ = contacts.push(Contact {
                        body_a: BodyId(i),
                        body_b: BodyId(j),
                        normal,
                        penetration,
                    });
                }
            }
        }

        contacts
    }

    /// Resolve a set of contacts by applying positional correction, impulse response
    /// (linear + angular), and Coulomb friction.
    ///
    /// Contact-point velocity includes both linear and angular contributions:
    /// `v_contact = v_linear + ω × r`, where `r` is the vector from the body
    /// center to the contact point. Impulses produce both linear velocity changes
    /// and torques via `τ = r × J`.
    pub fn resolve_contacts(&mut self, contacts: &[Contact]) {
        for contact in contacts {
            let a = contact.body_a.0;
            let b = contact.body_b.0;

            let inv_mass_a = self.bodies[a].inv_mass;
            let inv_mass_b = self.bodies[b].inv_mass;

            // Compute contact point: on the surface of each body along the normal.
            // For simplicity, use the midpoint of the overlap region along the normal.
            let contact_point = self.bodies[a].position
                + contact.normal * (contact.penetration * 0.5);

            let ra = contact_point - self.bodies[a].position;
            let rb = contact_point - self.bodies[b].position;

            let inv_inertia_a = self.bodies[a].inv_inertia_world();
            let inv_inertia_b = self.bodies[b].inv_inertia_world();

            // --- Positional correction (push bodies apart) ---
            let inv_mass_sum = inv_mass_a + inv_mass_b;
            if inv_mass_sum == 0.0 {
                continue; // Both static
            }
            let correction = contact.normal * (contact.penetration / inv_mass_sum);
            self.bodies[a].position -= correction * inv_mass_a;
            self.bodies[b].position += correction * inv_mass_b;

            // --- Contact-point velocity (linear + angular) ---
            let vel_a = self.bodies[a].velocity
                + self.bodies[a].angular_velocity.cross(&ra);
            let vel_b = self.bodies[b].velocity
                + self.bodies[b].angular_velocity.cross(&rb);
            let relative_vel = vel_b - vel_a;
            let vel_along_normal = relative_vel.dot(&contact.normal);

            // Only resolve if bodies are moving toward each other
            if vel_along_normal >= 0.0 {
                continue;
            }

            let restitution = self.bodies[a]
                .restitution
                .min(self.bodies[b].restitution);

            // Effective mass including angular contribution:
            // 1/m_eff = 1/m_a + 1/m_b + (I_a⁻¹(ra×n))×ra·n + (I_b⁻¹(rb×n))×rb·n
            let ra_cross_n = ra.cross(&contact.normal);
            let rb_cross_n = rb.cross(&contact.normal);
            let angular_term_a = (inv_inertia_a * ra_cross_n).cross(&ra).dot(&contact.normal);
            let angular_term_b = (inv_inertia_b * rb_cross_n).cross(&rb).dot(&contact.normal);
            let eff_mass_inv = inv_mass_a + inv_mass_b + angular_term_a + angular_term_b;

            if eff_mass_inv <= 0.0 {
                continue;
            }

            // Normal impulse magnitude
            let j = -(1.0 + restitution) * vel_along_normal / eff_mass_inv;

            let impulse = contact.normal * j;
            self.bodies[a].velocity -= impulse * inv_mass_a;
            self.bodies[b].velocity += impulse * inv_mass_b;
            self.bodies[a].angular_velocity -= inv_inertia_a * ra.cross(&impulse);
            self.bodies[b].angular_velocity += inv_inertia_b * rb.cross(&impulse);

            // --- Friction impulse (Coulomb model) ---
            let mu_a = self.bodies[a].friction;
            let mu_b = self.bodies[b].friction;
            let mu = (mu_a * mu_b).sqrt();

            if mu > 1e-6 {
                // Recompute relative velocity at contact point after normal impulse
                let vel_a = self.bodies[a].velocity
                    + self.bodies[a].angular_velocity.cross(&ra);
                let vel_b = self.bodies[b].velocity
                    + self.bodies[b].angular_velocity.cross(&rb);
                let relative_vel = vel_b - vel_a;

                let vn = relative_vel.dot(&contact.normal);
                let tangent_vel = relative_vel - contact.normal * vn;
                let tangent_speed = tangent_vel.norm();

                if tangent_speed > 1e-6 {
                    let tangent = tangent_vel / tangent_speed;

                    // Effective mass in the tangent direction (with angular terms)
                    let ra_cross_t = ra.cross(&tangent);
                    let rb_cross_t = rb.cross(&tangent);
                    let ang_t_a = (inv_inertia_a * ra_cross_t).cross(&ra).dot(&tangent);
                    let ang_t_b = (inv_inertia_b * rb_cross_t).cross(&rb).dot(&tangent);
                    let eff_mass_t_inv = inv_mass_a + inv_mass_b + ang_t_a + ang_t_b;

                    if eff_mass_t_inv > 0.0 {
                        // Clamp by Coulomb cone: |jt| <= mu * |jn|
                        let jt = (-tangent_speed / eff_mass_t_inv).max(-mu * j);

                        let friction_impulse = tangent * jt;
                        self.bodies[a].velocity -= friction_impulse * inv_mass_a;
                        self.bodies[b].velocity += friction_impulse * inv_mass_b;
                        self.bodies[a].angular_velocity -= inv_inertia_a * ra.cross(&friction_impulse);
                        self.bodies[b].angular_velocity += inv_inertia_b * rb.cross(&friction_impulse);
                    }
                }
            }
        }
    }

    // -- Constraint solver --

    /// Solve all constraints using position-based correction.
    ///
    /// Uses Extended Position-Based Dynamics (XPBD) style positional correction
    /// with compliance for soft constraints. Each constraint is solved
    /// `solver_iterations` times per call for convergence.
    pub fn solve_constraints(&mut self, dt: f32) {
        if self.constraints.is_empty() || dt <= 0.0 {
            return;
        }

        let iterations = self.solver_iterations;
        for _ in 0..iterations {
            // Iterate constraints by index to satisfy the borrow checker
            for ci in 0..self.constraints.len() {
                // Clone the constraint to avoid borrow conflict
                let constraint = self.constraints[ci].clone();
                match &constraint {
                    Constraint::Distance {
                        body_a,
                        body_b,
                        anchor_a,
                        anchor_b,
                        rest_length,
                        compliance,
                    } => {
                        self.solve_distance(
                            body_a.0,
                            anchor_a,
                            body_b.0,
                            anchor_b,
                            *rest_length,
                            *compliance,
                            dt,
                        );
                    }
                    Constraint::BallSocket {
                        body_a,
                        body_b,
                        anchor_a,
                        anchor_b,
                        compliance,
                    } => {
                        self.solve_distance(
                            body_a.0,
                            anchor_a,
                            body_b.0,
                            anchor_b,
                            0.0,
                            *compliance,
                            dt,
                        );
                    }
                    Constraint::Fixed {
                        body_a,
                        body_b,
                        anchor_a,
                        anchor_b,
                        relative_orientation,
                        compliance,
                    } => {
                        // Positional part (same as ball-socket)
                        self.solve_distance(
                            body_a.0,
                            anchor_a,
                            body_b.0,
                            anchor_b,
                            0.0,
                            *compliance,
                            dt,
                        );
                        // Rotational part
                        self.solve_orientation(
                            body_a.0,
                            body_b.0,
                            relative_orientation,
                            *compliance,
                            dt,
                        );
                    }
                }
            }
        }
    }

    /// Solve a single distance constraint between two anchor points.
    ///
    /// Uses XPBD positional correction: applies positional deltas and
    /// corresponding velocity updates to both bodies.
    fn solve_distance(
        &mut self,
        a: usize,
        anchor_a: &Vector3<f32>,
        b: usize,
        anchor_b: &Vector3<f32>,
        rest_length: f32,
        compliance: f32,
        dt: f32,
    ) {
        if !self.bodies[a].active && !self.bodies[b].active {
            return;
        }

        // World-space anchor positions
        let ra = self.bodies[a].orientation * anchor_a;
        let rb = self.bodies[b].orientation * anchor_b;
        let world_a = self.bodies[a].position + ra;
        let world_b = self.bodies[b].position + rb;

        let diff = world_b - world_a;
        let dist = diff.norm();

        let error = dist - rest_length;
        if error.abs() < 1e-6 {
            return;
        }

        // Direction of correction
        let n = if dist > 1e-6 {
            diff / dist
        } else {
            Vector3::new(0.0, 1.0, 0.0) // Arbitrary axis for zero-length
        };

        // Generalized inverse masses (linear + angular contribution)
        let inv_mass_a = self.bodies[a].inv_mass;
        let inv_mass_b = self.bodies[b].inv_mass;
        let inv_inertia_a = self.bodies[a].inv_inertia_world();
        let inv_inertia_b = self.bodies[b].inv_inertia_world();

        let ra_cross_n = ra.cross(&n);
        let rb_cross_n = rb.cross(&n);
        let w_a = inv_mass_a + ra_cross_n.dot(&(inv_inertia_a * ra_cross_n));
        let w_b = inv_mass_b + rb_cross_n.dot(&(inv_inertia_b * rb_cross_n));
        let w_sum = w_a + w_b;

        if w_sum <= 0.0 {
            return;
        }

        // XPBD: compliance term scaled by dt²
        let alpha = compliance / (dt * dt);
        let delta_lambda = -error / (w_sum + alpha);
        let correction = n * delta_lambda;

        // Apply positional correction
        self.bodies[a].position -= correction * inv_mass_a;
        self.bodies[b].position += correction * inv_mass_b;

        // Apply angular correction
        self.bodies[a].angular_velocity -= inv_inertia_a * ra.cross(&correction) / dt;
        self.bodies[b].angular_velocity += inv_inertia_b * rb.cross(&correction) / dt;

        // Update linear velocities from positional correction
        self.bodies[a].velocity -= correction * inv_mass_a / dt;
        self.bodies[b].velocity += correction * inv_mass_b / dt;
    }

    /// Solve the rotational part of a fixed (weld) joint.
    ///
    /// Applies angular corrections to enforce the target relative orientation.
    fn solve_orientation(
        &mut self,
        a: usize,
        b: usize,
        target_rel: &UnitQuaternion<f32>,
        compliance: f32,
        dt: f32,
    ) {
        if !self.bodies[a].active && !self.bodies[b].active {
            return;
        }

        let qa = self.bodies[a].orientation;
        let qb = self.bodies[b].orientation;

        // Current relative orientation vs target
        let current_rel = qb * qa.inverse();
        let error_q = current_rel * target_rel.inverse();

        // Extract the rotation axis and angle from the error quaternion
        let error_inner = error_q.into_inner();
        let error_vec = Vector3::new(error_inner.i, error_inner.j, error_inner.k);

        // For small angles: rotation vector ≈ 2 * [i, j, k] (when w > 0)
        let error = if error_inner.w >= 0.0 {
            error_vec * 2.0
        } else {
            -error_vec * 2.0
        };

        if error.norm_squared() < 1e-10 {
            return;
        }

        let inv_inertia_a = self.bodies[a].inv_inertia_world();
        let inv_inertia_b = self.bodies[b].inv_inertia_world();

        // Generalized inverse masses for rotation
        // For each axis, the angular "mass" contribution is n · (I⁻¹ * n)
        // Simplified: use the sum of diagonal elements as effective inverse mass
        let w_a = if self.bodies[a].body_type == BodyType::Dynamic {
            inv_inertia_a[(0, 0)] + inv_inertia_a[(1, 1)] + inv_inertia_a[(2, 2)]
        } else {
            0.0
        };
        let w_b = if self.bodies[b].body_type == BodyType::Dynamic {
            inv_inertia_b[(0, 0)] + inv_inertia_b[(1, 1)] + inv_inertia_b[(2, 2)]
        } else {
            0.0
        };
        let w_sum = w_a + w_b;
        if w_sum <= 0.0 {
            return;
        }

        let alpha = compliance / (dt * dt);
        let delta_lambda = -error / (w_sum + alpha);

        // Apply angular correction
        if self.bodies[a].body_type == BodyType::Dynamic {
            self.bodies[a].angular_velocity -= inv_inertia_a * delta_lambda / dt;
        }
        if self.bodies[b].body_type == BodyType::Dynamic {
            self.bodies[b].angular_velocity += inv_inertia_b * delta_lambda / dt;
        }
    }

    /// Advance the simulation by `dt` seconds.
    ///
    /// Integrates all dynamic bodies, detects and resolves collisions, then
    /// solves constraints.
    ///
    /// The `C` const generic sets the maximum number of collision contacts per step.
    pub fn step<const C: usize>(&mut self, dt: f32) {
        let gravity = self.gravity;
        for body in self.bodies.iter_mut() {
            if body.active {
                body.integrate(dt, gravity);
            }
        }

        let contacts = self.detect_collisions::<C>();
        self.resolve_contacts(&contacts);
        self.solve_constraints(dt);
    }

    /// Advance the simulation using fixed-size substeps for stability.
    ///
    /// Divides `dt` into `substeps` equal intervals. Each substep runs integration,
    /// collision detection/response, and constraint solving.
    pub fn step_fixed<const C: usize>(&mut self, dt: f32, substeps: u32) {
        let sub_dt = dt / substeps as f32;
        for _ in 0..substeps {
            self.step::<C>(sub_dt);
        }
    }
}

/// Helper to sync a [`RigidBody`]'s position and orientation back to a
/// [`K3dMesh`](crate::mesh::K3dMesh).
///
/// Call this after `PhysicsWorld::step()` to update mesh transforms.
///
/// # Example
/// ```ignore
/// physics_world.step::<16>(dt);
/// for (id, body) in physics_world.bodies() {
///     sync_body_to_mesh(body, &mut meshes[id]);
/// }
/// ```
pub fn sync_body_to_mesh(body: &RigidBody, mesh: &mut crate::mesh::K3dMesh<'_>) {
    mesh.set_position(body.position.x, body.position.y, body.position.z);
    mesh.set_rotation(body.orientation);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    const EPSILON: f32 = 1e-4;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    fn approx_vec_eq(a: &Vector3<f32>, b: &Vector3<f32>) -> bool {
        approx_eq(a.x, b.x) && approx_eq(a.y, b.y) && approx_eq(a.z, b.z)
    }

    // -- RigidBody tests --

    #[test]
    fn test_body_creation() {
        let body = RigidBody::new(5.0);
        assert_eq!(body.mass, 5.0);
        assert!(approx_eq(body.inv_mass, 0.2));
        assert_eq!(body.body_type, BodyType::Dynamic);
        assert!(approx_vec_eq(&body.position, &Vector3::zeros()));
        assert!(approx_vec_eq(&body.velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_static_body() {
        let body = RigidBody::new_static();
        assert_eq!(body.body_type, BodyType::Static);
        assert_eq!(body.inv_mass, 0.0);
        assert!(body.mass.is_infinite());
    }

    #[test]
    #[should_panic]
    fn test_body_zero_mass_panics() {
        RigidBody::new(0.0);
    }

    #[test]
    #[should_panic]
    fn test_body_negative_mass_panics() {
        RigidBody::new(-1.0);
    }

    #[test]
    fn test_builder_pattern() {
        let body = RigidBody::new(1.0)
            .with_position(Vector3::new(1.0, 2.0, 3.0))
            .with_velocity(Vector3::new(0.0, 5.0, 0.0))
            .with_restitution(0.8)
            .with_damping(0.05);

        assert!(approx_vec_eq(&body.position, &Vector3::new(1.0, 2.0, 3.0)));
        assert!(approx_vec_eq(&body.velocity, &Vector3::new(0.0, 5.0, 0.0)));
        assert!(approx_eq(body.restitution, 0.8));
        assert!(approx_eq(body.damping, 0.05));
    }

    #[test]
    fn test_apply_force() {
        let mut body = RigidBody::new(1.0);
        body.apply_force(Vector3::new(10.0, 0.0, 0.0));
        body.apply_force(Vector3::new(0.0, 5.0, 0.0));

        // Forces accumulate
        assert!(approx_vec_eq(
            &body.force_accumulator,
            &Vector3::new(10.0, 5.0, 0.0)
        ));
    }

    #[test]
    fn test_apply_impulse() {
        let mut body = RigidBody::new(2.0);
        body.apply_impulse(Vector3::new(10.0, 0.0, 0.0));

        // delta_v = impulse / mass = 10 / 2 = 5
        assert!(approx_vec_eq(&body.velocity, &Vector3::new(5.0, 0.0, 0.0)));
    }

    #[test]
    fn test_impulse_on_static_body_ignored() {
        let mut body = RigidBody::new_static();
        body.apply_impulse(Vector3::new(100.0, 0.0, 0.0));
        assert!(approx_vec_eq(&body.velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_speed() {
        let body = RigidBody::new(1.0)
            .with_velocity(Vector3::new(3.0, 4.0, 0.0));
        assert!(approx_eq(body.speed(), 5.0));
    }

    #[test]
    fn test_kinetic_energy() {
        let body = RigidBody::new(2.0)
            .with_velocity(Vector3::new(3.0, 0.0, 0.0));
        // KE = 0.5 * 2 * 9 = 9
        assert!(approx_eq(body.kinetic_energy(), 9.0));
    }

    // -- PhysicsWorld tests --

    #[test]
    fn test_world_creation() {
        let world = PhysicsWorld::<8>::new();
        assert_eq!(world.body_count(), 0);
        assert!(approx_vec_eq(&world.gravity(), &Vector3::zeros()));
    }

    #[test]
    fn test_add_and_get_body() {
        let mut world = PhysicsWorld::<8>::new();
        let body = RigidBody::new(1.0).with_position(Vector3::new(1.0, 2.0, 3.0));
        let id = world.add_body(body).unwrap();

        assert_eq!(world.body_count(), 1);
        let b = world.body(id).unwrap();
        assert!(approx_vec_eq(&b.position, &Vector3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn test_add_body_at_capacity() {
        let mut world = PhysicsWorld::<2>::new();
        assert!(world.add_body(RigidBody::new(1.0)).is_some());
        assert!(world.add_body(RigidBody::new(1.0)).is_some());
        assert!(world.add_body(RigidBody::new(1.0)).is_none()); // at capacity
    }

    #[test]
    fn test_gravity_freefall() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let body = RigidBody::new(1.0)
            .with_position(Vector3::new(0.0, 100.0, 0.0))
            .with_damping(0.0);
        let id = world.add_body(body).unwrap();

        // Step 1 second
        world.step::<4>(1.0);

        let b = world.body(id).unwrap();
        // After 1s of freefall at -10 m/s²:
        // v = 0 + (-10)*1 = -10 m/s
        // p = 100 + (-10)*1 = 90 m  (semi-implicit: velocity updated first, then position)
        assert!(approx_eq(b.velocity.y, -10.0));
        assert!(approx_eq(b.position.y, 90.0));
    }

    #[test]
    fn test_static_body_not_affected_by_gravity() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -9.81, 0.0));

        let body = RigidBody::new_static()
            .with_position(Vector3::new(0.0, 0.0, 0.0));
        let id = world.add_body(body).unwrap();

        world.step::<4>(1.0);

        let b = world.body(id).unwrap();
        assert!(approx_vec_eq(&b.position, &Vector3::zeros()));
        assert!(approx_vec_eq(&b.velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_force_accumulation_and_clearing() {
        let mut world = PhysicsWorld::<4>::new();

        let body = RigidBody::new(1.0).with_damping(0.0);
        let id = world.add_body(body).unwrap();

        // Apply a force and step
        world.body_mut(id).unwrap().apply_force(Vector3::new(10.0, 0.0, 0.0));
        world.step::<4>(1.0);

        let b = world.body(id).unwrap();
        assert!(approx_eq(b.velocity.x, 10.0));

        // Forces should be cleared — another step with no forces should not accelerate further
        world.step::<4>(1.0);
        let b = world.body(id).unwrap();
        assert!(approx_eq(b.velocity.x, 10.0)); // unchanged (no damping, no forces)
    }

    #[test]
    fn test_damping_reduces_velocity() {
        let mut world = PhysicsWorld::<4>::new();

        let body = RigidBody::new(1.0)
            .with_velocity(Vector3::new(10.0, 0.0, 0.0))
            .with_damping(0.1);
        let id = world.add_body(body).unwrap();

        world.step::<4>(1.0);

        let b = world.body(id).unwrap();
        // velocity should be reduced by damping
        assert!(b.velocity.x < 10.0);
        assert!(b.velocity.x > 0.0);
    }

    #[test]
    fn test_step_fixed_substeps() {
        let mut world_single = PhysicsWorld::<4>::new();
        let mut world_sub = PhysicsWorld::<4>::new();

        world_single.set_gravity(Vector3::new(0.0, -10.0, 0.0));
        world_sub.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let body = RigidBody::new(1.0)
            .with_position(Vector3::new(0.0, 100.0, 0.0))
            .with_damping(0.0);

        let id1 = world_single.add_body(body.clone()).unwrap();
        let id2 = world_sub.add_body(body).unwrap();

        // Single large step vs 10 substeps
        world_single.step::<4>(1.0);
        world_sub.step_fixed::<4>(1.0, 10);

        // Both should give similar results (substeps are more accurate)
        let b1 = world_single.body(id1).unwrap();
        let b2 = world_sub.body(id2).unwrap();

        // Velocities should be the same (gravity is constant)
        assert!(approx_eq(b1.velocity.y, b2.velocity.y));
    }

    #[test]
    fn test_bodies_iterator() {
        let mut world = PhysicsWorld::<4>::new();
        world.add_body(RigidBody::new(1.0).with_position(Vector3::new(1.0, 0.0, 0.0))).unwrap();
        world.add_body(RigidBody::new(2.0).with_position(Vector3::new(2.0, 0.0, 0.0))).unwrap();

        let positions: std::vec::Vec<f32> = world.bodies().map(|(_, b)| b.position.x).collect();
        assert_eq!(positions, std::vec![1.0, 2.0]);
    }

    #[test]
    fn test_projectile_motion() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        // Launch at 45 degrees: vx=10, vy=10
        let body = RigidBody::new(1.0)
            .with_velocity(Vector3::new(10.0, 10.0, 0.0))
            .with_damping(0.0);
        let id = world.add_body(body).unwrap();

        // Step for 2 seconds (at t=2, vy should be 10 - 20 = -10)
        for _ in 0..200 {
            world.step::<4>(0.01);
        }

        let b = world.body(id).unwrap();
        // x position: ~20m (10 m/s * 2s)
        assert!((b.position.x - 20.0).abs() < 0.5);
        // y velocity: ~-10 m/s
        assert!((b.velocity.y - (-10.0)).abs() < 0.5);
    }

    // -- Collision detection tests --

    #[test]
    fn test_sphere_sphere_no_collision() {
        let result = collide_sphere_sphere(
            &Vector3::new(0.0, 0.0, 0.0),
            1.0,
            &Vector3::new(3.0, 0.0, 0.0),
            1.0,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_sphere_sphere_touching() {
        let result = collide_sphere_sphere(
            &Vector3::new(0.0, 0.0, 0.0),
            1.0,
            &Vector3::new(2.0, 0.0, 0.0),
            1.0,
        );
        // Exactly touching — no penetration
        assert!(result.is_none());
    }

    #[test]
    fn test_sphere_sphere_overlapping() {
        let result = collide_sphere_sphere(
            &Vector3::new(0.0, 0.0, 0.0),
            1.0,
            &Vector3::new(1.5, 0.0, 0.0),
            1.0,
        );
        let (normal, penetration) = result.unwrap();
        assert!(approx_eq(penetration, 0.5));
        // Normal should point from A to B (positive X)
        assert!(normal.x > 0.0);
        assert!(approx_eq(normal.y, 0.0));
    }

    #[test]
    fn test_sphere_sphere_coincident() {
        let result = collide_sphere_sphere(
            &Vector3::new(0.0, 0.0, 0.0),
            1.0,
            &Vector3::new(0.0, 0.0, 0.0),
            1.0,
        );
        let (normal, penetration) = result.unwrap();
        assert!(approx_eq(penetration, 2.0));
        // Should get an arbitrary valid normal
        assert!(approx_eq(normal.norm(), 1.0));
    }

    #[test]
    fn test_aabb_aabb_no_collision() {
        let result = collide_aabb_aabb(
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
            &Vector3::new(3.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_aabb_aabb_overlapping_x() {
        let result = collide_aabb_aabb(
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
            &Vector3::new(1.5, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        let (normal, penetration) = result.unwrap();
        // X axis has least penetration (0.5) vs Y (2.0) and Z (2.0)
        assert!(approx_eq(penetration, 0.5));
        assert!(approx_eq(normal.x, 1.0));
    }

    #[test]
    fn test_aabb_aabb_overlapping_y() {
        let result = collide_aabb_aabb(
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
            &Vector3::new(0.0, 1.5, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        let (normal, penetration) = result.unwrap();
        assert!(approx_eq(penetration, 0.5));
        assert!(approx_eq(normal.y, 1.0));
    }

    #[test]
    fn test_aabb_aabb_negative_direction() {
        let result = collide_aabb_aabb(
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
            &Vector3::new(-1.5, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        let (normal, _) = result.unwrap();
        // Normal should point negative X (from A toward B)
        assert!(approx_eq(normal.x, -1.0));
    }

    #[test]
    fn test_sphere_aabb_no_collision() {
        let result = collide_sphere_aabb(
            &Vector3::new(5.0, 0.0, 0.0),
            1.0,
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_sphere_aabb_overlapping() {
        // Sphere at x=1.5 with radius 1, AABB from -1 to 1
        // Closest point on AABB to sphere center is (1, 0, 0)
        // Distance = 0.5, penetration = 1.0 - 0.5 = 0.5
        let result = collide_sphere_aabb(
            &Vector3::new(1.5, 0.0, 0.0),
            1.0,
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(1.0, 1.0, 1.0),
        );
        let (normal, penetration) = result.unwrap();
        assert!(approx_eq(penetration, 0.5));
        // Normal should point from sphere toward AABB (negative X)
        assert!(normal.x < 0.0);
    }

    #[test]
    fn test_sphere_aabb_sphere_inside() {
        // Sphere center is inside the AABB
        let result = collide_sphere_aabb(
            &Vector3::new(0.0, 0.0, 0.0),
            0.5,
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(2.0, 1.0, 3.0),
        );
        let (normal, penetration) = result.unwrap();
        // Least penetration is Y axis (1.0 to face), plus radius
        assert!(penetration > 0.0);
        assert!(approx_eq(normal.norm(), 1.0));
    }

    #[test]
    fn test_sphere_aabb_from_below() {
        // Sphere below AABB, overlapping on Y axis
        let result = collide_sphere_aabb(
            &Vector3::new(0.0, -1.5, 0.0),
            1.0,
            &Vector3::new(0.0, 0.0, 0.0),
            &Vector3::new(2.0, 1.0, 2.0),
        );
        let (normal, penetration) = result.unwrap();
        assert!(penetration > 0.0);
        // Normal should point from sphere toward AABB (positive Y)
        assert!(normal.y > 0.0);
    }

    // -- World-level collision tests --

    #[test]
    fn test_detect_collisions_no_colliders() {
        let mut world = PhysicsWorld::<4>::new();
        world.add_body(RigidBody::new(1.0)).unwrap();
        world.add_body(RigidBody::new(1.0)).unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 0);
    }

    #[test]
    fn test_detect_collisions_sphere_sphere() {
        let mut world = PhysicsWorld::<4>::new();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(1.5, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);
        assert!(approx_eq(contacts[0].penetration, 0.5));
    }

    #[test]
    fn test_detect_collisions_skips_static_pairs() {
        let mut world = PhysicsWorld::<4>::new();
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.5, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 0);
    }

    #[test]
    fn test_collision_response_separates_bodies() {
        let mut world = PhysicsWorld::<4>::new();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_restitution(0.0)
                    .with_damping(0.0),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(1.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_restitution(0.0)
                    .with_damping(0.0),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);

        world.resolve_contacts(&contacts);

        // After resolution, bodies should be separated (no overlap)
        let a = &world.body(BodyId(0)).unwrap().position;
        let b = &world.body(BodyId(1)).unwrap().position;
        let dist = (b - a).norm();
        assert!(dist >= 2.0 - EPSILON);
    }

    #[test]
    fn test_collision_response_bounce() {
        let mut world = PhysicsWorld::<4>::new();
        // Body A moving right
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_velocity(Vector3::new(5.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_restitution(1.0)
                    .with_damping(0.0),
            )
            .unwrap();
        // Body B stationary
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(1.5, 0.0, 0.0))
                    .with_velocity(Vector3::zeros())
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_restitution(1.0)
                    .with_damping(0.0),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        world.resolve_contacts(&contacts);

        let vel_a = world.body(BodyId(0)).unwrap().velocity;
        let vel_b = world.body(BodyId(1)).unwrap().velocity;

        // With equal masses and restitution=1.0 (perfectly elastic):
        // A should stop, B should take A's velocity
        assert!(approx_eq(vel_a.x, 0.0));
        assert!(approx_eq(vel_b.x, 5.0));
    }

    #[test]
    fn test_collision_dynamic_vs_static() {
        let mut world = PhysicsWorld::<4>::new();
        // Dynamic ball falling into static floor
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.5, 0.0))
                    .with_velocity(Vector3::new(0.0, -5.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_restitution(0.5)
                    .with_damping(0.0),
            )
            .unwrap();
        // Static floor
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, -1.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(10.0, 1.0, 10.0),
                    })
                    .with_restitution(1.0),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);

        world.resolve_contacts(&contacts);

        // Ball should bounce upward
        let ball = world.body(BodyId(0)).unwrap();
        assert!(ball.velocity.y > 0.0);

        // Static floor should not move
        let floor = world.body(BodyId(1)).unwrap();
        assert!(approx_vec_eq(&floor.velocity, &Vector3::zeros()));
        assert!(approx_eq(floor.position.y, -1.0));
    }

    #[test]
    fn test_step_with_collisions() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        // Ball above a static floor
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 2.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 0.5 })
                    .with_restitution(0.5)
                    .with_damping(0.0),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(10.0, 0.1, 10.0),
                    }),
            )
            .unwrap();

        // Run for several steps — ball should not fall through the floor
        for _ in 0..600 {
            world.step::<4>(1.0 / 60.0);
        }

        let ball = world.body(BodyId(0)).unwrap();
        // Ball center should be above the floor surface (floor top at y=0.1, ball radius=0.5)
        assert!(ball.position.y >= 0.5);
    }

    #[test]
    fn test_collider_builder() {
        let body = RigidBody::new(1.0)
            .with_collider(Collider::Sphere { radius: 2.0 });
        assert_eq!(body.collider, Some(Collider::Sphere { radius: 2.0 }));

        let body2 = RigidBody::new(1.0)
            .with_collider(Collider::Aabb {
                half_extents: Vector3::new(1.0, 2.0, 3.0),
            });
        assert!(matches!(body2.collider, Some(Collider::Aabb { .. })));
    }

    #[test]
    fn test_no_collider_body_ignored() {
        let mut world = PhysicsWorld::<4>::new();
        // Body without collider
        world.add_body(RigidBody::new(1.0)).unwrap();
        // Body with collider overlapping the first
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_collider(Collider::Sphere { radius: 100.0 }),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 0); // No collision because first body has no collider
    }

    #[test]
    fn test_aabb_sphere_collision_via_world() {
        let mut world = PhysicsWorld::<4>::new();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(1.0, 1.0, 1.0),
                    }),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(1.5, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);
        assert!(contacts[0].penetration > 0.0);
    }

    // -- Phase 3: Friction tests --

    #[test]
    fn test_friction_builder() {
        let body = RigidBody::new(1.0).with_friction(0.7);
        assert!(approx_eq(body.friction, 0.7));
    }

    #[test]
    fn test_friction_clamped() {
        let body = RigidBody::new(1.0).with_friction(2.0);
        assert!(approx_eq(body.friction, 1.0));
        let body2 = RigidBody::new(1.0).with_friction(-0.5);
        assert!(approx_eq(body2.friction, 0.0));
    }

    #[test]
    fn test_default_friction_values() {
        let dynamic = RigidBody::new(1.0);
        assert!(approx_eq(dynamic.friction, 0.3));
        let static_body = RigidBody::new_static();
        assert!(approx_eq(static_body.friction, 0.5));
    }

    #[test]
    fn test_friction_reduces_tangential_velocity() {
        // A ball sliding along a static floor should slow down due to friction
        let mut world = PhysicsWorld::<4>::new();

        // Ball moving horizontally, resting on floor
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.55, 0.0))
                    .with_velocity(Vector3::new(10.0, -0.1, 0.0))
                    .with_collider(Collider::Sphere { radius: 0.5 })
                    .with_restitution(0.0)
                    .with_friction(0.8)
                    .with_damping(0.0),
            )
            .unwrap();
        // Static floor
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(50.0, 0.1, 50.0),
                    })
                    .with_friction(0.8),
            )
            .unwrap();

        let initial_vx = 10.0_f32;

        // Step and detect collision
        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);
        world.resolve_contacts(&contacts);

        let ball = world.body(BodyId(0)).unwrap();
        // Tangential (X) velocity should be reduced by friction
        assert!(ball.velocity.x < initial_vx);
        assert!(ball.velocity.x >= 0.0); // Should not reverse
    }

    #[test]
    fn test_zero_friction_preserves_tangential_velocity() {
        let mut world = PhysicsWorld::<4>::new();

        // Ball sliding on frictionless surface
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.55, 0.0))
                    .with_velocity(Vector3::new(10.0, -1.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 0.5 })
                    .with_restitution(0.0)
                    .with_friction(0.0)
                    .with_damping(0.0),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(50.0, 0.1, 50.0),
                    })
                    .with_friction(0.0),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        world.resolve_contacts(&contacts);

        let ball = world.body(BodyId(0)).unwrap();
        // With zero friction, tangential velocity should be preserved
        assert!(approx_eq(ball.velocity.x, 10.0));
    }

    #[test]
    fn test_high_friction_vs_low_friction() {
        // Compare two identical scenarios with different friction values
        let make_world = |friction: f32| -> PhysicsWorld<4> {
            let mut world = PhysicsWorld::<4>::new();
            world
                .add_body(
                    RigidBody::new(1.0)
                        .with_position(Vector3::new(0.0, 0.55, 0.0))
                        .with_velocity(Vector3::new(10.0, -1.0, 0.0))
                        .with_collider(Collider::Sphere { radius: 0.5 })
                        .with_restitution(0.0)
                        .with_friction(friction)
                        .with_damping(0.0),
                )
                .unwrap();
            world
                .add_body(
                    RigidBody::new_static()
                        .with_position(Vector3::new(0.0, 0.0, 0.0))
                        .with_collider(Collider::Aabb {
                            half_extents: Vector3::new(50.0, 0.1, 50.0),
                        })
                        .with_friction(friction),
                )
                .unwrap();
            world
        };

        let mut world_low = make_world(0.1);
        let mut world_high = make_world(1.0);

        let contacts_low = world_low.detect_collisions::<4>();
        world_low.resolve_contacts(&contacts_low);
        let contacts_high = world_high.detect_collisions::<4>();
        world_high.resolve_contacts(&contacts_high);

        let vx_low = world_low.body(BodyId(0)).unwrap().velocity.x;
        let vx_high = world_high.body(BodyId(0)).unwrap().velocity.x;

        // High friction should slow tangential velocity more
        assert!(vx_high < vx_low);
    }

    // -- Phase 3: Body lifecycle tests --

    #[test]
    fn test_body_active_by_default() {
        let body = RigidBody::new(1.0);
        assert!(body.active);
        let static_body = RigidBody::new_static();
        assert!(static_body.active);
    }

    #[test]
    fn test_remove_body() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(RigidBody::new(1.0).with_velocity(Vector3::new(5.0, 0.0, 0.0))).unwrap();

        assert_eq!(world.active_body_count(), 1);
        assert!(world.remove_body(id));
        assert_eq!(world.active_body_count(), 0);
        assert_eq!(world.body_count(), 1); // Still in the world, just inactive

        // Body velocity should be zeroed
        let body = world.body(id).unwrap();
        assert!(!body.active);
        assert!(approx_vec_eq(&body.velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_remove_body_twice_returns_false() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(RigidBody::new(1.0)).unwrap();

        assert!(world.remove_body(id));
        assert!(!world.remove_body(id)); // Already inactive
    }

    #[test]
    fn test_remove_body_invalid_id() {
        let mut world = PhysicsWorld::<4>::new();
        assert!(!world.remove_body(BodyId(99)));
    }

    #[test]
    fn test_inactive_body_not_integrated() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let id = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 10.0, 0.0))
                .with_damping(0.0),
        ).unwrap();

        world.remove_body(id);
        world.step::<4>(1.0);

        // Position should not change since body is inactive
        let body = world.body(id).unwrap();
        assert!(approx_eq(body.position.y, 10.0));
    }

    #[test]
    fn test_inactive_body_not_collided() {
        let mut world = PhysicsWorld::<4>::new();

        // Two overlapping bodies
        let id_a = world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::zeros())
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.5, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 }),
            )
            .unwrap();

        // Should detect collision when both active
        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);

        // Deactivate first body — no more collisions
        world.remove_body(id_a);
        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 0);
    }

    #[test]
    fn test_set_active_reactivates_body() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let id = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 10.0, 0.0))
                .with_damping(0.0),
        ).unwrap();

        // Deactivate, step (should not move)
        world.remove_body(id);
        world.step::<4>(1.0);
        assert!(approx_eq(world.body(id).unwrap().position.y, 10.0));

        // Reactivate, step (should fall)
        world.set_active(id, true);
        assert_eq!(world.active_body_count(), 1);
        world.step::<4>(1.0);
        assert!(world.body(id).unwrap().position.y < 10.0);
    }

    #[test]
    fn test_active_body_count() {
        let mut world = PhysicsWorld::<8>::new();
        let id0 = world.add_body(RigidBody::new(1.0)).unwrap();
        let id1 = world.add_body(RigidBody::new(1.0)).unwrap();
        let _id2 = world.add_body(RigidBody::new(1.0)).unwrap();

        assert_eq!(world.active_body_count(), 3);

        world.remove_body(id0);
        assert_eq!(world.active_body_count(), 2);

        world.remove_body(id1);
        assert_eq!(world.active_body_count(), 1);
    }

    #[test]
    fn test_remove_preserves_other_body_ids() {
        let mut world = PhysicsWorld::<4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let id0 = world.add_body(RigidBody::new(1.0).with_position(Vector3::new(0.0, 10.0, 0.0)).with_damping(0.0)).unwrap();
        let id1 = world.add_body(RigidBody::new(1.0).with_position(Vector3::new(5.0, 10.0, 0.0)).with_damping(0.0)).unwrap();

        // Remove first, step
        world.remove_body(id0);
        world.step::<4>(1.0);

        // First body should not have moved, second should have fallen
        assert!(approx_eq(world.body(id0).unwrap().position.y, 10.0));
        assert!(world.body(id1).unwrap().position.y < 10.0);
    }

    // -- Phase 4: Angular dynamics tests --

    #[test]
    fn test_default_orientation_is_identity() {
        let body = RigidBody::new(1.0);
        assert!(approx_eq(body.orientation.w, 1.0));
        assert!(approx_vec_eq(&body.angular_velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_angular_velocity_builder() {
        let body = RigidBody::new(1.0)
            .with_angular_velocity(Vector3::new(0.0, 5.0, 0.0));
        assert!(approx_eq(body.angular_velocity.y, 5.0));
    }

    #[test]
    fn test_angular_damping_builder() {
        let body = RigidBody::new(1.0).with_angular_damping(0.05);
        assert!(approx_eq(body.angular_damping, 0.05));
    }

    #[test]
    fn test_angular_damping_clamped() {
        let body = RigidBody::new(1.0).with_angular_damping(2.0);
        assert!(approx_eq(body.angular_damping, 1.0));
        let body2 = RigidBody::new(1.0).with_angular_damping(-1.0);
        assert!(approx_eq(body2.angular_damping, 0.0));
    }

    #[test]
    fn test_inertia_sphere() {
        let body = RigidBody::new(2.0).with_inertia_sphere(0.5);
        // I = 2/5 * 2 * 0.25 = 0.2, inv = 5.0
        let inv_i = body.inv_inertia_local[(0, 0)];
        assert!(approx_eq(inv_i, 5.0));
        // Should be diagonal and uniform
        assert!(approx_eq(body.inv_inertia_local[(1, 1)], 5.0));
        assert!(approx_eq(body.inv_inertia_local[(2, 2)], 5.0));
        assert!(approx_eq(body.inv_inertia_local[(0, 1)], 0.0));
    }

    #[test]
    fn test_inertia_box() {
        let body = RigidBody::new(12.0)
            .with_inertia_box(Vector3::new(1.0, 2.0, 3.0));
        // Full dims: 2x4x6
        // Ixx = (12/12) * (16 + 36) = 52, inv = 1/52
        // Iyy = (12/12) * (4 + 36) = 40, inv = 1/40
        // Izz = (12/12) * (4 + 16) = 20, inv = 1/20
        assert!(approx_eq(body.inv_inertia_local[(0, 0)], 1.0 / 52.0));
        assert!(approx_eq(body.inv_inertia_local[(1, 1)], 1.0 / 40.0));
        assert!(approx_eq(body.inv_inertia_local[(2, 2)], 1.0 / 20.0));
    }

    #[test]
    fn test_inertia_static_body_unchanged() {
        let body = RigidBody::new_static().with_inertia_sphere(1.0);
        // Static bodies should keep zero inverse inertia
        assert!(approx_eq(body.inv_inertia_local[(0, 0)], 0.0));
    }

    #[test]
    fn test_apply_torque() {
        let mut body = RigidBody::new(1.0);
        body.apply_torque(Vector3::new(1.0, 0.0, 0.0));
        body.apply_torque(Vector3::new(0.0, 2.0, 0.0));
        assert!(approx_vec_eq(&body.torque_accumulator, &Vector3::new(1.0, 2.0, 0.0)));
    }

    #[test]
    fn test_apply_angular_impulse() {
        let mut body = RigidBody::new(1.0).with_inertia_sphere(1.0);
        // I = 2/5 * 1 * 1 = 0.4, inv = 2.5
        body.apply_angular_impulse(Vector3::new(1.0, 0.0, 0.0));
        // delta_omega = inv_I * impulse = 2.5 * 1.0 = 2.5
        assert!(approx_eq(body.angular_velocity.x, 2.5));
    }

    #[test]
    fn test_angular_impulse_on_static_body_ignored() {
        let mut body = RigidBody::new_static();
        body.apply_angular_impulse(Vector3::new(100.0, 0.0, 0.0));
        assert!(approx_vec_eq(&body.angular_velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_constant_angular_velocity_changes_orientation() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_angular_velocity(Vector3::new(0.0, core::f32::consts::PI, 0.0))
                .with_angular_damping(0.0)
                .with_damping(0.0),
        ).unwrap();

        // After 1 second at PI rad/s around Y, orientation should have rotated ~180 degrees
        for _ in 0..100 {
            world.step::<4>(0.01);
        }

        let body = world.body(id).unwrap();
        // Check quaternion is no longer identity
        assert!(body.orientation.w.abs() < 0.9); // Rotated significantly
        // Angular velocity should be preserved (no damping)
        assert!(approx_eq(body.angular_velocity.y, core::f32::consts::PI));
    }

    #[test]
    fn test_torque_produces_angular_acceleration() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_inertia_sphere(1.0)
                .with_angular_damping(0.0)
                .with_damping(0.0),
        ).unwrap();

        // Apply torque around Z axis
        world.body_mut(id).unwrap().apply_torque(Vector3::new(0.0, 0.0, 1.0));
        world.step::<4>(1.0);

        let body = world.body(id).unwrap();
        // I = 0.4, alpha = torque * inv_I = 1.0 * 2.5 = 2.5
        // After 1s: omega = 2.5 rad/s
        assert!(approx_eq(body.angular_velocity.z, 2.5));
    }

    #[test]
    fn test_torque_cleared_after_step() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_inertia_sphere(1.0)
                .with_angular_damping(0.0)
                .with_damping(0.0),
        ).unwrap();

        world.body_mut(id).unwrap().apply_torque(Vector3::new(0.0, 0.0, 1.0));
        world.step::<4>(1.0);

        // No new torque — angular velocity should stay constant
        let omega_before = world.body(id).unwrap().angular_velocity.z;
        world.step::<4>(1.0);
        let omega_after = world.body(id).unwrap().angular_velocity.z;
        assert!(approx_eq(omega_before, omega_after));
    }

    #[test]
    fn test_angular_damping_reduces_spin() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_angular_velocity(Vector3::new(10.0, 0.0, 0.0))
                .with_angular_damping(0.1)
                .with_damping(0.0),
        ).unwrap();

        world.step::<4>(1.0);

        let body = world.body(id).unwrap();
        assert!(body.angular_velocity.x < 10.0);
        assert!(body.angular_velocity.x > 0.0);
    }

    #[test]
    fn test_orientation_stays_normalized() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_angular_velocity(Vector3::new(3.0, 5.0, 7.0))
                .with_angular_damping(0.0)
                .with_damping(0.0),
        ).unwrap();

        // Many steps to check quaternion doesn't drift
        for _ in 0..1000 {
            world.step::<4>(0.01);
        }

        let body = world.body(id).unwrap();
        let q = body.orientation.into_inner();
        let norm = (q.w * q.w + q.i * q.i + q.j * q.j + q.k * q.k).sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_collision_imparts_angular_velocity() {
        // A ball hitting a static floor off-center should start spinning
        let mut world = PhysicsWorld::<4>::new();

        // Ball with some horizontal velocity hitting a floor
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.55, 0.0))
                    .with_velocity(Vector3::new(5.0, -1.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 0.5 })
                    .with_inertia_sphere(0.5)
                    .with_restitution(0.0)
                    .with_friction(0.8)
                    .with_damping(0.0)
                    .with_angular_damping(0.0),
            )
            .unwrap();

        // Static floor
        world
            .add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_collider(Collider::Aabb {
                        half_extents: Vector3::new(50.0, 0.1, 50.0),
                    })
                    .with_friction(0.8),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        assert_eq!(contacts.len(), 1);
        world.resolve_contacts(&contacts);

        let ball = world.body(BodyId(0)).unwrap();
        // Friction at the contact should have induced angular velocity (spinning)
        let omega_magnitude = ball.angular_velocity.norm();
        assert!(omega_magnitude > 0.01, "Expected angular velocity from friction, got {}", omega_magnitude);
    }

    #[test]
    fn test_remove_body_clears_angular_state() {
        let mut world = PhysicsWorld::<4>::new();
        let id = world.add_body(
            RigidBody::new(1.0)
                .with_angular_velocity(Vector3::new(5.0, 5.0, 5.0)),
        ).unwrap();

        world.remove_body(id);
        let body = world.body(id).unwrap();
        assert!(approx_vec_eq(&body.angular_velocity, &Vector3::zeros()));
    }

    #[test]
    fn test_inv_inertia_world_rotated() {
        // For a non-uniform inertia tensor, rotating the body should change
        // the world-space inverse inertia
        let mut body = RigidBody::new(1.0)
            .with_inertia_box(Vector3::new(1.0, 2.0, 3.0));

        let inv_i_local_00 = body.inv_inertia_local[(0, 0)];
        let inv_i_world_00 = body.inv_inertia_world()[(0, 0)];
        // At identity orientation, local = world
        assert!(approx_eq(inv_i_local_00, inv_i_world_00));

        // Rotate 90 degrees around Y
        body.orientation = UnitQuaternion::from_axis_angle(
            &nalgebra::Unit::new_normalize(Vector3::new(0.0, 1.0, 0.0)),
            core::f32::consts::FRAC_PI_2,
        );
        let inv_i_rotated = body.inv_inertia_world();
        // After 90-degree Y rotation, Ixx and Izz should swap
        assert!(approx_eq(inv_i_rotated[(0, 0)], body.inv_inertia_local[(2, 2)]));
        assert!(approx_eq(inv_i_rotated[(2, 2)], body.inv_inertia_local[(0, 0)]));
    }

    #[test]
    fn test_elastic_collision_angular_conservation() {
        // For a head-on elastic collision between equal spheres,
        // total angular momentum should be conserved (both start at zero)
        let mut world = PhysicsWorld::<4>::new();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(0.0, 0.0, 0.0))
                    .with_velocity(Vector3::new(5.0, 0.0, 0.0))
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_inertia_sphere(1.0)
                    .with_restitution(1.0)
                    .with_friction(0.0)
                    .with_damping(0.0)
                    .with_angular_damping(0.0),
            )
            .unwrap();
        world
            .add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(1.5, 0.0, 0.0))
                    .with_velocity(Vector3::zeros())
                    .with_collider(Collider::Sphere { radius: 1.0 })
                    .with_inertia_sphere(1.0)
                    .with_restitution(1.0)
                    .with_friction(0.0)
                    .with_damping(0.0)
                    .with_angular_damping(0.0),
            )
            .unwrap();

        let contacts = world.detect_collisions::<4>();
        world.resolve_contacts(&contacts);

        let omega_a = world.body(BodyId(0)).unwrap().angular_velocity;
        let omega_b = world.body(BodyId(1)).unwrap().angular_velocity;
        // With zero friction and head-on collision, no angular velocity should be generated
        assert!(omega_a.norm() < EPSILON);
        assert!(omega_b.norm() < EPSILON);
    }

    // -- Phase 5: Constraint & joint tests --

    #[test]
    fn test_add_constraint() {
        let mut world = PhysicsWorld::<4, 4>::new();
        let a = world.add_body(RigidBody::new(1.0)).unwrap();
        let b = world.add_body(RigidBody::new(1.0)).unwrap();

        let cid = world.add_constraint(Constraint::Distance {
            body_a: a,
            body_b: b,
            anchor_a: Vector3::zeros(),
            anchor_b: Vector3::zeros(),
            rest_length: 2.0,
            compliance: 0.0,
        }).unwrap();

        assert_eq!(world.constraint_count(), 1);
        assert!(world.constraint(cid).is_some());
    }

    #[test]
    fn test_add_constraint_at_capacity() {
        let mut world = PhysicsWorld::<4, 1>::new();
        let a = world.add_body(RigidBody::new(1.0)).unwrap();
        let b = world.add_body(RigidBody::new(1.0)).unwrap();

        let c = Constraint::BallSocket {
            body_a: a,
            body_b: b,
            anchor_a: Vector3::zeros(),
            anchor_b: Vector3::zeros(),
            compliance: 0.0,
        };
        assert!(world.add_constraint(c.clone()).is_some());
        assert!(world.add_constraint(c).is_none()); // At capacity
    }

    #[test]
    fn test_remove_constraint() {
        let mut world = PhysicsWorld::<4, 4>::new();
        let a = world.add_body(RigidBody::new(1.0)).unwrap();
        let b = world.add_body(RigidBody::new(1.0)).unwrap();

        let cid = world.add_ball_socket(a, Vector3::zeros(), b, Vector3::zeros(), 0.0).unwrap();
        assert_eq!(world.constraint_count(), 1);

        assert!(world.remove_constraint(cid));
        assert_eq!(world.constraint_count(), 0);
    }

    #[test]
    fn test_remove_constraint_invalid_id() {
        let mut world = PhysicsWorld::<4, 4>::new();
        assert!(!world.remove_constraint(ConstraintId(99)));
    }

    #[test]
    fn test_distance_constraint_maintains_length() {
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        // Two bodies connected by a rigid rod of length 3
        let a = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 5.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();
        let b = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(3.0, 5.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        world.add_distance_constraint(
            a, Vector3::zeros(),
            b, Vector3::zeros(),
            0.0,
        ).unwrap();

        // Run simulation — gravity pulls both down, constraint keeps distance
        for _ in 0..200 {
            world.step::<4>(1.0 / 60.0);
        }

        let pa = world.body(a).unwrap().position;
        let pb = world.body(b).unwrap().position;
        let dist = (pb - pa).norm();
        // Distance should stay close to 3.0
        assert!((dist - 3.0).abs() < 0.3,
            "Expected distance ~3.0, got {}", dist);
    }

    #[test]
    fn test_ball_socket_keeps_points_together() {
        // Two bodies at the same position with a ball-socket joint (zero anchors)
        // should stay together under gravity
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));
        world.solver_iterations = 8;

        let a = world.add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, 10.0, 0.0)),
        ).unwrap();

        let b = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 10.0, 0.0))
                .with_damping(0.01)
                .with_angular_damping(0.01),
        ).unwrap();

        // Ball-socket: pin body B's center to body A's center
        world.add_ball_socket(
            a, Vector3::zeros(),
            b, Vector3::zeros(),
            0.0,
        ).unwrap();

        for _ in 0..120 {
            world.step::<4>(1.0 / 60.0);
        }

        // Body B should stay close to A (gravity pulls it down but joint holds)
        let pa = world.body(a).unwrap().position;
        let pb = world.body(b).unwrap().position;
        let gap = (pb - pa).norm();
        assert!(gap < 1.0, "Ball-socket gap too large: {}", gap);
    }

    #[test]
    fn test_pendulum_swings_under_gravity() {
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let anchor = world.add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, 10.0, 0.0)),
        ).unwrap();

        let bob = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(3.0, 10.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        // Distance constraint (rod of length 3)
        world.add_distance_constraint(
            anchor, Vector3::zeros(),
            bob, Vector3::zeros(),
            0.0,
        ).unwrap();

        // Bob starts at same height as anchor, should swing down
        let initial_y = 10.0;

        for _ in 0..120 {
            world.step::<4>(1.0 / 60.0);
        }

        let bob_pos = world.body(bob).unwrap().position;
        // Bob should have swung below the anchor
        assert!(bob_pos.y < initial_y, "Bob should swing down, y={}", bob_pos.y);
    }

    #[test]
    fn test_soft_distance_constraint() {
        // A soft spring should allow the bodies to overshoot the rest length
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::zeros());

        let a = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 0.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();
        let b = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(5.0, 0.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        // Soft spring with rest_length=2, compliance=0.01 (springy)
        world.add_constraint(Constraint::Distance {
            body_a: a,
            body_b: b,
            anchor_a: Vector3::zeros(),
            anchor_b: Vector3::zeros(),
            rest_length: 2.0,
            compliance: 0.01,
        }).unwrap();

        // Step a few times
        for _ in 0..60 {
            world.step::<4>(1.0 / 60.0);
        }

        let pa = world.body(a).unwrap().position;
        let pb = world.body(b).unwrap().position;
        let dist = (pb - pa).norm();
        // With compliance, it should have moved toward rest_length but may not be exact
        assert!(dist < 5.0, "Spring should have contracted, dist={}", dist);
    }

    #[test]
    fn test_fixed_joint_preserves_relative_orientation() {
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let a = world.add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, 10.0, 0.0)),
        ).unwrap();
        let b = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(2.0, 10.0, 0.0))
                .with_inertia_box(Vector3::new(0.5, 0.5, 0.5))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        // Fixed joint — should preserve relative orientation
        world.add_fixed_joint(
            a, Vector3::new(2.0, 0.0, 0.0),
            b, Vector3::zeros(),
            0.0,
        ).unwrap();

        // Store initial relative orientation
        let initial_qa = world.body(a).unwrap().orientation;
        let initial_qb = world.body(b).unwrap().orientation;
        let initial_rel = initial_qb * initial_qa.inverse();

        for _ in 0..200 {
            world.step::<4>(1.0 / 60.0);
        }

        // Relative orientation should be similar to initial
        let qa = world.body(a).unwrap().orientation;
        let qb = world.body(b).unwrap().orientation;
        let current_rel = qb * qa.inverse();
        let error = current_rel * initial_rel.inverse();
        let angle = error.angle();
        assert!(angle < 0.5, "Fixed joint orientation drift: {} rad", angle);
    }

    #[test]
    fn test_chain_of_distance_constraints() {
        // Three bodies connected by two distance constraints forming a chain
        let mut world = PhysicsWorld::<8, 8>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));
        world.solver_iterations = 8;

        let anchor = world.add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, 10.0, 0.0)),
        ).unwrap();
        let link1 = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 8.0, 0.0))
                .with_damping(0.01)
                .with_angular_damping(0.01),
        ).unwrap();
        let link2 = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 6.0, 0.0))
                .with_damping(0.01)
                .with_angular_damping(0.01),
        ).unwrap();

        world.add_distance_constraint(
            anchor, Vector3::zeros(),
            link1, Vector3::zeros(),
            0.0,
        ).unwrap();
        world.add_distance_constraint(
            link1, Vector3::zeros(),
            link2, Vector3::zeros(),
            0.0,
        ).unwrap();

        for _ in 0..600 {
            world.step::<4>(1.0 / 60.0);
        }

        // Verify chain lengths are approximately maintained
        let pa = world.body(anchor).unwrap().position;
        let p1 = world.body(link1).unwrap().position;
        let p2 = world.body(link2).unwrap().position;

        let d1 = (p1 - pa).norm();
        let d2 = (p2 - p1).norm();

        assert!((d1 - 2.0).abs() < 0.5, "Link 1 distance: {}", d1);
        assert!((d2 - 2.0).abs() < 0.5, "Link 2 distance: {}", d2);

        // Everything should hang below the anchor
        assert!(p1.y < pa.y, "Link 1 should be below anchor");
        assert!(p2.y < p1.y, "Link 2 should be below link 1");
    }

    #[test]
    fn test_constraint_between_equal_bodies_symmetric() {
        // Two equal-mass bodies connected by a distance constraint in zero gravity
        // should move symmetrically toward each other
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::zeros());

        let a = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(-3.0, 0.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();
        let b = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(3.0, 0.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        // Distance constraint with rest_length=2 (bodies start 6 apart)
        world.add_constraint(Constraint::Distance {
            body_a: a,
            body_b: b,
            anchor_a: Vector3::zeros(),
            anchor_b: Vector3::zeros(),
            rest_length: 2.0,
            compliance: 0.0,
        }).unwrap();

        for _ in 0..120 {
            world.step::<4>(1.0 / 60.0);
        }

        let pa = world.body(a).unwrap().position;
        let pb = world.body(b).unwrap().position;

        // Center of mass should remain at origin (symmetric motion)
        let com = (pa + pb) / 2.0;
        assert!(com.norm() < 0.5, "COM should stay near origin, got {:?}", com);
    }

    #[test]
    fn test_constraint_with_static_body() {
        // Constraint between static and dynamic body — only dynamic should move
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::zeros());

        let fixed = world.add_body(
            RigidBody::new_static()
                .with_position(Vector3::new(0.0, 0.0, 0.0)),
        ).unwrap();
        let dynamic = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(5.0, 0.0, 0.0))
                .with_damping(0.0)
                .with_angular_damping(0.0),
        ).unwrap();

        // Distance constraint rest_length=2
        world.add_constraint(Constraint::Distance {
            body_a: fixed,
            body_b: dynamic,
            anchor_a: Vector3::zeros(),
            anchor_b: Vector3::zeros(),
            rest_length: 2.0,
            compliance: 0.0,
        }).unwrap();

        for _ in 0..120 {
            world.step::<4>(1.0 / 60.0);
        }

        // Static body should not move
        let p_fixed = world.body(fixed).unwrap().position;
        assert!(approx_vec_eq(&p_fixed, &Vector3::zeros()));

        // Dynamic body should have moved toward rest_length distance
        let p_dyn = world.body(dynamic).unwrap().position;
        let dist = p_dyn.norm();
        assert!((dist - 2.0).abs() < 0.5, "Expected ~2.0, got {}", dist);
    }

    #[test]
    fn test_solver_iterations_affect_convergence() {
        // More solver iterations should give tighter constraint enforcement
        let make_world = |iterations: u32| -> PhysicsWorld<4, 4> {
            let mut world = PhysicsWorld::<4, 4>::new();
            world.set_gravity(Vector3::new(0.0, -10.0, 0.0));
            world.solver_iterations = iterations;

            let a = world.add_body(
                RigidBody::new_static()
                    .with_position(Vector3::new(0.0, 10.0, 0.0)),
            ).unwrap();
            let b = world.add_body(
                RigidBody::new(1.0)
                    .with_position(Vector3::new(3.0, 10.0, 0.0))
                    .with_damping(0.0)
                    .with_angular_damping(0.0),
            ).unwrap();

            world.add_distance_constraint(
                a, Vector3::zeros(),
                b, Vector3::zeros(),
                0.0,
            ).unwrap();
            world
        };

        let mut world_low = make_world(1);
        let mut world_high = make_world(16);

        for _ in 0..120 {
            world_low.step::<4>(1.0 / 60.0);
            world_high.step::<4>(1.0 / 60.0);
        }

        let pa_low = world_low.body(BodyId(0)).unwrap().position;
        let pb_low = world_low.body(BodyId(1)).unwrap().position;
        let error_low = ((pb_low - pa_low).norm() - 3.0).abs();

        let pa_high = world_high.body(BodyId(0)).unwrap().position;
        let pb_high = world_high.body(BodyId(1)).unwrap().position;
        let error_high = ((pb_high - pa_high).norm() - 3.0).abs();

        // Higher iterations should give less error (or equal)
        assert!(error_high <= error_low + EPSILON,
            "High iterations error {} should be <= low iterations error {}", error_high, error_low);
    }

    #[test]
    fn test_no_constraints_no_effect() {
        // World with no constraints should behave identically
        let mut world = PhysicsWorld::<4, 4>::new();
        world.set_gravity(Vector3::new(0.0, -10.0, 0.0));

        let id = world.add_body(
            RigidBody::new(1.0)
                .with_position(Vector3::new(0.0, 10.0, 0.0))
                .with_damping(0.0),
        ).unwrap();

        world.step::<4>(1.0);

        let body = world.body(id).unwrap();
        assert!(approx_eq(body.velocity.y, -10.0));
    }

    #[test]
    fn test_add_distance_constraint_helper() {
        let mut world = PhysicsWorld::<4, 4>::new();
        let a = world.add_body(
            RigidBody::new(1.0).with_position(Vector3::new(0.0, 0.0, 0.0)),
        ).unwrap();
        let b = world.add_body(
            RigidBody::new(1.0).with_position(Vector3::new(5.0, 0.0, 0.0)),
        ).unwrap();

        let cid = world.add_distance_constraint(
            a, Vector3::zeros(),
            b, Vector3::zeros(),
            0.0,
        ).unwrap();

        // Rest length should be auto-computed from positions
        if let Constraint::Distance { rest_length, .. } = world.constraint(cid).unwrap() {
            assert!(approx_eq(*rest_length, 5.0));
        } else {
            panic!("Expected Distance constraint");
        }
    }
}
