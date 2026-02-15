# Physics Engine Examples

This document describes all the physics engine demonstration programs included in this project.

## Running the Examples

To run any physics demo, use:
```bash
cargo run --example <demo_name> --features std
```

For example:
```bash
cargo run --example physics_rolling_ball --features std
```

## Available Physics Demos

### 1. **physics_demo.rs** - Comprehensive Physics Showcase
The original comprehensive demo showing multiple physics features in one scene.

**Features demonstrated:**
- Falling cubes with gravity
- Angular dynamics (spinning cubes)
- Collision detection with impulse-based response
- Coulomb friction (different coefficients)
- Distance constraint joints (hanging chain)
- Body lifecycle management (activation/deactivation)
- Sphere and AABB colliders

**Controls:**
- `SPACE`: Apply upward impulse to all active cubes
- `T`: Apply random torque to spin cubes
- `D`: Deactivate/remove first active cube
- `R`: Reset positions and reactivate all
- `ESC`: Exit

---

### 2. **physics_rolling_ball.rs** - Beginner Introduction ⭐ START HERE
A simple, easy-to-understand demo perfect for learning the basics.

**Features demonstrated:**
- Basic rigid body dynamics
- Gravity simulation
- Friction coefficients and their effects
- Angular velocity from rolling motion
- Static tilted ramp

**Controls:**
- `SPACE`: Reset ball to top of ramp
- `1-3`: Change friction level (low/medium/high)
- `ESC`: Exit

**Why start here:** This is the simplest demo with clear visual feedback showing how friction affects motion.

---

### 3. **physics_bouncing_balls.rs** - Restitution Demonstration
Five balls with different "bounciness" (restitution coefficients).

**Features demonstrated:**
- Restitution coefficients (0.05 to 0.95)
- Elastic vs inelastic collisions
- Energy dissipation over time
- Visual comparison of material properties

**Controls:**
- `SPACE`: Drop all balls
- `R`: Reset positions
- `ESC`: Exit

**Educational value:** Clearly shows how restitution affects bounce height and energy conservation.

---

### 4. **physics_newtons_cradle.rs** - Momentum Conservation
Classic Newton's Cradle demonstrating conservation of momentum and energy.

**Features demonstrated:**
- Distance constraints (simulating strings)
- Nearly perfect elastic collisions (0.99 restitution)
- Momentum transfer through collision
- Multiple constraint solving

**Controls:**
- `1-5`: Pull back individual spheres
- `SPACE`: Release all pulled spheres
- `R`: Reset to neutral position
- `ESC`: Exit

**Physics principle:** Demonstrates conservation of momentum - pull back N spheres, N spheres swing out on the other side.

---

### 5. **physics_pendulum.rs** - Constraint-Based Motion
Multiple pendulums with different lengths and masses.

**Features demonstrated:**
- Distance constraints for swinging motion
- Pendulum period depends on length (not mass!)
- Angular dynamics in constrained systems
- Multiple independent constraints

**Controls:**
- `SPACE`: Apply impulse to all pendulums
- `1-3`: Pull back specific pendulum
- `R`: Reset to neutral
- `ESC`: Exit

**Physics principle:** Period of a pendulum depends on length, not mass - observe the different swing rates.

---

### 6. **physics_domino_chain.rs** - Chain Reaction
A line of dominoes demonstrating chain reaction physics.

**Features demonstrated:**
- Thin rectangular bodies (easy to tip)
- Angular momentum transfer through collision
- Chain reactions and cascading effects
- Tuned friction for realistic domino behavior

**Controls:**
- `SPACE`: Push first domino to start chain
- `R`: Reset all dominoes
- `ESC`: Exit

**Fun fact:** Watch how the angular impulse propagates through the chain as each domino hits the next.

---

### 7. **physics_stack_tower.rs** - Stability Testing
A tower of stacked boxes testing stability limits.

**Features demonstrated:**
- Stable stacking with friction
- Multiple simultaneous contacts
- Center of mass and balance
- Angular dynamics from off-center forces

**Controls:**
- `SPACE`: Apply impulse to middle box
- `LEFT/RIGHT`: Push tower horizontally
- `UP`: Lift top box
- `R`: Reset tower
- `ESC`: Exit

**Challenge:** How hard can you push before the tower falls?

---

### 8. **physics_wrecking_ball.rs** - Destructive Forces
A heavy wrecking ball swings to demolish a wall of boxes.

**Features demonstrated:**
- Mass differences (heavy ball vs light boxes)
- Constraint-based swinging motion
- Collision-based destruction
- Angular momentum in impacts
- Body deactivation

**Controls:**
- `SPACE`: Pull back and release wrecking ball
- `R`: Reset wall and ball
- `ESC`: Exit

**Satisfying physics:** Watch the heavy ball transfer momentum to light boxes, sending them flying!

---

## Physics Engine Features Summary

All demos showcase the following core physics engine capabilities:

### Collision Detection
- **Sphere collider**: Fast, simple, perfect for balls
- **AABB collider**: Axis-aligned bounding box for boxes and platforms
- Broad-phase and narrow-phase collision detection
- Penetration depth calculation

