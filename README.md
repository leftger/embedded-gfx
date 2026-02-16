# Embedded-3dgfx
<a href="https://crates.io/crates/embedded-3dgfx"><img alt="crates.io" src="https://img.shields.io/crates/v/embedded-3dgfx"></a>
<a href="https://github.com/leftger/embedded-3dgfx/actions"><img alt="actions" src="https://github.com/leftger/embedded-3dgfx/actions/workflows/rust.yml/badge.svg"></a>

A complete `no_std` 3D graphics and physics engine for embedded systems. Features real-time rendering, rigid body dynamics, soft body physics, skeletal animation, and advanced visual effects optimized for resource-constrained devices.

> **Note**: This is a fork of [embedded-gfx](https://github.com/Kezii/embedded-gfx) by [Kezii](https://github.com/Kezii). This fork adds texture mapping, fog/dithering effects, DMA rendering, Z-buffer improvements, and a complete physics engine.

## 🌟 Key Features

### 🎨 3D Graphics Rendering
- **Complete MVP Pipeline** - Model-View-Projection with perspective projection
- **Z-buffering** - Hardware-style depth testing with tunable epsilon
- **Advanced Shading** - Flat, Gouraud, directional lighting, Blinn-Phong specular
- **Texture Mapping** - Affine UV mapping with multi-texture support
- **Visual Effects** - Fog, dithering, billboards, vertex animation
- **LOD System** - Distance-based mesh detail switching
- **Performance** - Frustum culling, backface culling, DMA rendering

### ⚛️ Physics Engine
- **Rigid Body Dynamics** - Linear/angular motion, forces, torques
- **Collision Detection** - Sphere, AABB, and Capsule colliders
- **Collision Response** - Impulse-based with friction and restitution
- **Constraints** - Distance, ball-socket, and fixed joints
- **Ray Casting** - Line-of-sight, shooting, object selection
- **Body Management** - Activation/deactivation, lifecycle control

### 🦴 Skeletal Animation
- **Bone Hierarchy** - Parent-child skeletal structures
- **Linear Blend Skinning** - Smooth vertex deformation (SSD)
- **Multi-Bone Influence** - Up to 4 bones per vertex
- **Real-time Animation** - Interactive character animation

### 🧊 Soft Body Physics
- **Mass-Spring Systems** - Deformable objects (cloth, jelly, soft bodies)
- **Pressure Preservation** - Volume maintenance for enclosed bodies
- **Collision Response** - Ground collision with friction
- **Pre-built Shapes** - Cloth grids, jelly cubes, soft spheres

## 🚀 Quick Start

### Installation

```toml
[dependencies]
# For embedded (no_std)
embedded-3dgfx = { version = "0.1", default-features = false }

# For desktop/simulator with all features
embedded-3dgfx = { version = "0.1", features = ["std"] }
```

### Basic Example

```rust
use embedded_3dgfx::{K3dengine, mesh::{Geometry, K3dMesh, RenderMode}};
use nalgebra::Vector3;

// Create engine
let mut engine = K3dengine::new(320, 240);
engine.camera.set_position(Vector3::new(0.0, 0.0, 5.0).into());

// Create a cube mesh
let geometry = Geometry { vertices: &CUBE_VERTS, faces: &CUBE_FACES, /* ... */ };
let mut mesh = K3dMesh::new(geometry);
mesh.set_render_mode(RenderMode::Lines);

// Render
engine.render(std::iter::once(&mesh), |primitive| {
    draw(primitive, &mut display);
});
```

### Physics Example

```rust
use embedded_3dgfx::physics::{PhysicsWorld, RigidBody, Collider, Ray};
use nalgebra::Vector3;

// Create physics world
let mut world = PhysicsWorld::<16, 4>::new();
world.set_gravity(Vector3::new(0.0, -9.81, 0.0));

// Add a falling sphere
let sphere = RigidBody::new(1.0)
    .with_collider(Collider::Sphere { radius: 0.5 })
    .with_position(Vector3::new(0.0, 10.0, 0.0));
world.add_body(sphere);

// Add static ground
let ground = RigidBody::new_static()
    .with_collider(Collider::Aabb { half_extents: Vector3::new(10.0, 0.5, 10.0) });
world.add_body(ground);

// Simulate
world.step::<8>(0.016);

// Ray cast
let ray = Ray::new(Vector3::zeros(), Vector3::new(0.0, -1.0, 0.0));
if let Some(hit) = world.ray_cast(&ray, 100.0) {
    println!("Hit at distance: {}", hit.distance);
}
```

## 🎮 Examples

The project includes 23+ interactive examples. Run with:

```bash
cargo run --example <name> --features std
```

### 3D Rendering Examples

**Core Rendering:**
- `basic_rendering` - Render mode cycling (Points, Lines, Solid)
- `rotating_cube` - Animated 3D transformations
- `scene_viewer` - Interactive multi-mesh scene

**Lighting & Effects:**
- `lighting_demo` - Directional lighting with ambient
- `gouraud_demo` - Smooth color interpolation
- `blinn_phong_demo` - Specular highlights
- `fog_dithering_demo` - Atmospheric effects

**Advanced Features:**
- `texture_mapping_demo` - UV-mapped textures
- `dma_rendering_demo` - Double-buffer performance
- `billboard_demo` - Camera-facing quads
- `lod_demo` - Level of detail switching
- `vertex_animation_demo` - Keyframe animation
- `stl_viewer` - Load and view STL models

### Physics Examples

**Rigid Body Dynamics:** (See [PHYSICS_EXAMPLES.md](PHYSICS_EXAMPLES.md))
- `physics_rolling_ball` ⭐ **START HERE** - Beginner-friendly intro
- `physics_bouncing_balls` - Multiple colliding spheres
- `physics_pendulum` - Swinging pendulum with constraints
- `physics_newtons_cradle` - Newton's cradle with chain constraints
- `physics_stack_tower` - Stacking boxes with friction
- `physics_domino_chain` - Domino effect simulation
- `physics_wrecking_ball` - Wrecking ball demolition
- `physics_demo` - Comprehensive physics showcase
- `capsule_physics_demo` - Capsule collider interactions

**Advanced Physics:** (See [SKELETAL_AND_SOFTBODY.md](SKELETAL_AND_SOFTBODY.md))
- `skeletal_animation_demo` - Bone hierarchy and skinning
- `cloth_simulation` - Hanging cloth with wind
- `jelly_cube_demo` - Soft deformable cube
- `raycast_demo` - Ray casting and hit detection

## 📚 Feature Flags

### `std` (includes `perfcounter`)
- Enables Painter's Algorithm (uses `std::vec::Vec`)
- Performance counter with std timing
- Required for desktop simulator examples

### `perfcounter` (optional)
- Enables FPS/timing measurements
- Requires timing source: `std` or `embassy-time`
- Automatically included with `std`

### `embassy-time` (optional)
- Timing for embedded targets using Embassy framework
- Use with `perfcounter` for embedded performance monitoring

### `row_width_*` (mutually exclusive)
- `row_width_160` - Optimize for 160px wide displays
- `row_width_240` - Optimize for 240px wide displays (default)
- `row_width_320` - Optimize for 320px wide displays

## 🎯 Core Rendering Features

### Rendering Pipeline
- **Full MVP Transforms** - Model-View-Projection with perspective
- **Z-buffering** - 16.16 fixed-point depth testing
- **Backface Culling** - ~50% face elimination
- **Frustum Culling** - 50-200% speedup for off-screen objects
- **Optimized Rasterization** - Integer-only triangle filling

### Shading & Lighting
- **Flat Shading** - Per-face solid colors
- **Gouraud Shading** - Smooth vertex color interpolation
- **Directional Lighting** - Diffuse with ambient term
- **Blinn-Phong** - Specular highlights with half-vector
- **Pre-computed Constants** - Per-mesh optimization

### Visual Effects
- **Fog** - Depth-based atmospheric effects
- **Dithering** - 4×4 Bayer matrix ordered dithering
- **Billboards** - Camera-facing sprites
- **Vertex Animation** - Keyframe interpolation

### Texturing
- **Affine Mapping** - UV-based texture coordinates
- **Multi-texture** - Multiple textures per scene
- **Power-of-2 Optimization** - Fast wrapping with bit masks
- **RGB565 Format** - Memory-efficient 16-bit color

### Performance Systems
- **DMA Rendering** - ~30% FPS boost with double buffering
- **LOD** - 3-10× triangle reduction at distance
- **Swap Chain** - Asynchronous buffer transfers
- **Inline Optimization** - Aggressive hot-path inlining

## ⚛️ Physics System

### Rigid Body Dynamics
```rust
let body = RigidBody::new(1.0)
    .with_collider(Collider::Capsule { height: 2.0, radius: 0.5 })
    .with_position(Vector3::new(0.0, 5.0, 0.0))
    .with_restitution(0.6)  // Bounciness
    .with_friction(0.5);     // Surface friction
```

### Collision Detection
- **Sphere Collider** - Cheapest, perfect for balls and particles
- **AABB Collider** - Boxes, platforms, walls (axis-aligned)
- **Capsule Collider** - Best for characters (efficient + smooth)
- **Broad Phase** - O(n²) narrow-phase only (spatial partitioning planned)

### Constraints & Joints
```rust
// Distance constraint (rope, rod)
world.add_constraint(Constraint::Distance {
    body_a: ball_id,
    body_b: anchor_id,
    rest_length: 2.0,
    compliance: 0.0,  // Stiffness
});

// Ball-socket joint (ragdoll)
world.add_constraint(Constraint::BallSocket { /* ... */ });

// Fixed joint (welding)
world.add_constraint(Constraint::Fixed { /* ... */ });
```

### Ray Casting
```rust
let ray = Ray::new(origin, direction);
if let Some(hit) = world.ray_cast(&ray, 100.0) {
    println!("Hit body {:?} at {}", hit.body_id, hit.distance);
    println!("Point: {:?}, Normal: {:?}", hit.point, hit.normal);
}
```

## 🦴 Skeletal Animation

```rust
let mut skeleton = Skeleton::<8>::new();
let root = skeleton.add_bone(Bone::new("root"), None).unwrap();
let arm = skeleton.add_bone(
    Bone::new("arm").with_position(Vector3::new(0.0, 1.0, 0.0)),
    Some(root)
).unwrap();

skeleton.update_transforms();
skeleton.compute_inverse_bind_poses();

// Animate
skeleton.get_bone_mut(arm).unwrap().set_rotation(rotation);
skeleton.update_transforms();

// Apply to mesh
apply_skinning(&skeleton, &skinning_data, &bind_vertices, &mut deformed);
```

## 🧊 Soft Body Physics

```rust
// Create cloth (8×8 grid)
let mut cloth = SoftBody::<64, 256>::create_cloth(8, 8, 0.4, 200.0, 1.0).unwrap();
cloth.set_gravity(Vector3::new(0.0, -9.81, 0.0));

// Create jelly cube (3×3×3 particles)
let mut jelly = SoftBody::<64, 256>::create_jelly_cube(3, 0.5, 150.0, 0.5).unwrap();
jelly.pressure_config.enabled = true;  // Volume preservation

// Simulate
cloth.step(0.016);
jelly.step(0.016);

// Get deformed vertices for rendering
cloth.get_vertex_positions(&mut vertex_buffer);
```

## 📋 System Requirements

### Embedded Targets
**Minimum:**
- ARM Cortex-M4F (with FPU)
- 128KB RAM (single-buffer + small scenes)
- 256KB+ Flash

**Recommended:**
- ARM Cortex-M33 with FPU (e.g., STM32WBA)
- 512KB+ RAM (double-buffer + Z-buffer + physics)
- 512KB+ Flash

### Performance Budget (240×135 @ 60 FPS)
- Rendering: ~10-13ms per frame
- Physics (16 bodies): ~2-3ms per frame
- Display transfer (DMA): ~3ms (parallel)
- **Total: 60+ FPS achievable**

### Memory Usage
| Feature | Memory Cost |
|---------|-------------|
| Single framebuffer (240×135) | 65 KB |
| Double framebuffer (240×135) | 130 KB |
| Z-buffer (240×135) | 130 KB |
| Physics (16 bodies) | ~4 KB |
| Soft body (64 particles) | ~2 KB |
| Skeleton (8 bones) | ~1 KB |

**Total typical usage:** 200-300 KB RAM

## 🧪 Testing

```bash
# Run all 182 tests
cargo test

# Run library tests only
cargo test --lib

# Test with all features
cargo test --lib --all-features
```

**Test Coverage:**
- ✅ 182 passing tests
- ✅ Rendering pipeline (transformations, clipping, rasterization)
- ✅ Physics (collisions, constraints, integration)
- ✅ Skeletal animation (skinning, bone hierarchy)
- ✅ Soft body (springs, particles, pressure)
- ✅ Ray casting (all collider types)

## 📦 no_std Compatibility

Fully `no_std` compatible by default. All core features work without the standard library.

### Feature Flag Combinations

**Pure Embedded (no_std):**
```toml
embedded-3dgfx = { version = "0.1", default-features = false }
```

**Embedded with Embassy Timing:**
```toml
embedded-3dgfx = { version = "0.1", default-features = false, features = ["perfcounter", "embassy-time"] }
```

**Desktop/Simulator:**
```toml
embedded-3dgfx = { version = "0.1", features = ["std"] }
```

**Custom Row Width:**
```toml
embedded-3dgfx = { version = "0.1", default-features = false, features = ["row_width_320"] }
```

## 🎯 Use Cases

### Game Development
- ✅ Character controllers (capsule colliders + ray casting)
- ✅ Projectile physics (rigid bodies + continuous motion)
- ✅ Skeletal animation (characters, creatures)
- ✅ Particle systems (billboards + soft bodies)
- ✅ Level geometry (static AABB colliders)

### Simulations
- ✅ Robotics visualization (skeletal arms, kinematics)
- ✅ Physics demonstrations (pendulum, Newton's cradle)
- ✅ Cloth/soft body (flags, deformable objects)
- ✅ Mechanical systems (constraints, joints)

### Embedded Displays
- ✅ STM32 with LCD displays
- ✅ ESP32 with TFT screens
- ✅ RP2040 with displays
- ✅ M5Stack devices

## 📚 Documentation

- **[PHYSICS_EXAMPLES.md](PHYSICS_EXAMPLES.md)** - Complete physics demo guide
- **[SKELETAL_AND_SOFTBODY.md](SKELETAL_AND_SOFTBODY.md)** - Animation & soft body documentation
- **[examples/README.md](examples/README.md)** - All examples with screenshots
- **API Docs** - `cargo doc --open`

## 🏗️ Architecture

```
embedded-3dgfx/
├── src/
│   ├── lib.rs              # Main engine & rendering
│   ├── camera.rs           # View/projection matrices
│   ├── mesh.rs             # Geometry & LOD
│   ├── draw.rs             # Rasterization & effects
│   ├── physics.rs          # Rigid body dynamics (3048 lines)
│   ├── skeleton.rs         # Skeletal animation (470 lines)
│   ├── softbody.rs         # Soft body physics (749 lines)
│   ├── texture.rs          # Texture management
│   ├── billboard.rs        # Camera-facing quads
│   ├── animation.rs        # Keyframe animation
│   ├── swapchain.rs        # DMA double-buffering
│   ├── perfcounter.rs      # Performance monitoring
│   └── painters.rs         # Painter's algorithm
│
├── examples/               # 23 interactive demos
│   ├── basic_rendering.rs
│   ├── rotating_cube.rs
│   ├── physics_*.rs        # 8 physics demos
│   ├── cloth_simulation.rs
│   ├── skeletal_animation_demo.rs
│   ├── raycast_demo.rs
│   └── ...
│
└── load_stl/              # STL file embedding macro
```

## 🔧 Performance Optimizations

Recent performance improvements (2026):
- ✅ **Interpolator-based rasterization** - Faster triangle filling
- ✅ **FnvIndexSet edge deduplication** - Faster mesh processing
- ✅ **Configurable row buffers** - Memory/speed trade-off
- ✅ **Inline hot paths** - Better compiler optimization
- ✅ **mem::swap optimizations** - Reduced allocations

**Measured improvements:**
- Triangle rasterization: ~20% faster
- Edge generation: ~30% faster
- Memory usage: Configurable (160-320px row buffers)

## 🎓 Learning Resources

Start with these examples in order:

1. **`rotating_cube`** - Learn basic 3D rendering
2. **`physics_rolling_ball`** ⭐ - Understand physics basics
3. **`lighting_demo`** - Explore shading
4. **`skeletal_animation_demo`** - See advanced animation
5. **`cloth_simulation`** - Experience soft body physics

## 🤝 Contributing

Contributions welcome! Priority areas:
- Hardware-specific display backends (ESP32, STM32, RP2040)
- Spatial partitioning (octree/grid for broad-phase)
- More joint types (hinge, slider, prismatic)
- Perspective-correct texture mapping
- Character controller system
- Mesh colliders (convex hulls)

## 🌟 Production Use

Working embedded example: [Rust on M5Stack Cardputer](https://github.com/Kezii/Rust-M5Stack-Cardputer)

## 📖 References

**Graphics:**
- [Tricks of the 3D Game Programming Gurus](https://www.amazon.com/Tricks-Game-Programming-Gurus-Advanced/dp/0672318350)
- [Michael Abrash's Graphics Programming Black Book](https://github.com/jagregory/abrash-black-book)
- [PSX Graphics Programming](https://psx-spx.consoledev.net/graphicsprocessingunitgpu/)

**Physics:**
- [Game Physics Engine Development](https://www.routledge.com/Game-Physics-Engine-Development/Millington/p/book/9780123819765)
- [Real-Time Collision Detection](https://www.amazon.com/Real-Time-Collision-Detection-Interactive-Technology/dp/1558607323)
- [Game Programming Gems](https://www.amazon.com/Game-Programming-Gems/dp/1584500492) (Physics sections)

## 📜 License

MIT OR Apache-2.0 - See LICENSE file for details.

---

**Built with ❤️ for the embedded Rust community**
