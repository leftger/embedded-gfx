# Skeletal Animation and Soft Body Physics

This document describes the skeletal subspace deformation (skinning) and soft body physics systems.

## Skeletal Animation System

Located in `src/skeleton.rs`, provides hierarchical bone structures for character animation.

### Key Features

- **Hierarchical Bones**: Parent-child bone relationships with transform propagation
- **Linear Blend Skinning**: Smooth vertex deformation based on bone influences
- **Multi-Bone Influences**: Up to 4 bones can influence each vertex
- **Inverse Bind Pose**: Proper skinning math for deformation
- **Normal Skinning**: Correctly transforms normals to maintain surface orientation

### Basic Usage

```rust
use embedded_3dgfx::skeleton::{Skeleton, Bone, SkinningData, VertexSkinning, apply_skinning};
use nalgebra::Vector3;

// Create skeleton
let mut skeleton = Skeleton::<8>::new();

// Add bones hierarchically
let root = skeleton.add_bone(Bone::new("root"), None).unwrap();
let child = skeleton.add_bone(
    Bone::new("arm").with_position(Vector3::new(0.0, 1.0, 0.0)),
    Some(root)
).unwrap();

// Set up bind pose
skeleton.update_transforms();
skeleton.compute_inverse_bind_poses();

// Create skinning data (per vertex)
let mut skinning_data = SkinningData::new();
skinning_data.add_vertex(VertexSkinning::two_bones(root.0, 0.7, child.0, 0.3)).unwrap();

// Animate bones
skeleton.get_bone_mut(child).unwrap().set_rotation(rotation);
skeleton.update_transforms();

// Apply skinning to deform mesh
apply_skinning(&skeleton, &skinning_data, &bind_vertices, &mut deformed_vertices);
```

### Example Demos

- **skeletal_animation_demo.rs**: Animated articulated arm showing bone hierarchy and smooth deformation

## Soft Body Physics

Located in `src/softbody.rs`, provides deformable object simulation using mass-spring systems.

### Key Features

- **Particle System**: Each soft body is composed of point masses
- **Spring Network**: Structural, shear, and bend springs for realistic deformation
- **Pressure Preservation**: Optional volume/pressure maintenance for enclosed bodies
- **Collision Response**: Ground plane collision with friction and restitution
- **Helper Constructors**: Pre-built configurations for cloth, jelly cubes, and soft spheres
- **No-std Compatible**: Uses heapless collections

### Basic Usage

```rust
use embedded_3dgfx::softbody::{SoftBody, Particle};
use nalgebra::Vector3;

// Create empty soft body
let mut soft_body = SoftBody::<64, 128>::new();

// Add particles
let p0 = soft_body.add_particle(Particle::new(Vector3::new(0.0, 0.0, 0.0), 1.0)).unwrap();
let p1 = soft_body.add_particle(Particle::new(Vector3::new(1.0, 0.0, 0.0), 1.0)).unwrap();

// Connect with spring
soft_body.add_spring(p0, p1, 1.0, 100.0, 0.5).unwrap(); // rest_length, stiffness, damping

// Pin first particle
soft_body.get_particle_mut(p0).unwrap().pinned = true;

// Simulate
soft_body.step(0.016); // 60 FPS

// Get positions for rendering
let mut vertices = vec![[0.0f32; 3]; soft_body.particles.len()];
soft_body.get_vertex_positions(&mut vertices);
```

### Pre-built Shapes

#### Cloth
```rust
let cloth = SoftBody::<64, 256>::create_cloth(
    width,      // particles along X
    height,     // particles along Y
    spacing,    // distance between particles
    stiffness,  // spring stiffness
    damping     // spring damping
).unwrap();
```

Creates a cloth grid pinned at the top edge with structural and shear springs.

#### Jelly Cube
```rust
let jelly = SoftBody::<64, 256>::create_jelly_cube(
    size,       // particles per axis (NxNxN grid)
    spacing,    // particle spacing
    stiffness,  // spring stiffness
    damping     // spring damping
).unwrap();
```

Creates a 3D deformable cube with pressure preservation for volume maintenance.

#### Soft Sphere
```rust
let ball = SoftBody::<64, 256>::create_soft_sphere(
    radius,        // sphere radius
    subdivisions,  // detail level (unused in current impl)
    stiffness,     // spring stiffness
    damping        // spring damping
).unwrap();
```

Creates a soft sphere with icosphere topology and pressure preservation.

### Configuration

```rust
// Gravity
soft_body.set_gravity(Vector3::new(0.0, -9.81, 0.0));

// Global damping (energy loss)
soft_body.damping = 0.99; // 0.0 = full damping, 1.0 = no damping

// Ground collision
soft_body.ground_plane = Some(0.0); // y = 0
soft_body.ground_restitution = 0.3; // bounciness
soft_body.ground_friction = 0.5;    // surface friction

// Pressure/volume preservation
soft_body.pressure_config.enabled = true;
soft_body.pressure_config.pressure_coefficient = 10.0;
```

### Example Demos

- **cloth_simulation.rs**: Hanging cloth with wind forces and gravity
- **jelly_cube_demo.rs**: Bouncing deformable cube demonstrating volume preservation

## Performance Considerations

### Skeletal Animation
- **O(bones × vertices × influences)** complexity for skinning
- Recommended: 4-8 bones, up to 512 vertices, max 4 influences per vertex
- Cache skinning matrices when possible
- Update transforms only when bones change

### Soft Body Physics
- **O(particles + springs)** per timestep
- Recommended: 32-64 particles, 128-256 springs for embedded systems
- Use damping to prevent instability
- Consider substepping for stiff springs
- Ground collision is O(particles)

## Integration with Rigid Body Physics

Soft bodies can interact with the existing rigid body physics system in `src/physics.rs`:

```rust
// Future integration (not yet implemented):
// - Soft body to rigid body collisions
// - Soft body tearing and fracture
// - Attachment points between soft and rigid bodies
```

## Running the Examples

```bash
# Skeletal animation
cargo run --example skeletal_animation_demo --features std

# Cloth simulation
cargo run --example cloth_simulation --features std

# Jelly cube
cargo run --example jelly_cube_demo --features std
```

## Technical Details

### Linear Blend Skinning (LBS)

For each vertex `v`, the deformed position `v'` is:

```
v' = Σ(w_i × M_i × v)
```

Where:
- `w_i` = weight for bone `i` (weights sum to 1.0)
- `M_i` = skinning matrix = `world_transform × inverse_bind_pose`
- Sum over all influencing bones

### Mass-Spring Dynamics

Each spring applies forces:

```
F_spring = k × (current_length - rest_length) × direction
F_damping = c × relative_velocity
F_total = F_spring + F_damping
```

Integration uses semi-implicit Euler:
```
v' = v + (F/m) × dt
x' = x + v' × dt
```

### Volume Preservation

Pressure forces push particles radially from center of mass:

```
F_pressure = pressure_coefficient × direction_from_center
```

This approximates incompressible soft bodies.
