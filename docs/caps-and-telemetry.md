# Profile Caps and Telemetry

This guide covers how to run the record/execute pipeline with deterministic memory budgets, lightweight telemetry, and CI enforcement.

## Why this exists

Embedded targets need fixed, predictable frame costs. Different Cortex-M classes have different practical limits. Telemetry lets you measure record-time and execute-time pressure without heap allocation.

## Pipeline overview

The two-phase API separates scene traversal from rasterization:

1. **Record** — traverse the scene graph, emit draw commands into a bounded `CommandBuffer<N>`
2. **Execute** — rasterize the command buffer against your draw target and z-buffer

This keeps traversal and raster execution explicit and avoids hidden double work. Command buffers can be replayed without re-traversing the scene graph.

## Default profile caps

Apply default caps immediately after creating the engine:

```rust
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;

let mut engine = K3dengine::new(320, 240);
apply_default_caps(&mut engine);
```

Resolution:
- `desktop-unbounded` feature enabled → caps disabled
- otherwise → embedded balanced profile (defaults to M33 balanced)

Set caps explicitly with profile constants from `embedded_3dgfx::config`:

```rust
engine.set_caps(embedded_3dgfx::config::ProfileCaps { /* ... */ });
```

## Telemetry API

Telemetry structs are plain counters passed into record/execute calls:

**`RecordTelemetry`**
- `meshes_total`, `meshes_visible`, `unique_textures`, `draw_commands`, `fallback_used`

**`ExecuteTelemetry`**
- `commands_total`, `draw_commands`, `clear_color_commands`, `clear_depth_commands`

```rust
let mut record_tel = embedded_3dgfx::telemetry::RecordTelemetry::default();
let mut execute_tel = embedded_3dgfx::telemetry::ExecuteTelemetry::default();

engine.record(meshes.iter(), &mut commands, Some(&mut record_tel))?;
engine.execute(&mut display, &mut frame_ctx, &commands, Some(&mut execute_tel))?;
```

Pass `None` in hot paths to eliminate instrumentation overhead.

## Budget failures

When caps are exceeded, calls return `RenderError::OutOfBudget(BudgetKind::...)`.

Common `BudgetKind` variants:
- `MeshesPerFrame` — too many visible meshes
- `TrianglesPerMesh` — individual LOD too dense
- `VerticesPerMesh` — per-mesh geometry too large
- `Textures` — too many unique textures in one frame
- `ZBufferLength` — z-buffer slice size mismatch

Typical responses: reduce mesh complexity, lower visible count, increase `CommandBuffer<N>` capacity, or pick a less strict profile.

## Command buffer sizing

Choose `N` from measured draw command counts:

1. Measure `record_telemetry.draw_commands` in steady and stress scenes
2. Add 20–40% margin
3. Keep `N` fixed per profile/build for deterministic memory use

## Feature flags affecting the pipeline

**`triple-buffering`** — exposes `TripleSwapChain` APIs for smoother frame pacing at higher memory cost:
```bash
cargo check --lib --features triple-buffering
```

**`fixed-transform`** — routes projection through 16.16 fixed-point helpers (`src/fixed_math.rs`) for non-FPU targets:
```bash
cargo check --lib --features fixed-transform
```

The fixed-point path uses 16.16 arithmetic. Unit tests enforce near-roundtrip fidelity; pixel-space tolerance target is ≤ 1 px for typical scene ranges.

**`dwt-profiler` / `rtt-trace` / `itm-trace`** — board instrumentation hooks (see `docs/backend-integration.md`):
```bash
cargo check --lib --no-default-features --features "row_width_240 perfcounter dwt-profiler"
```

## Demo HUD

Telemetry HUD formatting is shared in `examples/shared/perf_hud.rs`. Current telemetry demos: `dma_rendering_demo`, `lod_demo`.

---

## CI Telemetry Budget Enforcement

### Telemetry snapshot classes

Integration tests emit structured telemetry lines consumed by `.github/scripts/check_telemetry_budget.py`:

| Class | Meaning | Expected `fallback_used` |
|-------|---------|--------------------------|
| `CI_TELEMETRY` | Steady scene | `0` |
| `CI_TELEMETRY_STRESS` | High-load scene | `0` |
| `CI_TELEMETRY_FAILSOFT` | Intentionally over-budget | `1` |

The checker validates **all** matching lines in the stream (not just the last), so multiple snapshot tests can be enforced in one CI step.

### Baseline thresholds

| Lane | record_draw_commands | execute_commands_total | execute_draw_commands |
|------|---------------------:|----------------------:|---------------------:|
| `CI_TELEMETRY` | ≤ 3 | ≤ 4 | ≤ 3 |
| `CI_TELEMETRY_STRESS` | ≤ 24 | ≤ 25 | ≤ 24 |
| `CI_TELEMETRY_FAILSOFT` | ≤ 1 | ≤ 2 | ≤ 1 |

