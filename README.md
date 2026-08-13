# embedded-3dgfx

[![crates.io](https://img.shields.io/crates/v/embedded-3dgfx.svg)](https://crates.io/crates/embedded-3dgfx)
[![docs.rs](https://img.shields.io/docsrs/embedded-3dgfx)](https://docs.rs/embedded-3dgfx)
[![CI](https://github.com/leftger/embedded-3dgfx/actions/workflows/ci.yml/badge.svg)](https://github.com/leftger/embedded-3dgfx/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

A `no_std` 3D graphics and physics engine for embedded systems: software rasterization, rigid/soft-body physics, skeletal animation, and effects tuned for MCUs.

> Fork of [embedded-gfx](https://github.com/Kezii/embedded-gfx) by [Kezii](https://github.com/Kezii), extended with textures, fog/dithering, DMA swapchains, AA, physics, BSP, and more.

## Highlights

- **Record / execute** — traverse once, rasterize from a fixed-capacity command buffer (optional tiled / textured execute)
- **Rendering** — MVP + frustum/backface cull, Z-buffer, flat/Gouraud/Blinn-Phong, perspective-correct textures, fog, point lights, particles, LOD, HUD
- **Physics** *(feature `physics`)* — rigid bodies, joints, soft body, raycast with UV
- **Animation** — skeletal LBS, vertex morphs, transform tracks / tweens
- **Embedded-friendly** — `heapless` caps, async-agnostic swapchain present, optional DMA2D / Embassy hooks

## Screenshots

<table>
  <tr>
    <td align="center"><img src="assets/gif_suzanne.gif" alt="Blinn-Phong Suzanne" width="320"><br><em>Blinn-Phong</em></td>
    <td align="center"><img src="assets/gif_physics.gif" alt="Physics balls" width="320"><br><em>Rigid body physics</em></td>
  </tr>
  <tr>
    <td align="center"><img src="assets/gif_particles.gif" alt="Particles + fog" width="320"><br><em>Particles + fog</em></td>
    <td align="center"><img src="assets/gif_cloth.gif" alt="Cloth" width="320"><br><em>Soft-body cloth</em></td>
  </tr>
</table>

```bash
cargo run --example screenshots --features std
```

## Installation

```toml
[dependencies]
# Embedded (no_std) — slim 0.5 default is just `row_width_240`
embedded-3dgfx = { version = "0.5", default-features = false, features = ["row_width_320", "depth-u16"] }

# Orientation-style lit meshes
embedded-3dgfx = { version = "0.5", default-features = false, features = ["row_width_320", "depth-u16", "lighting"] }

# Desktop / simulator
embedded-3dgfx = { version = "0.5", features = ["std", "physics"] }
```

### MCU feature recipes

| Recipe | Features |
| :--- | :--- |
| Minimal wireframe / solid | `default-features = false`, `row_width_320`, `depth-u16` |
| Lit mesh (Gouraud / Blinn / Toon) | add `lighting` |
| Doom-style | `lighting`, `textured`, `raycast`, `hud` |
| Physics demo | add `physics` |

**0.5 breaking change:** `std`, `aa-heuristic`, and `aa-coverage` are no longer in the default feature set. Desktop apps should opt into `std` (and AA) explicitly.

## Quick start

```rust
use embedded_3dgfx::{K3dengine, mesh::{Geometry, K3dMesh, RenderMode}};
use nalgebra::Vector3;

let mut engine = K3dengine::new(320, 240);
engine.camera.set_position(Vector3::new(0.0, 0.0, 5.0).into());

let geometry = Geometry { vertices: &CUBE_VERTS, faces: &CUBE_FACES, /* ... */ };
let mut mesh = K3dMesh::new(geometry);
mesh.set_render_mode(RenderMode::Lines);

let mut commands = embedded_3dgfx::command_buffer::CommandBuffer::<512>::new();
engine.record(core::iter::once(&mesh), &mut commands, None).unwrap();
engine.execute(&mut display, &mut frame_ctx, &commands, None).unwrap();
```

More patterns (particles, lights, fog, physics, skeleton, soft body, async present) live under `examples/` and on [docs.rs](https://docs.rs/embedded-3dgfx).

## Feature flags

| Flag | Default | Description |
|------|---------|-------------|
| `row_width_*` | `240` | Row-buffer width (`96` / `160` / `240` / `320`, mutually exclusive) |
| `std` | off | Desktop helpers / `perfcounter` |
| `lighting` | off | `SolidLightDir` / Gouraud / Blinn / Toon / `SectorBright` + `lights` |
| `textured` | off | Texture modes + `texture` module (implies `lighting`) |
| `raycast` | off | Doom-style raycaster, BSP helpers, `sector_lights` |
| `scene` | off | Skeleton, character, particles, billboard, animation / scene stream |
| `hud` | off | HUD helpers |
| `painters` | off | Painter's algorithm helpers (`painters` module) |
| `physics` | off | Rigid body, soft body, physics raycast |
| `aa-heuristic` / `aa-coverage` | off | Triangle edge AA (coverage needs a W×H buffer) |
| `dsp` / `fixed-transform` / `fixed-raster` | off | Shared Q16.16 / quat path via [`embedded-dsp`](https://crates.io/crates/embedded-dsp) |
| `triple-buffering` / `embassy` / `dma2d` | off | Swapchain / Embassy / DMA2D hooks |
| `perfcounter` / `dwt-profiler` / `rtt-trace` / `itm-trace` | off | Timing / trace sinks |

Flash impact of the slim recipes is tracked in [`docs/feature-size.md`](docs/feature-size.md) (`size_harness` + CI budget).

### Optional scene extras *(off by default — keeps MCU binaries lean)*

| Feature | What you get |
|---------|----------------|
| `aabb-cull` | Cached AABB, two-stage frustum cull, raycast broadphase |
| `render-layers` | Camera ↔ mesh layer bitmasks |
| `record-sort` | Priority / distance sort in `record` |
| `lod-crossfade` | LOD fade margins |
| `anim-blend` | Clip blending, bone slerp, skinned AABBs (also enables `scene`) |
| `gizmos` | AABB / frustum debug wireframes |
| `visibility-extras` | `aabb-cull` + `render-layers` + `record-sort` + `lod-crossfade` |
| `scene-extras` | All of the above |

```toml
embedded-3dgfx = { version = "0.5", features = ["std", "scene-extras"] }
```

```bash
cargo test --test scene_extras --features "std,scene-extras"
```

## Examples

```bash
cargo run --example rotating_cube --features std
cargo run --example lighting_demo --features "std,lighting"
cargo run --example texture_mapping_demo --features "std,textured"
cargo run --example skeletal_animation_demo --features "std,scene"
# physics demos also need: --features "std,physics" (many also want lighting)
```

Rendering: `basic_rendering`, `rotating_cube`, `scene_viewer`, `lighting_demo`, `gouraud_demo`, `blinn_phong_demo`, `fog_dithering_demo`, `texture_mapping_demo`, `mesh_texture_demo`, `retro_presets_demo`, `bsp_builder_demo`, `dma_rendering_demo`, `billboard_demo`, `lod_demo`, `vertex_animation_demo`, `painters_algorithm_demo`, `boot_menu`, `stl_viewer`, …

Physics: `physics_rolling_ball`, `physics_bouncing_balls`, `physics_pendulum`, `physics_newtons_cradle`, `physics_stack_tower`, `cloth_simulation`, `jelly_cube_demo`, `raycast_demo`, `skeletal_animation_demo`, …

## Docs & bring-up

| Doc | Topic |
|-----|-------|
| [`docs/caps-and-telemetry.md`](docs/caps-and-telemetry.md) | Caps, telemetry, CI budgets |
| [`docs/feature-size.md`](docs/feature-size.md) | Slim vs full flash (`.text`) budgets |
| [`docs/backend-integration.md`](docs/backend-integration.md) | Board bring-up, memory sizing |
| [`docs/asset-pipeline.md`](docs/asset-pipeline.md) | Offline assets / scene streaming |

**Typical target:** Cortex-M4F/M33 with FPU; ~128 KB RAM minimum, ~512 KB+ recommended for double-buffer + Z + physics at 240×135.

## Testing

```bash
cargo test --lib
cargo test --lib --features dma2d,depth-u16
cargo test --test scene_extras --features "std,scene-extras"
```

Git hooks (fmt on commit / push): `./scripts/install-git-hooks.sh`

## Contributing

PRs welcome — especially board backends, broad-phase spatial structures, and extra joint / collider types.

## License

Dual-licensed under **MIT OR Apache-2.0**. See [`LICENSE-MIT`](./LICENSE-MIT), [`LICENSE-APACHE`](./LICENSE-APACHE), and [`NOTICE`](./NOTICE).
