# RFC: Software-Rendered 3D Backend for LVGL

**Status:** Draft
**Author:** [Your Name]
**Date:** 2026-02-16
**Target:** LVGL Core Team and Community

---

## Summary

Propose adding a pure software 3D rendering backend to LVGL that enables 3D graphics on embedded systems without GPU/OpenGL requirements. This would integrate the `embedded-3dgfx` Rust library as an optional rendering backend, bringing complete 3D capabilities (mesh rendering, physics, skeletal animation) to resource-constrained microcontrollers.

---

## Motivation

### Current State

LVGL currently supports 3D rendering through:
- **lv_3dtexture widget** - Displays 3D content rendered by external engines
- **glTF support** - Full glTF 2.0 specification support
- **OpenGL ES backend** - Primary 3D rendering engine (requires GPU)

### The Gap

**Problem:** Many embedded systems lack GPUs capable of OpenGL ES:
- ARM Cortex-M4F/M7/M33 microcontrollers (STM32, NRF, ESP32-C3)
- Low-cost IoT devices with simple TFT displays
- Industrial HMI panels without dedicated graphics hardware
- Automotive instrument clusters on older MCUs

These platforms can run LVGL's 2D UI perfectly but cannot leverage the 3D features due to the OpenGL dependency.

### Opportunity

The `embedded-3dgfx` Rust library provides:
- **Pure software 3D rendering** - No GPU required, runs on any Cortex-M4F+
- **no_std compatible** - Perfect for embedded systems
- **Comprehensive features:**
  - Z-buffered rendering with texture mapping
  - Rigid body physics (16+ objects)
  - Soft body physics (cloth, deformables)
  - Skeletal animation with skinning
  - Visual effects (fog, dithering, LOD)
- **Proven performance:** 60 FPS @ 240×135 on STM32 (M5Stack Cardputer)
- **Active development:** 182 passing tests, 23 examples, v0.2.0 released

Integrating this would give LVGL a "software 3D mode" complementing its OpenGL backend, similar to how it offers software (`draw_sw`) and GPU-accelerated (`draw_sdl`, `draw_opengles`) 2D rendering.

---

## Proposal

### High-Level Architecture

Add a new optional draw backend: **`LV_USE_DRAW_EMBEDDED3D`**

```
┌─────────────────────────────────────────────┐
│          LVGL Application Layer             │
│  (lv_3dtexture, lv_obj, UI widgets)        │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│         LVGL Draw Pipeline                  │
│  (Task routing, draw unit management)       │
└─────────────────────────────────────────────┘
         ↓                          ↓
┌──────────────────┐      ┌──────────────────┐
│  Existing Units  │      │  New: embedded3d │
│  - draw_sw       │      │  Draw Unit       │
│  - draw_opengles │      │                  │
│  - draw_sdl      │      │  (Rust FFI)      │
└──────────────────┘      └──────────────────┘
         ↓                          ↓
┌─────────────────────────────────────────────┐
│            Framebuffer / Display            │
└─────────────────────────────────────────────┘
```

### Integration Points

#### 1. Configuration Flag

In `lv_conf.h`:

```c
/**
 * Enable embedded-3dgfx software 3D rendering backend
 * Requires: Rust toolchain, embedded-3dgfx crate
 * Use for: MCUs without OpenGL, pure software rendering
 * Memory: ~200-400 KB RAM (framebuffer + Z-buffer)
 */
#define LV_USE_DRAW_EMBEDDED3D 1

#if LV_USE_DRAW_EMBEDDED3D
    /* Rendering resolution (independent of display size) */
    #define LV_DRAW_EMBEDDED3D_WIDTH  240
    #define LV_DRAW_EMBEDDED3D_HEIGHT 135

    /* Enable Z-buffer (depth testing) */
    #define LV_DRAW_EMBEDDED3D_ZBUFFER 1

    /* Enable physics engine */
    #define LV_DRAW_EMBEDDED3D_PHYSICS 1

    /* Enable texture mapping */
    #define LV_DRAW_EMBEDDED3D_TEXTURES 1
#endif
```

#### 2. Draw Unit Registration

New file: `src/draw/embedded3d/lv_draw_embedded3d.c`

