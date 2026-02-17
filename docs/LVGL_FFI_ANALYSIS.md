# LVGL Rust FFI Analysis for embedded-3dgfx Integration

**Document Version:** 1.0
**Date:** 2026-02-16
**Purpose:** Technical guide for implementing FFI bindings between LVGL (C) and embedded-3dgfx (Rust)

---

## Executive Summary

This document analyzes the Foreign Function Interface (FFI) requirements for integrating the embedded-3dgfx Rust library with LVGL's C codebase. It covers:

1. Current state of LVGL Rust bindings
2. Missing functionality (Canvas widget)
3. FFI bridge architecture
4. Data structure mapping
5. Memory safety considerations
6. Implementation roadmap

---

## 1. Current LVGL Rust Bindings State

### Repository: `lvgl/lv_binding_rust`

**Architecture:**
```
┌─────────────────────────────────────────────────┐
│  Application Code (Safe Rust)                   │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│  lvgl crate (Safe Wrappers)                     │
│  - Widget types (Label, Button, etc.)           │
│  - Display, Style, Color abstractions           │
│  - Lifetime management                          │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│  lvgl-sys crate (Unsafe FFI)                    │
│  - Raw bindings via bindgen                     │
│  - Direct C function calls                      │
│  - repr(C) structs                              │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│  LVGL C Library (Vendored)                      │
│  - Version: 8.3.7 (commit 2b56e04)             │
└─────────────────────────────────────────────────┘
```

**Key Findings:**

✅ **Available:**
- Display registration with refresh callbacks
- Basic widgets: Label, Button, Bar, Arc, Slider, Meter, Table, Keyboard
- Style system (colors, fonts, alignment)
- Event handling
- Memory management (lv_mem_monitor)

❌ **Missing (Critical for our use case):**
- **Canvas widget** - No bindings for `lv_canvas`
- **3D texture widget** - No bindings for `lv_3dtexture`
- **Custom draw units** - No exposed APIs for draw unit registration
- **Direct framebuffer access** - Limited low-level buffer manipulation

### Binding Generation

Uses `bindgen` to auto-generate FFI from C headers:

```toml
# build.rs
bindgen::Builder::default()
    .header("vendor/lvgl/lvgl.h")
    .use_core()
    .generate()
```

**Implications:**
- Adding new widgets requires regenerating bindings + writing safe wrappers
- Type safety challenges for opaque pointers (`lv_obj_t *`)
- Callback handling requires careful lifetime management

---

## 2. Missing Canvas Widget

### C API (LVGL 8.3.7)

The Canvas widget exists in LVGL C but is **not exposed** in Rust bindings:

```c
// lv_canvas.h (C API)
lv_obj_t * lv_canvas_create(lv_obj_t * parent);
void lv_canvas_set_buffer(lv_obj_t * canvas, void * buf,
                           lv_coord_t w, lv_coord_t h,
                           lv_color_format_t cf);
void lv_canvas_set_px(lv_obj_t * canvas, int32_t x, int32_t y,
                       lv_color_t color, lv_opa_t opa);
lv_color_t lv_canvas_get_px(lv_obj_t * canvas, int32_t x, int32_t y);
void lv_canvas_fill_bg(lv_obj_t * canvas, lv_color_t color, lv_opa_t opa);

// Drawing functions
void lv_canvas_draw_rect(lv_obj_t * canvas, lv_coord_t x, lv_coord_t y,
                          lv_coord_t w, lv_coord_t h,
                          const lv_draw_rect_dsc_t * dsc);
void lv_canvas_draw_text(lv_obj_t * canvas, lv_coord_t x, lv_coord_t y,
                          lv_coord_t max_w, lv_draw_label_dsc_t * dsc,
                          const char * txt);
// ... more drawing primitives
```

### Required Rust Wrapper

Need to add to `lvgl/src/widgets/canvas.rs`:

