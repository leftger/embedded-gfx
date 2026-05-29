# Backend Integration

This guide covers everything needed to bring up a new board or display backend: architecture constraints, memory sizing, hardware profiling, smoke testing, and a validated compatibility matrix.

## no_std Frame-Path Architecture

The crate root is `#![no_std]`. The `std` feature is opt-in.

Runtime frame APIs (`record`, `execute`, `execute_tiled`) operate on caller-provided buffers only:

- Command recording writes into a bounded `CommandBuffer<N>` (heapless storage)
- Execution writes into caller-provided framebuffer and z-buffer slices
- Tile binning uses compile-time-bounded `heapless` vectors
- No heap allocation in frame-path functions

CI runs `.github/scripts/check_no_alloc_frame_path.py` to reject obvious heap-allocation patterns in frame-path functions. `--no-default-features` checks remain in CI to validate constrained/no_std-style builds.

**Expected frame loop:**

1. Pre-allocate command buffer and z-buffer
2. `engine.record(meshes, &mut commands, telemetry)?`
3. `engine.execute(&mut display, &mut frame_ctx, &commands, telemetry)?`
4. Present full frame or region through backend/swapchain APIs

## Memory Sizing

### Core buffer formulas (Rgb565)

| Buffer | Bytes |
|--------|-------|
| Single framebuffer | `width × height × 2` |
| Double framebuffer | `2 × width × height × 2` |
| Triple framebuffer | `3 × width × height × 2` |
| Z-buffer (`u32`) | `width × height × 4` |

Example at 240×135:
- Single framebuffer: 64,800 bytes (~63 KiB)
- Double framebuffer: 129,600 bytes (~127 KiB)
- Z-buffer: 129,600 bytes (~127 KiB)

### Quick budget formula

```
total ≈ framebuffers + zbuffer + command_buffer + scratch + scene_state
```

Where `command_buffer` depends on `CommandBuffer<N>` capacity, `scratch` includes temporary row/raster structures, and `scene_state` includes mesh arrays and transforms.

### Headroom

- Reserve 15–25% RAM for spikes and stack growth
- If physics/animation is enabled, prefer 25%+ headroom
- Avoid caps that leave <10% free RAM in normal scenes

### Common budget failures

| Error | Cause |
|-------|-------|
| `BudgetKind::MeshesPerFrame` | Too many visible meshes |
| `BudgetKind::TrianglesPerMesh` | Individual LOD too dense |
| `BudgetKind::VerticesPerMesh` | Per-mesh geometry too large |
| `BudgetKind::Textures` | Too many unique textures per frame |
| `BudgetKind::ZBufferLength` | Z-buffer slice size mismatch |

### Pre-ship checklist

- [ ] RAM budget documented per target profile
- [ ] Z-buffer and framebuffer sizes validated against final resolution
- [ ] Telemetry snapshots captured for steady/stress/fail-soft cases
- [ ] Caps values and command-buffer capacity checked into source control

## Bring-Up Checklist

### 1. Platform profile and budget selection

- [ ] Pick a baseline profile from `embedded_3dgfx::config` (`m3`, `m4`, `m33`, `m55`)
- [ ] Confirm target resolution and color format (`Rgb565`)
- [ ] Verify framebuffer + z-buffer fit within available RAM
- [ ] Verify `CommandBuffer<N>` fits frame complexity

### 2. DrawTarget and present path

- [ ] Implement or adapt an `embedded_graphics` `DrawTarget<Color = Rgb565>`
- [ ] Confirm correct origin/dimensions via `OriginDimensions`
- [ ] Validate full-frame present path first (no partial update optimization yet)
- [ ] Add a smoke render that draws points, lines, and filled triangles

### 3. Render loop wiring

- [ ] Use record/execute flow in frame loop:
  1. `engine.record(...)`
  2. `engine.execute(...)`
- [ ] Reuse pre-allocated z-buffer and command buffer across frames
- [ ] Apply caps at startup (`apply_default_caps(...)` or `set_caps(...)`)
- [ ] Verify budget errors are surfaced and logged (`RenderError::OutOfBudget(...)`)

### 4. Performance and telemetry

- [ ] Capture `RecordTelemetry` and `ExecuteTelemetry` for representative scenes
- [ ] Confirm fallback behavior for constrained profiles where expected
- [ ] Record at least one "steady" and one "stress" snapshot for your board
- [ ] Keep measurements with build flags and profile noted

### 5. Reliability checks

- [ ] Run deterministic integration tests locally before hardware pass
- [ ] Validate no panics under camera movement, culling, and empty-scene cases
- [ ] Validate over-budget behavior is deterministic
- [ ] Confirm render output remains stable across repeated runs

### 6. Board smoke test

- [ ] Add a board smoke command to CI docs or board-specific automation
- [ ] Include: startup render, moving camera, and one constrained-budget scene
- [ ] Define pass/fail signals (console output, telemetry line, screenshot hash)
- [ ] Link this backend row in the compatibility matrix below when validated

### Reference bring-up sequence