`CI_TELEMETRY_FAILSOFT` also requires `primary_budget_kind=MeshesPerFrame` (configurable via `TELEMETRY_FAILSOFT_EXPECTED_BUDGET_KIND`).

### Per-profile baseline matrix

| Profile lane | record_draw_commands | execute_commands_total | execute_draw_commands |
|---|---:|---:|---:|
| `m3` | ≤ 3 | ≤ 4 | ≤ 3 |
| `m4` | ≤ 3 | ≤ 4 | ≤ 3 |
| `m33` | ≤ 3 | ≤ 4 | ≤ 3 |
| `m55` (profile-budget) | ≤ 3 | ≤ 4 | ≤ 3 |
| `m55-perf-budget` | ≤ 4 | ≤ 5 | ≤ 4 |

### Golden scene regression matrix

| Scene class | Test function |
|-------------|--------------|
| Points hash | `test_golden_hash_points_scene_record_execute` |
| Lines hash | `test_golden_hash_lines_scene_record_execute` |
| Solid hash | `test_golden_hash_solid_scene_record_execute` |
| Gouraud hash | `test_golden_hash_gouraud_scene_record_execute` |
| Telemetry baseline | `test_ci_telemetry_snapshot_record_execute` |
| Telemetry lines baseline | `test_ci_telemetry_snapshot_lines_record_execute` |
| Telemetry stress baseline | `test_ci_telemetry_snapshot_stress_points_record_execute` |
| Telemetry fail-soft baseline | `test_ci_telemetry_snapshot_failsoft_record_execute` |

### Performance evidence tests

- Dirty-region reporting: `test_execute_recorded_frame_reports_dirty_region`, `test_dirty_region_smaller_than_full_frame_for_small_scene`
- Tile/bin pipeline: `test_build_tile_bins_for_recorded_commands`, `test_tiled_execute_matches_non_tiled_pixel_count`

### Running the checker locally

```bash
cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture \
  2>&1 \
  | .github/scripts/check_telemetry_budget.py \
    --max-record-draw-commands 3 \
    --max-execute-commands-total 4 \
    --max-execute-draw-commands 3
```

Environment-variable form:
```bash
TELEMETRY_MAX_RECORD_DRAW_COMMANDS=3 \
TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL=4 \
TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS=3 \
TELEMETRY_STRESS_MAX_RECORD_DRAW_COMMANDS=24 \
TELEMETRY_STRESS_MAX_EXECUTE_COMMANDS_TOTAL=25 \
TELEMETRY_STRESS_MAX_EXECUTE_DRAW_COMMANDS=24 \
cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture 2>&1 \
  | .github/scripts/check_telemetry_budget.py
```

M55 perf lane:
```bash
EMBEDDED_3DGFX_CAPS=m55 \
TELEMETRY_MAX_RECORD_DRAW_COMMANDS=4 \
TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL=5 \
TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS=4 \
cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture 2>&1 \
  | .github/scripts/check_telemetry_budget.py
```

### CI jobs that enforce baselines

Defined in `.github/workflows/rust.yml`:
- `test` — default profile
- `embedded-budget` matrix — `row_width_96/160/240/320`
- `profile-budget` matrix — `m3/m4/m33/m55`
- `m55-perf-budget`

### Baseline update process

Only update thresholds for intentional architectural changes:

1. Capture old/new telemetry snapshots in PR notes
2. Explain why the change is necessary
3. Update workflow env thresholds
4. Keep `docs/backend-integration.md` compatibility matrix aligned

PRs that change telemetry thresholds must include the checklist in `.github/PULL_REQUEST_TEMPLATE.md`. A threshold change should not merge without: a short rationale, a profile impact note, and confirmation that fail-soft semantics remain correct.

### Full environment variable reference

```
TELEMETRY_MAX_RECORD_DRAW_COMMANDS
TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL
TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS
TELEMETRY_STRESS_MAX_RECORD_DRAW_COMMANDS
TELEMETRY_STRESS_MAX_EXECUTE_COMMANDS_TOTAL
TELEMETRY_STRESS_MAX_EXECUTE_DRAW_COMMANDS
TELEMETRY_FAILSOFT_MAX_RECORD_DRAW_COMMANDS
TELEMETRY_FAILSOFT_MAX_EXECUTE_COMMANDS_TOTAL
TELEMETRY_FAILSOFT_MAX_EXECUTE_DRAW_COMMANDS
TELEMETRY_FAILSOFT_EXPECTED_BUDGET_KIND
```