```rust
use crate::Widget;
use crate::{Color, Obj, Result};
use core::ptr::NonNull;

/// Canvas widget for drawing arbitrary graphics
pub struct Canvas<'a> {
    core: Obj<'a>,
}

impl<'a> Canvas<'a> {
    /// Create a new canvas widget
    pub fn create(parent: &mut impl crate::NativeObject) -> Result<Self> {
        let raw = unsafe {
            let ptr = lvgl_sys::lv_canvas_create(parent.raw()?.as_mut());
            NonNull::new(ptr).ok_or(crate::LvError::InvalidReference)?
        };
        Ok(Self {
            core: Obj::from_raw(raw),
        })
    }

    /// Set the canvas buffer and dimensions
    ///
    /// # Safety
    /// The buffer must:
    /// - Live as long as the canvas object
    /// - Be properly aligned for the color format
    /// - Have size >= width * height * bytes_per_pixel
    pub fn set_buffer(
        &mut self,
        buffer: &'a mut [u16],  // RGB565
        width: u32,
        height: u32,
    ) -> Result<()> {
        unsafe {
            lvgl_sys::lv_canvas_set_buffer(
                self.core.raw()?.as_mut(),
                buffer.as_mut_ptr() as *mut _,
                width as i16,
                height as i16,
                lvgl_sys::LV_COLOR_FORMAT_RGB565,
            );
        }
        Ok(())
    }

    /// Set a single pixel
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Color) -> Result<()> {
        unsafe {
            lvgl_sys::lv_canvas_set_px(
                self.core.raw()?.as_mut(),
                x,
                y,
                color.raw,
                255, // Full opacity
            );
        }
        Ok(())
    }

    /// Fill entire canvas with color
    pub fn fill(&mut self, color: Color) -> Result<()> {
        unsafe {
            lvgl_sys::lv_canvas_fill_bg(
                self.core.raw()?.as_mut(),
                color.raw,
                255,
            );
        }
        Ok(())
    }

    /// Invalidate canvas to trigger redraw
    pub fn invalidate(&mut self) {
        unsafe {
            lvgl_sys::lv_obj_invalidate(self.core.raw().unwrap().as_mut());
        }
    }
}

impl<'a> Widget for Canvas<'a> {
    type SpecialEvent = ();
    type Part = crate::Part;

    unsafe fn from_raw(raw: NonNull<lvgl_sys::lv_obj_t>) -> Self {
        Self {
            core: Obj::from_raw(raw),
        }
    }

    fn raw(&self) -> core::result::Result<NonNull<lvgl_sys::lv_obj_t>, crate::LvError> {
        self.core.raw()
    }
}
```

**Why This Matters:**
The Canvas widget is the **primary integration point** for displaying embedded-3dgfx framebuffers in LVGL.

---

## 3. FFI Bridge Architecture

### Three-Layer Design

```
┌──────────────────────────────────────────────────────────┐
│  Application Layer (Rust or C)                           │
│  - Uses high-level APIs                                  │
└──────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────┐
│  Safe Rust Wrapper Layer                                 │
│  embedded3d_lvgl crate                                   │
│  - Widget3D<'a> struct                                   │
│  - Safe API: render_scene(), set_camera(), etc.          │
│  - Lifetime management                                   │
└──────────────────────────────────────────────────────────┘
                         ↓
┌──────────────────────────────────────────────────────────┐
│  FFI Bridge Layer (Rust ↔ C)                            │
│  embedded3d_ffi crate                                    │
│  - #[no_mangle] extern "C" functions                     │
│  - repr(C) data structures                               │
│  - Pointer conversions                                   │
└──────────────────────────────────────────────────────────┘
         ↓                                    ↓
┌──────────────────────┐      ┌───────────────────────────┐
│  embedded-3dgfx      │      │  LVGL C Library           │
│  (Rust)              │      │  - lv_canvas              │
│  - K3dengine         │      │  - lv_3dtexture           │
│  - K3dMesh           │      │  - Display system         │
│  - Physics, etc.     │      │                           │
└──────────────────────┘      └───────────────────────────┘
```

### Crate Organization

```
embedded-3dgfx/
├── src/                 # Existing 3D engine code
├── lvgl_integration/    # New integration code
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs       # Re-exports
│   │   ├── ffi.rs       # Raw FFI bindings
│   │   ├── widget.rs    # Safe Widget3D wrapper
│   │   ├── convert.rs   # Data type conversions
│   │   └── buffer.rs    # Framebuffer management
│   └── build.rs         # Optionally link LVGL
└── examples/
    └── lvgl_3d_demo.rs
```

---

## 4. FFI Data Structure Mapping

### Challenge: Type System Mismatch

**C (LVGL):**
- Pointer-heavy, manual memory management
- Dynamically-typed objects (`lv_obj_t *` for everything)
- No lifetime tracking
- Global state (display, screens)