```bash
# 1. Constrained build check
EMBEDDED_3DGFX_CAPS=m4 cargo check --lib --no-default-features --features row_width_240

# 2. Deterministic telemetry snapshots
EMBEDDED_3DGFX_CAPS=m4 cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture

# 3. Validate telemetry limits
# (pipe output to .github/scripts/check_telemetry_budget.py)

# 4. Run at least one interactive scene on hardware using record/execute path

# 5. Capture steady + stress snapshots in board notes

# 6. Mark backend in compatibility matrix once repeatable
```

**Exit criteria:** backend is complete when the board renders the canonical record/execute path, profile/cap limits are configured and validated, and a repeatable smoke procedure exists with actionable failure signals.

## Hardware Profiling (DWT / RTT / ITM)

### Feature flags

| Flag | Purpose |
|------|---------|
| `dwt-profiler` | DWT cycle-counter sampling hooks |
| `rtt-trace` | Emits `RTT_TRACE ...` markers |
| `itm-trace` | Emits `ITM_TRACE ...` markers |

```bash
cargo check --lib --no-default-features --features "row_width_240 perfcounter dwt-profiler"
```

### How it works

- `PerformanceCounter::start_of_frame()` initializes DWT counter hooks
- `add_measurement()` records microseconds and cycle deltas where available
- `print()` records frame-level cycle totals
- Hooks are implemented in `src/hardware_profile.rs`
- DWT cycle reads are active only on ARM targets with `dwt-profiler`
- RTT/ITM sinks emit textual markers suitable for board logging pipelines
- On non-ARM or when feature-disabled, hooks degrade safely to no-op

### Suggested board workflow

1. Flash a profile-constrained build
2. Capture `perf.frame` and `perf.measurement` traces for steady + stress scenes
3. Track cycle deltas across commits to detect regressions
4. Attach trace snippets to PRs that modify rendering hot paths

## Hardware Smoke Test Workflow

### Workflow entrypoint

`.github/workflows/hardware-smoke.yml` — designed for `workflow_dispatch` with self-hosted runners.

Expected runner labels: `self-hosted`, `embedded-3dgfx-hw`, board label (e.g. `stm32-m4`, `stm32-m33`, `cortex-m55`).

### Smoke script

`.github/scripts/hardware_smoke.sh` emits standardized lines:
```
HARDWARE_SMOKE board=... profile=... stage=... status=...
```

Validated by `.github/scripts/check_hardware_smoke.py`.

### Minimum smoke stages

1. Build constrained profile for board
2. Run deterministic telemetry snapshot tests
3. Validate telemetry output format and expectations
4. Persist smoke log artifact

Failure output should include: board identifier, profile, failed stage, and command context.

---

## Compatibility Matrix

Legend: **Validated** = covered by CI and/or deterministic integration checks | **Expected** = should work, not yet formally validated | **Planned** = not yet implemented

### Build and feature profiles

| Profile | Status | Validation source |
|---------|--------|-------------------|
| `std` default | Validated | `rust.yml` — check, test, examples |
| `--no-default-features --features row_width_240` | Validated | `rust.yml` — embedded-budget matrix |
| `desktop-unbounded` | Expected | Local/demo usage |
| `row_width_96/160/320` | Validated | `rust.yml` — embedded-budget matrix |

### Rendering pipeline

| Capability | Status | Validation source |
|------------|--------|-------------------|
| Record/execute command buffer pipeline | Validated | Integration tests + examples |
| Deterministic command recording | Validated | `tests/integration_tests.rs` |
| Golden-output regression digests | Validated | `tests/integration_tests.rs` |
| Telemetry counters (record/execute) | Validated | Integration tests + demo HUD |
| Fail-soft fallback APIs | Validated | Integration tests + demos |

### CI budget guard coverage

| Snapshot class | Status | Expected signals |
|----------------|--------|-----------------|
| `CI_TELEMETRY` | Validated | `fallback_used=0` |
| `CI_TELEMETRY_STRESS` | Validated | `fallback_used=0` |
| `CI_TELEMETRY_FAILSOFT` | Validated | `fallback_used=1`, expected budget kind |

### Backends and runtime targets

| Target / Backend | Status | Notes |
|------------------|--------|-------|
| `embedded-graphics-simulator` desktop | Validated | Primary interactive demo environment |
| Generic `DrawTarget` execution path | Validated | Core renderer backend-agnostic path |
| Hardware SPI/LTDC/DMA2D board backends | Planned | Needs board-specific implementation and smoke tests |
| Hardware-in-the-loop CI | Planned | Not yet in CI |

### MCU class coverage

| MCU class | Status | Validation source |
|-----------|--------|-------------------|
| Cortex-M33 balanced | Validated | Default caps + demos/tests + `profile-budget` CI matrix |
| Cortex-M4 balanced | Validated | `rust.yml` profile-budget matrix (`EMBEDDED_3DGFX_CAPS=m4`) |
| Cortex-M3 balanced | Validated | `rust.yml` profile-budget matrix (`EMBEDDED_3DGFX_CAPS=m3`) |
| Cortex-M55 perf | Validated | `rust.yml` profile-budget matrix + `m55-perf-budget` lane |

### Extending the matrix

When adding a new backend/profile/board:

1. Add a deterministic test or CI job that exercises it
2. Add threshold/telemetry assertions where possible
3. Update the row above from Expected/Planned to Validated
4. Link the validating workflow step or test name in the notes column
