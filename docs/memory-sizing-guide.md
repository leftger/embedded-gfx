# Memory Sizing Guide

This guide helps estimate memory for common deployment profiles and avoid surprise over-budget failures.

## Core Buffers

For `Rgb565`:

- Framebuffer bytes: `width * height * 2`
- Double buffer bytes: `2 * width * height * 2`
- Z-buffer bytes (`u32`): `width * height * 4`

Example (`240x135`):

- Single framebuffer: `240 * 135 * 2 = 64,800` bytes (~63.3 KiB)
- Double framebuffer: `129,600` bytes (~126.6 KiB)
- Z-buffer: `129,600` bytes (~126.6 KiB)

## Quick Budget Formula

Use this conservative estimate for render-only memory:

`total ~= framebuffers + zbuffer + command_buffer + scratch + scene_state`

Where:

- `framebuffers` is single or double buffer based on present mode.
- `command_buffer` depends on `CommandBuffer<N>` capacity.
- `scratch` includes temporary row/raster structures.
- `scene_state` includes mesh arrays, transforms, and any runtime metadata.

## Suggested Headroom

- Reserve 15-25% RAM headroom for spikes and stack growth.
- If physics/animation is enabled, prefer 25%+ headroom.
- Avoid selecting caps that leave <10% free RAM in normal scenes.

## Profile-Oriented Workflow

1. Start with `apply_default_caps(&mut engine)`.
2. Record telemetry for representative scenes.
3. If over-budget:
   - reduce meshes/triangles/textures,
   - lower resolution, or
   - adjust profile/caps.
4. Re-run telemetry and keep before/after snapshots.

## Command Buffer Sizing

Choose `CommandBuffer<N>` from measured draw command counts:

- Measure `record_telemetry.draw_commands` in steady and stress scenes.
- Add margin (typically +20-40%).
- Keep `N` fixed per profile/build to preserve deterministic memory use.

## Common Failure Modes

- `BudgetKind::MeshesPerFrame`: too many visible meshes.
- `BudgetKind::TrianglesPerMesh`: individual LOD too dense.
- `BudgetKind::VerticesPerMesh`: per-mesh geometry too large.
- `BudgetKind::Textures`: too many unique textures in one frame.
- `BudgetKind::ZBufferLength`: z-buffer slice size mismatch.

## Pre-Ship Checklist

- [ ] RAM budget documented per target profile.
- [ ] Z-buffer and framebuffer sizes validated against final resolution.
- [ ] Telemetry snapshots captured for steady/stress/fail-soft cases.
- [ ] Caps values and command-buffer capacity checked into source control.