**Rust (embedded-3dgfx):**
- Ownership and borrowing
- Strongly typed (K3dMesh, Camera, etc.)
- Compile-time lifetime checking
- Stack-allocated when possible

### Key Data Structures

#### 4.1 Mesh Data

**embedded-3dgfx (Rust):**
```rust
pub struct K3dMesh<'a> {
    geometry: Geometry<'a>,
    render_mode: RenderMode,
    transform: Transform,
    // ...
}

pub struct Geometry<'a> {
    pub vertices: &'a [[f32; 3]],
    pub faces: &'a [[usize; 3]],
    pub colors: &'a [Rgb565],
    pub normals: &'a [[f32; 3]],
    pub uvs: &'a [[f32; 2]],
    pub texture_id: Option<u32>,
}
```

**FFI Bridge (repr(C)):**
```rust
#[repr(C)]
pub struct FfiMeshData {
    vertices_ptr: *const f32,
    vertices_count: usize,

    faces_ptr: *const u32,      // Convert usize to u32
    faces_count: usize,

    normals_ptr: *const f32,
    normals_count: usize,

    // Transform matrix (4x4 column-major)
    transform: [f32; 16],

    // Render settings
    render_mode: u32,  // 0=Points, 1=Lines, 2=Solid
    color_rgb565: u16,
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_render_mesh(
    engine: *mut K3dengine,
    mesh_data: *const FfiMeshData,
    output_buffer: *mut u16,
    width: u16,
    height: u16,
) -> i32 {
    // 1. Validate pointers
    if engine.is_null() || mesh_data.is_null() || output_buffer.is_null() {
        return -1; // Error
    }

    // 2. Convert FFI data to Rust types
    let engine = &mut *engine;
    let mesh = convert_ffi_to_mesh(mesh_data)?;

    // 3. Render to framebuffer
    let fb_slice = std::slice::from_raw_parts_mut(
        output_buffer,
        (width as usize) * (height as usize)
    );

    engine.render(std::iter::once(&mesh), |primitive| {
        draw_to_rgb565_buffer(primitive, fb_slice, width, height);
    });

    0 // Success
}
```

**Key Conversions:**

| Rust Type | C/FFI Type | Conversion |
|-----------|------------|------------|
| `&[f32; 3]` | `*const f32` + count | Flatten array |
| `&[usize; 3]` | `*const u32` + count | Cast usize → u32 |
| `Rgb565` | `uint16_t` | Direct (same repr) |
| `nalgebra::Matrix4<f32>` | `float[16]` | Column-major array |
| `Vector3<f32>` | `float[3]` | Direct copy |

#### 4.2 Camera

**embedded-3dgfx:**
```rust
pub struct Camera {
    pub position: Point3<f32>,
    pub target: Point3<f32>,
    pub up: Vector3<f32>,
    pub fov: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    // Internal matrices
    view_matrix: Matrix4<f32>,
    projection_matrix: Matrix4<f32>,
}
```

**FFI:**
```rust
#[repr(C)]
pub struct FfiCamera {
    position: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
    fov: f32,
    aspect: f32,
    near: f32,
    far: f32,
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_set_camera(
    engine: *mut K3dengine,
    camera: *const FfiCamera,
) -> i32 {
    let engine = &mut *engine;
    let cam = &*camera;

    engine.camera.set_position(Point3::from_slice(&cam.position));
    engine.camera.set_target(Point3::from_slice(&cam.target));
    engine.camera.set_fov(cam.fov);
    // ... etc

    0
}
```

#### 4.3 Framebuffer

**Shared Memory Approach:**

```rust
// Rust side: Create framebuffer managed by Rust
static mut FRAMEBUFFER: Option<Vec<u16>> = None;

#[no_mangle]
pub unsafe extern "C" fn embedded3d_init(width: u16, height: u16) -> *mut u16 {
    let size = (width as usize) * (height as usize);
    let fb = vec![0u16; size];
    let ptr = fb.as_mut_ptr();
    FRAMEBUFFER = Some(fb);
    ptr // Return pointer to C
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_get_framebuffer() -> *mut u16 {
    FRAMEBUFFER.as_mut().map(|fb| fb.as_mut_ptr()).unwrap_or(std::ptr::null_mut())
}
```

