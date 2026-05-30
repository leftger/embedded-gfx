# embedded-3dgfx Examples

This directory contains 31 interactive examples demonstrating the full capabilities of embedded-3dgfx: 3D rendering, retro/BSP workflows, physics, skeletal animation, and soft body dynamics.

## Prerequisites

These examples use `embedded-graphics-simulator` for desktop visualization. SDL2 is required:

### macOS
```bash
brew install sdl2
```

### Ubuntu/Debian
```bash
sudo apt-get install libsdl2-dev
```

### Windows
Download SDL2 development libraries from [libsdl.org](https://www.libsdl.org/download-2.0.php)

## Running Examples

All examples require the `std` feature:

```bash
cargo run --example <name> --features std
```

## 🎨 3D Rendering Examples

### Core Rendering

#### `basic_rendering` - Render Mode Cycling
Demonstrates three fundamental render modes: Points, Lines, and Solid.

**Controls:**
- `SPACE` - Cycle through render modes
- `ESC` - Exit

```bash
cargo run --example basic_rendering --features std
```

#### `boot_menu` - Boot Splash & Menu Transitions (96×64)
Bitmap logo (textured billboard), 5 menu items, fade transitions, keyframed intro + slide-in.

**Controls:**
- `SPACE` - Skip boot, go to menu
- `UP` / `DOWN` - Change selection (5 items)
- `R` - Restart full demo
- `ESC` - Exit

```bash
cargo run --example boot_menu --features std
```

For firmware, use `default-features = false` and `features = ["row_width_96"]`.

#### `rotating_cube` - Animated 3D Rotation
Continuously rotating cube with FPS counter.

**Features:**
- Time-based transformations
- Real-time FPS display
- Smooth multi-axis rotation

```bash
cargo run --example rotating_cube --features std
```

#### `scene_viewer` - Interactive Multi-Mesh Scene
Complex scene with multiple objects and camera controls.

**Controls:**
- `Arrow Keys` - Rotate camera
- `+/-` - Zoom
- `ESC` - Exit

```bash
cargo run --example scene_viewer --features std
```

### Lighting & Shading

#### `lighting_demo` - Directional Lighting
Shows diffuse lighting with ambient term on rotating objects.

**Controls:**
- `SPACE` - Toggle light auto-rotation
- `Arrow Keys` - Manual light control
- `ESC` - Exit

```bash
cargo run --example lighting_demo --features std
```

#### `gouraud_demo` - Smooth Color Interpolation
Demonstrates Gouraud shading with per-vertex colors.

```bash
cargo run --example gouraud_demo --features std
```

#### `blinn_phong_demo` - Specular Highlights
Shows specular reflections using Blinn-Phong shading.

**Features:**
- Specular highlights
- Shininess control
- Multiple STL models

```bash
cargo run --example blinn_phong_demo --features std
```

### Visual Effects

#### `fog_dithering_demo` - Atmospheric Effects
Interactive fog and dithering effects.

**Controls:**
- `F` - Toggle fog
- `D` - Toggle dithering
- `+/-` - Adjust fog distance
- `ESC` - Exit

```bash
cargo run --example fog_dithering_demo --features std
```

#### `billboard_demo` - Camera-Facing Sprites
Demonstrates billboarding for particles and sprites.

```bash
cargo run --example billboard_demo --features std
```

### Advanced Features

#### `texture_mapping_demo` - UV Mapping
Shows affine texture mapping with multiple patterns.

**Features:**
- Multiple texture patterns
- UV coordinate mapping
- Texture switching

```bash
cargo run --example texture_mapping_demo --features std
```

#### `retro_presets_demo` - Retro Visual Preset Toggle
Compares Doom-like, PSX-like, and modern visual styles on the same scene.

**Controls:**
- `1/2/3` - Switch Doom / PSX / Modern preset
- `SPACE` - Toggle auto-rotation
- `ESC` - Exit

```bash
cargo run --example retro_presets_demo --features std
```

#### `bsp_builder_demo` - BSP Room Strip Builder
Builds a small BSP world from room specs at startup and renders it with textured BSP traversal.

**Controls:**
- `W/S` - Move forward/back
- `A/D` - Turn left/right
- `1/2/3` - Switch Doom / PSX / Modern preset
- `4` - Toggle checkerboard stipple
- `ESC` - Exit

```bash
cargo run --example bsp_builder_demo --features std
```

#### `dma_rendering_demo` - Double Buffering
Performance comparison between single and double buffering.

**Features:**
- ~30% FPS improvement demonstration
- Swap chain visualization

```bash
cargo run --example dma_rendering_demo --features std
```

#### `lod_demo` - Level of Detail
Distance-based mesh detail switching.

**Features:**
- 3-10× performance improvement at distance
- Smooth LOD transitions

```bash
cargo run --example lod_demo --features std
```

#### `vertex_animation_demo` - Keyframe Animation
Animated models using keyframe interpolation.

```bash
cargo run --example vertex_animation_demo --features std
```

#### `stl_viewer` - STL Model Viewer
Load and view STL models (Suzanne, teapot, Blåhaj).

**Controls:**
- `1/2/3` - Switch models
- `R` - Rotate model

```bash
cargo run --example stl_viewer --features std
```

#### `painters_algorithm_demo` - Z-buffer-free Rendering
Back-to-front triangle sorting without Z-buffer (saves 1.92MB RAM).

**Controls:**
- `SPACE` - Toggle Painter's Algorithm vs Z-buffer
- `R` - Rotate objects

```bash
cargo run --example painters_algorithm_demo --features std
```

## ⚛️ Physics Examples

*See [PHYSICS_EXAMPLES.md](../PHYSICS_EXAMPLES.md) for detailed physics documentation.*

### Rigid Body Dynamics

#### `physics_rolling_ball` ⭐ **START HERE**
Beginner-friendly introduction to physics.

**Features:**
- Gravity simulation
- Friction and rolling motion
- Static ramp

**Controls:**
- `R` - Reset ball
- `SPACE` - Apply impulse
- `F` - Toggle friction

```bash
cargo run --example physics_rolling_ball --features std
```

#### `physics_bouncing_balls` - Multi-Body Collisions
Multiple spheres colliding with each other and the ground.

```bash
cargo run --example physics_bouncing_balls --features std
```

#### `physics_pendulum` - Constraint Joints
Swinging pendulum using distance constraints.

```bash
cargo run --example physics_pendulum --features std
```

#### `physics_newtons_cradle` - Chain Constraints
Classic Newton's cradle with multiple distance constraints.

```bash
cargo run --example physics_newtons_cradle --features std
```

#### `physics_stack_tower` - Stacking & Friction
Boxes stacking with realistic friction.

```bash
cargo run --example physics_stack_tower --features std
```

#### `physics_domino_chain` - Sequential Collisions
Domino chain reaction simulation.

```bash
cargo run --example physics_domino_chain --features std
```

#### `physics_wrecking_ball` - Demolition
Wrecking ball knocking down a wall.

```bash
cargo run --example physics_wrecking_ball --features std
```

#### `physics_demo` - Comprehensive Showcase
Original comprehensive physics demo showing all features together.

```bash
cargo run --example physics_demo --features std
```

#### `capsule_physics_demo` - Capsule Colliders
Capsule colliders interacting with spheres and AABBs.

**Features:**
- Capsule-sphere collisions
- Capsule-AABB collisions
- Capsule-capsule collisions
- Realistic tumbling

**Controls:**
- `SPACE` - Spawn capsule
- `C` - Add cube
- `S` - Add sphere
- `R` - Reset

```bash
cargo run --example capsule_physics_demo --features std
```

#### `raycast_demo` - Ray Casting
Interactive ray casting with visual feedback.

**Features:**
- Ray-sphere intersection
- Ray-AABB intersection
- Ray-capsule intersection
- Hit highlighting

**Controls:**
- `SPACE` - Shoot ray
- `Arrow Keys` - Rotate camera
- `R` - Reset

```bash
cargo run --example raycast_demo --features std
```

## 🦴 Skeletal Animation Examples

*See [SKELETAL_AND_SOFTBODY.md](../SKELETAL_AND_SOFTBODY.md) for detailed documentation.*

#### `skeletal_animation_demo` - Bone Hierarchy & Skinning
Animated articulated arm with hierarchical bones.

**Features:**
- Parent-child bone relationships
- Linear blend skinning (SSD)
- Real-time deformation
- Up to 4 bone influences per vertex

**Controls:**
- `SPACE` - Toggle animation
- `UP/DOWN` - Rotate shoulder
- `LEFT/RIGHT` - Rotate elbow
- `R` - Reset pose
- `ESC` - Exit

```bash
cargo run --example skeletal_animation_demo --features std
```

## 🧊 Soft Body Physics Examples

*See [SKELETAL_AND_SOFTBODY.md](../SKELETAL_AND_SOFTBODY.md) for detailed documentation.*

#### `cloth_simulation` - Mass-Spring Cloth
Hanging cloth with wind interaction.

**Features:**
- Mass-spring network
- Structural and shear springs
- Pinned constraints
- Wind forces
- Gravity toggle

**Controls:**
- `W` - Wind right
- `S` - Wind left
- `G` - Toggle gravity
- `R` - Reset
- `ESC` - Exit

```bash
cargo run --example cloth_simulation --features std
```

#### `jelly_cube_demo` - Deformable Cube
Bouncing soft body with volume preservation.

**Features:**
- 3D mass-spring network
- Pressure/volume preservation
- Ground collision
- Bouncy behavior

**Controls:**
- `SPACE` - Drop cube
- `UP` - Apply upward force
- `P` - Toggle pressure
- `R` - Reset
- `ESC` - Exit

```bash
cargo run --example jelly_cube_demo --features std
```

## 🎯 Example Categories

### By Difficulty

**Beginner:**
1. `rotating_cube` - Basic 3D rendering
2. `basic_rendering` - Render modes
3. `physics_rolling_ball` - Physics intro

**Intermediate:**
4. `lighting_demo` - Lighting basics
5. `scene_viewer` - Camera control
6. `physics_bouncing_balls` - Multi-body physics
7. `billboard_demo` - Special effects

**Advanced:**
8. `skeletal_animation_demo` - Character animation
9. `cloth_simulation` - Soft body dynamics
10. `raycast_demo` - Ray casting
11. `physics_newtons_cradle` - Complex constraints

### By Feature

**Rendering:**
- `basic_rendering`, `rotating_cube`, `scene_viewer`
- `lighting_demo`, `gouraud_demo`, `blinn_phong_demo`
- `fog_dithering_demo`, `texture_mapping_demo`
- `retro_presets_demo`, `bsp_builder_demo`

**Physics:**
- `physics_rolling_ball`, `physics_bouncing_balls`
- `physics_pendulum`, `physics_newtons_cradle`
- `physics_stack_tower`, `physics_domino_chain`
- `physics_wrecking_ball`, `physics_demo`
- `capsule_physics_demo`, `raycast_demo`

**Animation:**
- `vertex_animation_demo` - Keyframe animation
- `skeletal_animation_demo` - Bone-based animation
- `cloth_simulation` - Physics-based animation
- `jelly_cube_demo` - Soft body animation

**Performance:**
- `dma_rendering_demo` - Double buffering
- `lod_demo` - Level of detail
- `painters_algorithm_demo` - Memory optimization

## 🔧 Customizing Examples

### Changing Resolution

Edit the example source:
```rust
// Change from 320×240 to 640×480
let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(640, 480));
let mut engine = K3dengine::new(640, 480);

// Adjust window scale
let output_settings = OutputSettingsBuilder::new().scale(1).build(); // Was .scale(2)
```

### Adjusting Performance

```rust
// Reduce quality for higher FPS
engine.camera.set_near_far(2.0, 20.0);  // Narrower depth range

// Use LOD for distant objects
mesh.set_lod(Some(medium_detail), Some(low_detail), lod_levels);

// Disable expensive features
// Remove fog, dithering, or use simpler shading
```

## 🚀 Performance Tips

1. **Use LOD** - 3-10× fewer triangles at distance
2. **Enable DMA** - 30% FPS improvement
3. **Tune Z-buffer** - Better depth precision
4. **Frustum culling** - Automatic 50-200% speedup
5. **Backface culling** - Eliminates ~50% of faces

## 📊 Example Performance

On desktop (Apple M1):
- Simple scenes: 1000+ FPS
- Complex scenes (500+ triangles): 200-400 FPS
- Physics (16 bodies): 60 FPS
- Soft body (64 particles): 60 FPS

On embedded (STM32 Cortex-M33 @ 100MHz):
- Simple scenes: 60-120 FPS
- Medium scenes (100 triangles): 30-60 FPS
- Physics (8 bodies): 30-60 FPS

## 🎓 Learning Path

**Recommended order for learning:**

1. **Rendering Basics**
   - `rotating_cube` → `basic_rendering` → `scene_viewer`

2. **Lighting & Effects**
   - `lighting_demo` → `gouraud_demo` → `fog_dithering_demo`

3. **Physics Fundamentals**
   - `physics_rolling_ball` → `physics_bouncing_balls` → `physics_pendulum`

4. **Advanced Physics**
   - `physics_newtons_cradle` → `capsule_physics_demo` → `raycast_demo`

5. **Animation & Soft Body**
   - `skeletal_animation_demo` → `cloth_simulation` → `jelly_cube_demo`

## 🐛 Troubleshooting

**SDL2 not found:**
```
error: failed to run custom build command for `sdl2-sys`
```
→ Install SDL2 (see Prerequisites above)

**Feature errors:**
```
error: could not find `perfcounter` in `embedded_3dgfx`
```
→ Add `--features std` to cargo run command

**Window crashes:**
```
called `Option::unwrap()` on a `None` value
```
→ Ensure SDL2 is properly installed and display is available

## 📝 Notes

- All examples target 60 FPS (16ms frame time)
- Physics examples use fixed timestep (0.016s)
- Soft body examples may run slower on some systems
- Examples are for demonstration - optimize for production use
- Window close button works - press ESC or close window to exit

## 🔗 Related Documentation

- **[../PHYSICS_EXAMPLES.md](../PHYSICS_EXAMPLES.md)** - Detailed physics demo guide
- **[../SKELETAL_AND_SOFTBODY.md](../SKELETAL_AND_SOFTBODY.md)** - Animation & soft body docs
- **[../README.md](../README.md)** - Main library documentation