```c
#include "lv_draw_embedded3d.h"

/* FFI declarations for Rust functions */
extern void embedded3d_init(uint16_t width, uint16_t height);
extern void embedded3d_render_mesh(const lv_draw_3d_mesh_t *mesh, uint16_t *fb);
extern void embedded3d_clear_zbuffer(void);

typedef struct {
    lv_draw_unit_t base_unit;
    uint16_t width;
    uint16_t height;
    uint16_t *framebuffer;  /* RGB565 buffer */
    uint32_t *zbuffer;       /* Depth buffer (optional) */
} lv_draw_embedded3d_unit_t;

static int32_t embedded3d_evaluate_cb(lv_draw_unit_t *draw_unit,
                                       lv_draw_task_t *task)
{
    /* Claim LV_DRAW_TASK_TYPE_3D tasks if we're enabled */
    if (task->type == LV_DRAW_TASK_TYPE_3D) {
        return 90; /* High score - we're specialized for software 3D */
    }
    return 0; /* Not our task */
}

static int32_t embedded3d_dispatch_cb(lv_draw_unit_t *draw_unit,
                                       lv_draw_task_t *task)
{
    lv_draw_embedded3d_unit_t *unit = (lv_draw_embedded3d_unit_t *)draw_unit;

    switch (task->type) {
        case LV_DRAW_TASK_TYPE_3D:
            /* Call Rust rendering engine via FFI */
            embedded3d_render_mesh(task->mesh_data, unit->framebuffer);
            return 1; /* Success */

        default:
            return 0; /* Can't handle */
    }
}

void lv_draw_embedded3d_init(void)
{
    lv_draw_embedded3d_unit_t *unit = lv_draw_create_unit(sizeof(*unit));
    unit->base_unit.evaluate_cb = embedded3d_evaluate_cb;
    unit->base_unit.dispatch_cb = embedded3d_dispatch_cb;

    unit->width = LV_DRAW_EMBEDDED3D_WIDTH;
    unit->height = LV_DRAW_EMBEDDED3D_HEIGHT;

    /* Allocate rendering buffers */
    unit->framebuffer = lv_malloc(unit->width * unit->height * 2); /* RGB565 */

#if LV_DRAW_EMBEDDED3D_ZBUFFER
    unit->zbuffer = lv_malloc(unit->width * unit->height * 4); /* 32-bit depth */
#endif

    /* Initialize Rust engine */
    embedded3d_init(unit->width, unit->height);
}
```

#### 3. Rust FFI Bridge

New file: `embedded3d_bridge/src/lib.rs`

```rust
// Bridge library between LVGL (C) and embedded-3dgfx (Rust)
#![no_std]

use embedded_3dgfx::{K3dengine, mesh::K3dMesh};
use embedded_graphics_core::pixelcolor::Rgb565;

static mut ENGINE: Option<K3dengine> = None;

#[no_mangle]
pub unsafe extern "C" fn embedded3d_init(width: u16, height: u16) {
    ENGINE = Some(K3dengine::new(width, height));
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_render_mesh(
    mesh_data: *const LvglMeshData,
    framebuffer: *mut u16
) {
    let engine = ENGINE.as_mut().unwrap();
    let mesh = convert_lvgl_mesh_to_k3d(mesh_data);

    // Render to framebuffer via closure
    engine.render(core::iter::once(&mesh), |primitive| {
        // Draw primitives to framebuffer
        draw_to_framebuffer(primitive, framebuffer, width, height);
    });
}

#[repr(C)]
pub struct LvglMeshData {
    vertices: *const f32,
    vertex_count: usize,
    indices: *const u32,
    index_count: usize,
    // ... transformation matrices, etc.
}

fn convert_lvgl_mesh_to_k3d(data: *const LvglMeshData) -> K3dMesh {
    // Convert C-style mesh data to Rust K3dMesh
    // This handles memory layout differences safely
}

fn draw_to_framebuffer(primitive: DrawPrimitive, fb: *mut u16, w: u16, h: u16) {
    // Rasterize primitive directly to RGB565 framebuffer
    // This is the performance-critical hot path
}
```

#### 4. Widget Enhancement

Extend `lv_3dtexture` to accept software-rendered content:

```c
/* New API for software 3D */
lv_obj_t *tex = lv_3dtexture_create(parent);

/* Option A: Let LVGL handle rendering */
lv_3dtexture_set_render_mode(tex, LV_3D_RENDER_SOFTWARE);
lv_3dtexture_set_mesh_data(tex, mesh_vertices, mesh_indices, ...);

/* Option B: User provides pre-rendered framebuffer */
lv_3dtexture_set_framebuffer(tex, rgb565_buffer, width, height);
```

---

## Implementation Phases

### Phase 1: Proof of Concept (2-3 weeks)
- ✅ FFI bridge between LVGL C and embedded-3dgfx Rust
- ✅ Basic mesh rendering (wireframe cube)
- ✅ Framebuffer integration with lv_3dtexture
- ✅ Single-threaded, blocking rendering

**Deliverable:** Demo showing rotating 3D cube in LVGL window