**C side (LVGL):**
```c
uint16_t *fb = embedded3d_init(240, 180);

lv_obj_t *canvas = lv_canvas_create(parent);
lv_canvas_set_buffer(canvas, fb, 240, 180, LV_COLOR_FORMAT_RGB565);

// Render 3D
embedded3d_render_mesh(engine, mesh_data, fb, 240, 180);
lv_obj_invalidate(canvas); // Trigger LVGL redraw
```

---

## 5. Memory Safety Considerations

### 5.1 Lifetime Management

**Problem:** C has no concept of Rust lifetimes.

**Solutions:**

1. **Opaque Pointers:**
```rust
// Don't expose internals
pub struct K3dengine { /* ... */ }

// C sees this as an opaque pointer
#[no_mangle]
pub extern "C" fn embedded3d_engine_create(w: u16, h: u16) -> *mut K3dengine {
    Box::into_raw(Box::new(K3dengine::new(w, h)))
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_engine_destroy(engine: *mut K3dengine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine)); // Properly deallocate
    }
}
```

2. **Borrowed Data Validation:**
```rust
unsafe fn validate_mesh_data(data: *const FfiMeshData) -> Result<(), &'static str> {
    if data.is_null() {
        return Err("Null mesh data pointer");
    }

    let mesh = &*data;

    if mesh.vertices_ptr.is_null() && mesh.vertices_count > 0 {
        return Err("Null vertices pointer with non-zero count");
    }

    // Check alignment
    if (mesh.vertices_ptr as usize) % std::mem::align_of::<f32>() != 0 {
        return Err("Misaligned vertices pointer");
    }

    Ok(())
}
```

3. **Reference Counting (if needed):**
```rust
use std::sync::Arc;

struct EngineHandle {
    engine: Arc<Mutex<K3dengine>>,
}

// Allows shared ownership between Rust and C
```

### 5.2 Thread Safety

**Issue:** LVGL is typically single-threaded, but embedded-3dgfx could theoretically use multiple cores.

**Strategy:**
- **Phase 1:** Single-threaded only, no sync primitives
- **Phase 2+:** Add `Mutex<K3dengine>` if multi-threaded rendering needed

```rust
#[cfg(feature = "thread-safe")]
static ENGINE: Mutex<Option<K3dengine>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn embedded3d_render_threadsafe(/* ... */) {
    let mut engine = ENGINE.lock().unwrap();
    // ... render
}
```

### 5.3 Error Handling

**C Convention:** Return codes (0 = success, negative = error)

```rust
#[repr(i32)]
pub enum FfiError {
    Success = 0,
    NullPointer = -1,
    InvalidDimensions = -2,
    OutOfMemory = -3,
    RenderFailed = -4,
}

#[no_mangle]
pub unsafe extern "C" fn embedded3d_render_mesh(/* ... */) -> i32 {
    match render_mesh_internal(/* ... */) {
        Ok(_) => FfiError::Success as i32,
        Err(e) => {
            log_error(e); // Optional: log to stderr
            FfiError::RenderFailed as i32
        }
    }
}
```

### 5.4 Memory Leaks

**Common Pitfalls:**

1. **Forgetting to free allocated objects:**
```rust
// BAD: Memory leak
let engine = Box::new(K3dengine::new(240, 180));
Box::into_raw(engine); // Leaks if never destroyed

// GOOD: Provide destructor
#[no_mangle]
pub unsafe extern "C" fn embedded3d_engine_destroy(ptr: *mut K3dengine) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}
```

2. **Double-free:**
```rust
// BAD: Don't expose drop directly
pub unsafe extern "C" fn embedded3d_mesh_free(mesh: *mut K3dMesh) {
    drop(Box::from_raw(mesh)); // Caller could call twice!
}

// GOOD: Nullify pointer after freeing (C pattern)
pub unsafe extern "C" fn embedded3d_mesh_free(mesh: *mut *mut K3dMesh) {
    if !mesh.is_null() && !(*mesh).is_null() {
        drop(Box::from_raw(*mesh));
        *mesh = std::ptr::null_mut(); // Prevent double-free
    }
}
```

---

## 6. Performance Considerations

### 6.1 FFI Overhead

**Typical FFI call costs:**
- Function call: ~1-5 ns (negligible)
- Pointer dereference: ~1 ns
- Data validation: ~10-50 ns
- **Total:** < 100 ns per FFI call