### Rigid Body Dynamics
- **Linear motion**: Position, velocity, acceleration
- **Angular motion**: Orientation (quaternions), angular velocity, torque
- **Forces**: Gravity, impulses, accumulated forces
- **Mass properties**: Mass, inverse mass, inertia tensor
- **Damping**: Linear and angular damping for stability

### Collision Response
- **Impulse-based resolution**: Physically accurate collision response
- **Restitution**: Bounciness coefficient (0.0 = inelastic, 1.0 = perfectly elastic)
- **Friction**: Coulomb friction model for realistic sliding/rolling
- **Solver iterations**: Configurable accuracy vs performance tradeoff

### Constraints
- **Distance constraints**: Fixed-distance connections (strings, ropes, chains)
- **Compliance**: Rigid (0.0) or soft constraints
- **Constraint solver**: Position-based dynamics (PBD) solver
- **Multiple constraints**: Systems with many interconnected bodies

### Body Lifecycle
- **Static bodies**: Infinite mass, unaffected by forces (floors, walls)
- **Dynamic bodies**: Full physics simulation
- **Activation/Deactivation**: Remove bodies from simulation
- **Reactivation**: Bring bodies back into simulation

### Advanced Features
- **Angular inertia**: Sphere and box inertia tensors for realistic rotation
- **Multiple contacts**: Handle many simultaneous collisions
- **Substeps**: Multiple physics steps per frame for stability
- **No heap allocations**: Fixed-size `heapless` collections for embedded systems

## Performance Tips

1. **Solver iterations**: More iterations = more accurate but slower
   - Simple scenes: 4-8 iterations
   - Complex constraints: 12-20 iterations

2. **Substeps**: Multiple substeps prevent tunneling
   - Fast-moving objects: 4-8 substeps
   - Slow motion: 1-2 substeps

3. **Contact limit**: Set based on scene complexity
   - Simple: 16 contacts
   - Medium: 32 contacts
   - Complex: 64-128 contacts

4. **Body/constraint capacity**: Set via const generics
   ```rust
   PhysicsWorld::<16, 8>::new()  // 16 bodies, 8 constraints
   ```

## Learning Path

Recommended order for understanding the physics engine:

1. **physics_rolling_ball** - Start with the basics
2. **physics_bouncing_balls** - Learn about restitution
3. **physics_stack_tower** - Understand friction and stability
4. **physics_pendulum** - Introduction to constraints
5. **physics_newtons_cradle** - Advanced constraints and momentum
6. **physics_domino_chain** - Chain reactions and angular dynamics
7. **physics_wrecking_ball** - Complex multi-body interactions
8. **physics_demo** - Everything combined!

## Common Patterns

### Creating a Dynamic Body
```rust
let body = RigidBody::new(mass)
    .with_position(Vector3::new(x, y, z))
    .with_velocity(Vector3::new(vx, vy, vz))
    .with_collider(Collider::Sphere { radius })
    .with_restitution(0.5)
    .with_friction(0.5)
    .with_inertia_sphere(radius);
let id = physics.add_body(body).unwrap();
```

### Creating a Static Body (Floor, Wall)
```rust
let floor = RigidBody::new_static()
    .with_position(Vector3::new(0.0, 0.0, 0.0))
    .with_collider(Collider::Aabb {
        half_extents: Vector3::new(10.0, 0.1, 10.0)
    })
    .with_friction(0.6);
physics.add_body(floor).unwrap();
```

### Adding a Distance Constraint
```rust
physics.add_distance_constraint(
    anchor_id,
    Vector3::zeros(),  // Anchor attachment point
    body_id,
    Vector3::zeros(),  // Body attachment point
    0.0,               // Compliance (0.0 = rigid)
).unwrap();
```

### Stepping Physics
```rust
let dt = 1.0 / 60.0;  // 60 FPS
physics.step_fixed::<MAX_CONTACTS>(dt, substeps);
```

### Syncing Physics to Visuals
```rust
for &body_id in &body_ids {
    let body = physics.body(body_id).unwrap();
    sync_body_to_mesh(body, &mut mesh);
}
```

## Troubleshooting

**Bodies fall through floor:**
- Increase solver iterations
- Add substeps
- Check collision types match (sphere-sphere, sphere-aabb, aabb-aabb)

**Constraints are loose/stretchy:**
- Increase solver iterations (try 20+)
- Reduce compliance (use 0.0 for rigid)
- Reduce physics timestep

**Simulation explodes:**
- Reduce timestep
- Add more damping (linear and angular)
- Check for extremely small or large masses
- Ensure inertia tensors are set correctly

**Performance issues:**
- Reduce max contacts
- Reduce solver iterations
- Use fewer substeps
- Simplify collider shapes (sphere is fastest)

## Technical Details

- **No std required**: Uses `heapless` for fixed-capacity collections
- **No heap allocations**: All memory pre-allocated at compile time
- **Const generics**: Capacity set at compile time for optimal performance
- **Quaternion rotations**: Stable, gimbal-lock-free 3D rotations
- **Inertia tensors**: Physically accurate rotational dynamics

## Contributing

When adding new physics examples:
1. Follow the naming convention: `physics_<name>.rs`
2. Include comprehensive doc comments at the top
3. List all controls clearly
4. Explain what physics principles are demonstrated
5. Keep it simple and focused on one or two concepts
6. Add entry to this document

Enjoy exploring physics simulation! 🎮⚡