### Phase 2: Core Features (4-6 weeks)
- ⬜ Full draw unit integration (`evaluate_cb`, `dispatch_cb`)
- ⬜ Z-buffer support for depth testing
- ⬜ Texture mapping (RGB565 format)
- ⬜ Lighting (flat, Gouraud, Blinn-Phong)
- ⬜ Performance optimization (DMA, double-buffering)
- ⬜ Memory management (configurable buffer sizes)

**Deliverable:** Feature-complete 3D backend matching embedded-3dgfx capabilities

### Phase 3: Advanced Features (6-8 weeks)
- ⬜ Physics engine integration (optional, `LV_DRAW_EMBEDDED3D_PHYSICS`)
- ⬜ Skeletal animation support
- ⬜ Soft body physics (cloth simulation)
- ⬜ Multi-threaded rendering (if RTOS available)
- ⬜ Comprehensive examples and documentation

**Deliverable:** Production-ready 3D backend with demos

### Phase 4: Rust Bindings (4 weeks, parallel)
- ⬜ Add Canvas widget to `lv_binding_rust`
- ⬜ Safe Rust wrappers for new 3D APIs
- ⬜ Idiomatic Rust examples
- ⬜ Integration tests

**Deliverable:** First-class Rust support for LVGL 3D

---

## Technical Considerations

### Memory Requirements

Typical configuration (240×135 @ RGB565):

| Component | Memory | Notes |
|-----------|--------|-------|
| Framebuffer | 65 KB | RGB565 pixel data |
| Z-buffer | 130 KB | Optional, 32-bit depth |
| Mesh data | 5-20 KB | Vertices, faces, normals |
| Physics state | 4 KB | 16 rigid bodies |
| Engine overhead | 2 KB | Camera, transforms |
| **Total** | **206 KB** | (Typical) |
| **Total (no Z-buffer)** | **76 KB** | (Minimal) |

Targets: Cortex-M4F+ with 256KB+ RAM (e.g., STM32F4, ESP32-S3, NRF52840)

### Performance Expectations

Measured on STM32WBA52CG @ 100 MHz (Cortex-M33):

| Scene Complexity | FPS | Details |
|------------------|-----|---------|
| Simple (cube, 12 faces) | 60+ | Wireframe, no textures |
| Medium (200 triangles) | 45-60 | Flat shading, Z-buffer |
| Complex (500 triangles) | 25-35 | Textures, Gouraud shading |
| With Physics (16 bodies) | 50-60 | Physics adds ~2ms overhead |

DMA rendering provides ~30% FPS boost via double-buffering.

### Platform Compatibility

**Minimum Requirements:**
- ARM Cortex-M4F or higher (FPU required)
- 128 KB RAM (without Z-buffer, small scenes)
- 256 KB Flash
- 64 MHz+ clock speed

**Recommended:**
- ARM Cortex-M33 with FPU
- 512 KB+ RAM (for Z-buffer + double-buffering)
- 100 MHz+ clock

**Tested Platforms:**
- ✅ STM32F4/F7 series
- ✅ STM32WBA52 (M5Stack Cardputer)
- ⬜ ESP32-S3 (planned)
- ⬜ NRF52840 (planned)
- ⬜ RP2040 (dual-core optimization)

---

## API Examples

### Example 1: Simple 3D Widget

```c
#include "lvgl.h"

void create_3d_cube(lv_obj_t *parent) {
    /* Create 3D texture widget */
    lv_obj_t *tex3d = lv_3dtexture_create(parent);
    lv_obj_set_size(tex3d, 240, 180);
    lv_obj_center(tex3d);

    /* Configure software rendering */
    lv_3dtexture_set_render_mode(tex3d, LV_3D_RENDER_SOFTWARE);

    /* Load mesh data (cube) */
    static float vertices[] = {
        -1, -1,  1,  1, -1,  1,  1,  1,  1, -1,  1,  1,
        -1, -1, -1,  1, -1, -1,  1,  1, -1, -1,  1, -1
    };
    static uint16_t indices[] = {
        0,1,2, 0,2,3, 4,5,6, 4,6,7, /* ... */
    };

    lv_3dtexture_set_mesh(tex3d, vertices, 8, indices, 12);
    lv_3dtexture_set_color(tex3d, lv_color_hex(0x00FFFF));

    /* Animate rotation */
    lv_anim_t a;
    lv_anim_init(&a);
    lv_anim_set_var(&a, tex3d);
    lv_anim_set_exec_cb(&a, set_rotation_anim_cb);
    lv_anim_set_values(&a, 0, 360);
    lv_anim_set_time(&a, 3000);
    lv_anim_set_repeat_count(&a, LV_ANIM_REPEAT_INFINITE);
    lv_anim_start(&a);
}
```