For framebuffer rendering at 60 FPS (16.6 ms budget), even 1000 FFI calls = 0.1 ms overhead (0.6% of frame budget).

**Conclusion:** FFI overhead is negligible for this use case.

### 6.2 Data Copying

**Avoid unnecessary copies:**

```rust
// BAD: Copies entire framebuffer
pub extern "C" fn render_and_copy(/* ... */) -> Vec<u16> {
    let mut fb = vec![0; width * height];
    render_to(&mut fb);
    fb // Returns by value, copies
}

// GOOD: Write directly to caller-provided buffer
pub unsafe extern "C" fn render_to_buffer(
    output: *mut u16,
    width: u16,
    height: u16
) {
    let fb = std::slice::from_raw_parts_mut(output, (width * height) as usize);
    render_to(fb); // No copy
}
```

### 6.3 Cache Efficiency

Ensure data structures are cache-friendly:

```rust
// GOOD: Contiguous memory
#[repr(C)]
struct FfiVertex {
    x: f32, y: f32, z: f32,  // 12 bytes, cache-aligned
}

// BAD: Scattered memory
struct NonContiguousVertex {
    x: Box<f32>,  // Heap allocation
    y: Box<f32>,  // Each field is a pointer chase
    z: Box<f32>,
}
```

---

## 7. Implementation Roadmap

### Phase 1: Minimal FFI Bridge (Week 1-2)

**Goal:** Render a single cube to LVGL display

```rust
// Minimal API surface
#[no_mangle] pub extern "C" fn embedded3d_init(w: u16, h: u16) -> *mut K3dengine;
#[no_mangle] pub extern "C" fn embedded3d_destroy(engine: *mut K3dengine);
#[no_mangle] pub unsafe extern "C" fn embedded3d_render_cube(
    engine: *mut K3dengine,
    fb: *mut u16
);
```

**Testing:**
- Rust unit tests for FFI functions
- C test harness calling Rust functions
- Valgrind for memory leaks

### Phase 2: Canvas Widget (Week 3-4)

**Goal:** Add Canvas to lvgl-rs, use for 3D display

**Tasks:**
1. Fork `lv_binding_rust`
2. Add `canvas.rs` with FFI bindings (see section 2)
3. Write integration tests
4. Submit PR to upstream

### Phase 3: Full Mesh API (Week 5-6)

**Goal:** Support arbitrary meshes, not just cubes

```rust
#[no_mangle] pub unsafe extern "C" fn embedded3d_create_mesh(
    vertices: *const f32,
    vertex_count: usize,
    faces: *const u32,
    face_count: usize
) -> *mut K3dMesh;

#[no_mangle] pub unsafe extern "C" fn embedded3d_mesh_set_transform(
    mesh: *mut K3dMesh,
    transform: *const f32  // 4x4 matrix
);
```

### Phase 4: Camera Control (Week 7)

**Goal:** Interactive camera manipulation

```rust
#[no_mangle] pub unsafe extern "C" fn embedded3d_set_camera_position(
    engine: *mut K3dengine,
    x: f32, y: f32, z: f32
);
```

### Phase 5: Advanced Features (Week 8+)

- Physics integration
- Texture mapping
- Lighting controls
- Performance profiling

---

## 8. Testing Strategy

### 8.1 Unit Tests (Rust)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_engine_lifecycle() {
        unsafe {
            let engine = embedded3d_init(240, 180);
            assert!(!engine.is_null());
            embedded3d_destroy(engine);
        }
    }

    #[test]
    fn test_null_pointer_handling() {
        unsafe {
            let result = embedded3d_render_mesh(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                240, 180
            );
            assert_eq!(result, FfiError::NullPointer as i32);
        }
    }
}
```

### 8.2 Integration Tests (C)

```c
// test_ffi.c
#include <assert.h>
#include "embedded3d_ffi.h"

void test_basic_rendering() {
    void *engine = embedded3d_init(240, 180);
    assert(engine != NULL);

    uint16_t *fb = malloc(240 * 180 * sizeof(uint16_t));
    int result = embedded3d_render_cube(engine, fb);
    assert(result == 0);

    // Verify framebuffer has non-zero pixels
    int non_zero = 0;
    for (int i = 0; i < 240 * 180; i++) {
        if (fb[i] != 0) non_zero++;
    }
    assert(non_zero > 0);

    free(fb);
    embedded3d_destroy(engine);
}
```

### 8.3 Memory Safety Tests

```bash
# Run with AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" cargo test

# Run C tests with Valgrind
valgrind --leak-check=full ./test_ffi

# Run with MIRI (Rust undefined behavior detector)
cargo +nightly miri test
```

---

## 9. Documentation Requirements

### 9.1 FFI Header File

**`embedded3d_ffi.h`:**

```c
#ifndef EMBEDDED3D_FFI_H
#define EMBEDDED3D_FFI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque engine handle
typedef struct K3dengine K3dengine;

// Error codes
#define E3D_SUCCESS           0
#define E3D_NULL_POINTER     -1
#define E3D_INVALID_DIMS     -2
#define E3D_OUT_OF_MEMORY    -3
#define E3D_RENDER_FAILED    -4

/**
 * Initialize 3D engine
 * @param width Framebuffer width in pixels
 * @param height Framebuffer height in pixels
 * @return Engine handle, or NULL on failure
 */
K3dengine *embedded3d_init(uint16_t width, uint16_t height);

/**
 * Destroy engine and free resources
 * @param engine Handle from embedded3d_init()
 */
void embedded3d_destroy(K3dengine *engine);

/**
 * Render a cube to framebuffer
 * @param engine Engine handle
 * @param framebuffer RGB565 buffer (width * height * 2 bytes)
 * @return E3D_SUCCESS or error code
 */
int embedded3d_render_cube(K3dengine *engine, uint16_t *framebuffer);

#ifdef __cplusplus
}
#endif

#endif // EMBEDDED3D_FFI_H
```

### 9.2 Usage Example Documentation

Create `docs/LVGL_INTEGRATION_GUIDE.md` with:
- Prerequisites (Rust toolchain, LVGL version)
- Build instructions
- API reference
- Complete working examples
- Troubleshooting

---

## 10. Open Issues & Future Work

### 10.1 LVGL 9.0 Support

**Status:** Current Rust bindings target LVGL 8.3.7

**Action Items:**
1. Monitor LVGL 9.0 stable release
2. Update vendored submodule
3. Regenerate bindings with bindgen
4. Test compatibility

### 10.2 No-std Support in FFI

**Challenge:** Current FFI might use `std::vec::Vec`

**Solution:**
```rust
#[cfg(not(feature = "std"))]
use heapless::Vec;

#[cfg(feature = "std")]
use std::vec::Vec;
```

### 10.3 Cross-compilation

**Platforms to test:**
- `thumbv7em-none-eabihf` (Cortex-M4F/M7)
- `thumbv8m.main-none-eabihf` (Cortex-M33)
- `riscv32imc-unknown-none-elf` (RISC-V)

---

## 11. Summary

### Critical Path Items

1. ✅ **Analyze current LVGL Rust bindings** - Done
2. ⬜ **Add Canvas widget to lvgl-rs** - Needed for MVP
3. ⬜ **Create FFI bridge crate** - Core integration work
4. ⬜ **Implement safe Rust wrapper** - User-facing API
5. ⬜ **Write comprehensive tests** - Ensure reliability
6. ⬜ **Document thoroughly** - Enable adoption

### Estimated Effort

- **FFI Bridge:** 2-3 weeks (1 engineer)
- **Canvas Widget:** 1 week
- **Testing & Documentation:** 1-2 weeks
- **Total:** ~4-6 weeks for MVP

### Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Memory leaks | Medium | High | Valgrind, careful review |
| ABI compatibility | Low | High | Use `repr(C)`, test extensively |
| Performance issues | Low | Medium | Profile early, optimize hot paths |
| Upstream bindings | Medium | Medium | Fork if needed, submit PRs |

---

## References

- **FFI Nomicon:** https://doc.rust-lang.org/nomicon/ffi.html
- **bindgen User Guide:** https://rust-lang.github.io/rust-bindgen/
- **LVGL C API Docs:** https://docs.lvgl.io/8.3/
- **LVGL Rust Bindings:** https://github.com/lvgl/lv_binding_rust
- **embedded-3dgfx:** https://github.com/leftger/embedded-3dgfx

---

**Document Status:** Ready for implementation

For questions or contributions, please open an issue on the embedded-3dgfx GitHub repository.