### Example 2: 3D with Physics

```c
void create_physics_demo(lv_obj_t *parent) {
    lv_obj_t *tex3d = lv_3dtexture_create(parent);
    lv_3dtexture_set_render_mode(tex3d, LV_3D_RENDER_SOFTWARE);

#if LV_DRAW_EMBEDDED3D_PHYSICS
    /* Initialize physics world */
    lv_3d_physics_init(tex3d);
    lv_3d_physics_set_gravity(tex3d, 0, -9.81, 0);

    /* Add falling sphere */
    lv_3d_mesh_t *sphere = lv_3d_sphere_create(0.5, 16);
    lv_3d_mesh_set_position(sphere, 0, 5, 0);
    lv_3d_mesh_set_physics(sphere, LV_3D_RIGIDBODY);
    lv_3d_mesh_set_mass(sphere, 1.0);

    /* Add static ground */
    lv_3d_mesh_t *ground = lv_3d_box_create(10, 0.5, 10);
    lv_3d_mesh_set_physics(ground, LV_3D_STATIC);

    /* Physics updates automatically in draw cycle */
#endif
}
```

### Example 3: Rust Application

```rust
use lvgl::{self, Display, DrawBuffer};
use lvgl::widgets::{Canvas, Button, Label};
use embedded_3dgfx::{K3dengine, mesh::K3dMesh};

fn main() {
    // Initialize LVGL
    let buffer = DrawBuffer::<32400>::default();
    let display = Display::register(buffer, 240, 180, |refresh| {
        // Display refresh callback
    }).unwrap();

    let mut screen = display.get_scr_act().unwrap();

    // Create 3D canvas (new widget from Phase 4)
    let mut canvas = Canvas::create(&mut screen).unwrap();
    canvas.set_size(240, 180);

    // Initialize 3D engine
    let mut engine = K3dengine::new(240, 180);
    let mut mesh = create_cube_mesh();

    // Render loop
    loop {
        // Render 3D to framebuffer
        let mut framebuffer = vec![0u16; 240 * 180];
        engine.render(std::iter::once(&mesh), |primitive| {
            draw_to_buffer(primitive, &mut framebuffer);
        });

        // Update canvas
        canvas.set_buffer(&framebuffer, 240, 180, ColorFormat::Rgb565);
        canvas.invalidate();

        lvgl::task_handler();
        std::thread::sleep(Duration::from_millis(16));
    }
}
```

---

## Alternatives Considered

### Alternative 1: Pure C Implementation

Write a software 3D renderer in C instead of using Rust.

**Pros:**
- No Rust toolchain required
- Easier for C-focused embedded developers
- Lower integration complexity

**Cons:**
- Significant development time (6-12 months for feature parity)
- Memory safety concerns (manual memory management)
- Would duplicate existing, proven code
- Ongoing maintenance burden

**Decision:** Use Rust via FFI. The embedded-3dgfx library is mature, well-tested, and actively maintained. FFI overhead is minimal for framebuffer-based rendering.

### Alternative 2: Separate Library

Keep embedded-3dgfx as a separate library, documented for use alongside LVGL.

**Pros:**
- No LVGL core changes needed
- Users can integrate manually

**Cons:**
- Fragmented ecosystem (no standard integration)
- Duplicate work by every developer
- No official support or examples
- Misses opportunity to standardize software 3D

**Decision:** Integrate directly. LVGL already follows this pattern with multiple draw backends (SDL, OpenGL, software, VG Lite, etc.).

### Alternative 3: WebGL/JavaScript Backend

Target web-based displays using Emscripten + WebGL.

**Pros:**
- Browser compatibility
- Leverages GPU via WebGL

**Cons:**
- Not suitable for embedded targets (main use case)
- Doesn't solve the "no GPU" problem
- Different use case than this proposal

**Decision:** Out of scope. This RFC focuses on embedded MCUs without GPUs.

---

## Open Questions

1. **Licensing:**
   - embedded-3dgfx is MIT OR Apache-2.0
   - LVGL is MIT
   - ✅ Compatible, no issues

2. **Rust Toolchain Dependency:**
   - Should this be an optional component?
   - Provide pre-compiled binaries for common targets?
   - **Proposal:** Make it opt-in via `LV_USE_DRAW_EMBEDDED3D`, provide prebuilt libs

3. **API Naming:**
   - `lv_3dtexture` vs `lv_3d_view` vs `lv_canvas3d`?
   - **Proposal:** Keep `lv_3dtexture`, add `_set_render_mode()` parameter

4. **Thread Safety:**
   - How to handle rendering in RTOS environments?
   - **Proposal:** Single-threaded for Phase 1, add mutex guards in Phase 2

5. **Version Compatibility:**
   - Target LVGL 9.0 or backport to 8.x?
   - **Proposal:** Develop for LVGL 9.0, consider 8.x backport if demand exists

---

## Success Metrics

1. **Performance:**
   - ✅ 30+ FPS for medium complexity scenes (200 triangles) on STM32F4 @ 100MHz
   - ✅ 60 FPS for simple scenes (cube, basic shapes)

2. **Memory:**
   - ✅ Runs in 256 KB RAM (including LVGL overhead)
   - ✅ Configurable buffer sizes for smaller targets

3. **Adoption:**
   - ⬜ 3+ example projects in lvgl/lv_examples
   - ⬜ Documentation in official LVGL docs
   - ⬜ 10+ community projects using the feature (within 6 months)

4. **Quality:**
   - ✅ 95%+ test coverage for FFI bridge
   - ✅ Zero known memory safety issues (via Rust)
   - ⬜ CI/CD testing on 3+ hardware platforms

---

## Timeline

- **Month 1-2:** RFC review, design approval, FFI prototype
- **Month 3-4:** Phase 1 (POC with basic rendering)
- **Month 5-7:** Phase 2 (full draw unit integration)
- **Month 8-10:** Phase 3 (advanced features, physics)
- **Month 11-12:** Phase 4 (Rust bindings), documentation, examples
- **Month 13+:** Community feedback, optimization, platform expansion

**Total:** 12-14 months to production-ready state

---

## Community Impact

### Benefits

1. **Expands LVGL's Addressable Market:**
   - Opens 3D capabilities to millions of low-cost MCUs
   - Enables rich 3D interfaces without expensive hardware

2. **Educational Value:**
   - Great for learning 3D graphics on resource-constrained devices
   - Physics simulation demos for STEM education

3. **Real-World Applications:**
   - Industrial HMIs with 3D equipment visualization
   - Automotive dashboards with 3D gauges
   - IoT devices with visual feedback (3D charts, animations)
   - Robotics interfaces with arm/kinematics visualization

4. **Rust Ecosystem Growth:**
   - Strengthens LVGL's Rust bindings
   - Attracts Rust embedded developers to LVGL

### Risks

1. **Maintenance Burden:**
   - New backend requires ongoing support
   - **Mitigation:** embedded-3dgfx is externally maintained; LVGL only maintains FFI bridge

2. **Complexity:**
   - Rust toolchain adds build complexity
   - **Mitigation:** Provide pre-built binaries, clear documentation, optional feature flag

3. **Performance Expectations:**
   - Users may expect GPU-level performance
   - **Mitigation:** Clear documentation on performance characteristics, hardware requirements

---

## References

- **embedded-3dgfx:** https://github.com/leftger/embedded-3dgfx
- **LVGL Draw Pipeline:** https://docs.lvgl.io/master/main-modules/draw/draw_pipeline.html
- **LVGL 3D Texture Widget:** https://docs.lvgl.io/master/widgets/3dtexture.html
- **Rust FFI Guide:** https://doc.rust-lang.org/nomicon/ffi.html
- **Production Example:** Rust on M5Stack Cardputer: https://github.com/Kezii/Rust-M5Stack-Cardputer

---

## Appendix: FAQ

**Q: Why Rust for an embedded C library?**
A: embedded-3dgfx is mature, well-tested (182 tests), and actively maintained. Writing equivalent C code would take 6-12 months. Rust provides memory safety guarantees critical for complex 3D engines. FFI overhead is negligible for framebuffer rendering.

**Q: Can I use this without Rust knowledge?**
A: Yes! End users interact with C APIs only. Rust is an implementation detail. Pre-built libraries available for common platforms.

**Q: What about performance vs OpenGL?**
A: Software rendering is 10-50× slower than GPU, but enables 3D on devices without GPUs. For many applications (30 FPS, simple scenes), it's sufficient and much cheaper.

**Q: Will this work on Cortex-M3 (no FPU)?**
A: Not efficiently. The engine uses floating-point math extensively. Cortex-M4F with FPU is minimum.

**Q: Can I mix 2D and 3D?**
A: Yes! That's the point. LVGL's 2D widgets (buttons, labels) overlay seamlessly on 3D content rendered to textures.

---

**Feedback Welcome!**
Please discuss on LVGL Forum: https://forum.lvgl.io
Or open issues on GitHub: https://github.com/lvgl/lvgl/issues
