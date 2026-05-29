# Profile Caps and Telemetry

This guide explains how to run the record/execute pipeline with deterministic memory budgets and lightweight telemetry.

## Why this exists

- Embedded targets need fixed, predictable frame costs.
- Different Cortex-M classes have different practical limits.
- Telemetry lets you measure record-time and execute-time pressure without heap allocation.

## Pipeline overview

Use the two-phase API:

1. Record render commands into a bounded `CommandBuffer<N>`.
2. Execute the command buffer against your draw target and z-buffer.

This keeps traversal and raster execution explicit and avoids hidden double work.

## Default profile caps

Apply default caps immediately after creating the engine:

```rust
use embedded_3dgfx::K3dengine;
use embedded_3dgfx::config::apply_default_caps;

let mut engine = K3dengine::new(320, 240);
apply_default_caps(&mut engine);
```

Default resolution is:

- `desktop-unbounded` feature enabled: caps disabled
- otherwise: an embedded balanced profile (defaulting to M33 balanced)

You can also set caps explicitly with profile constants from `embedded_3dgfx::config`.

## Environment/profile selection

Use build features for broad target classes and env overrides for local experiments.

Suggested flow:

- CI embedded job: build with embedded profile features and no `desktop-unbounded`
- Desktop simulator job: build with `desktop-unbounded` for unconstrained experimentation
- Local budget tuning: set env override and compare telemetry deltas

## Telemetry API

Telemetry structs are plain counters:

- `RecordTelemetry`
  - `meshes_total`
  - `meshes_visible`
  - `unique_textures`
  - `draw_commands`
  - `fallback_used`
- `ExecuteTelemetry`
  - `commands_total`
  - `draw_commands`
  - `clear_color_commands`
  - `clear_depth_commands`

Use telemetry-enabled calls:

```rust
engine.record_render_commands_with_telemetry(
    meshes.iter(),
    &mut commands,
    Some(&mut record_telemetry),
)?;

engine.execute_recorded_frame_with_telemetry::<_, 16384>(
    &mut display,
    &mut zbuffer,
    width,
    height,
    &commands,
    Some(&mut execute_telemetry),
)?;
```

Pass `None` in hot paths if you want the smallest possible instrumentation overhead.

## Budget failures

When caps are exceeded, calls return `RenderError` with budget context.

Typical responses:

- Reduce mesh complexity (LOD thresholds, triangle counts)
- Lower visible object count (culling, scene partitioning)
- Increase command buffer capacity if target RAM permits
- Pick a less strict profile for the device class

## Golden regression checks

The integration suite includes deterministic digest tests over rendered output in the record/execute path.
These catch raster behavior regressions before hardware testing.

Run:

```bash
cargo test --tests
```

## CI telemetry budget checks

A CI guard is available at:

- `.github/scripts/check_telemetry_budget.py`

It parses a `CI_TELEMETRY` line and enforces max thresholds for:

- `record_draw_commands`
- `execute_commands_total`
- `execute_draw_commands`

The checker validates all `CI_TELEMETRY`, `CI_TELEMETRY_STRESS`, and `CI_TELEMETRY_FAILSOFT` lines in the input stream (not just the last one), so multiple snapshot tests can be enforced in one CI step.

Threshold resolution order:

1. CLI flags (highest priority)
2. Environment variables
3. Built-in defaults (`3`, `4`, `3`)

Environment variable names:

- `TELEMETRY_MAX_RECORD_DRAW_COMMANDS`
- `TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL`
- `TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS`
- `TELEMETRY_STRESS_MAX_RECORD_DRAW_COMMANDS`
- `TELEMETRY_STRESS_MAX_EXECUTE_COMMANDS_TOTAL`
- `TELEMETRY_STRESS_MAX_EXECUTE_DRAW_COMMANDS`
- `TELEMETRY_FAILSOFT_MAX_RECORD_DRAW_COMMANDS`
- `TELEMETRY_FAILSOFT_MAX_EXECUTE_COMMANDS_TOTAL`
- `TELEMETRY_FAILSOFT_MAX_EXECUTE_DRAW_COMMANDS`
- `TELEMETRY_FAILSOFT_EXPECTED_BUDGET_KIND`

The checker also enforces expected fallback state:

- `CI_TELEMETRY` / `CI_TELEMETRY_STRESS` require `fallback_used=0`
- `CI_TELEMETRY_FAILSOFT` requires `fallback_used=1`
- `CI_TELEMETRY_FAILSOFT` also requires `primary_budget_kind` to match `TELEMETRY_FAILSOFT_EXPECTED_BUDGET_KIND` (default: `MeshesPerFrame`)

Local usage example:

```bash
cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture \
  2>&1 \
  | .github/scripts/check_telemetry_budget.py \
    --max-record-draw-commands 3 \
    --max-execute-commands-total 4 \
    --max-execute-draw-commands 3
```

Environment-driven example:

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

The default CI workflow runs this guard in `.github/workflows/rust.yml`.

## Constrained-profile CI

The workflow also includes an `Embedded Budget` job that validates telemetry under a constrained feature set:

```bash
for feat in row_width_96 row_width_160 row_width_240 row_width_320; do
  cargo check --lib --no-default-features --features "$feat"
  cargo test --test integration_tests test_ci_telemetry_snapshot_ --no-default-features --features "$feat" -- --nocapture 2>&1 \
    | .github/scripts/check_telemetry_budget.py
done
```

## Profile-cap CI matrix

The workflow also validates explicit profile selection through `EMBEDDED_3DGFX_CAPS`:

```bash
for profile in m3 m4 m33 m55; do
  EMBEDDED_3DGFX_CAPS="$profile" \
  cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture 2>&1 \
    | .github/scripts/check_telemetry_budget.py
done
```

## M55 perf budget lane

The workflow also defines a dedicated `m55-perf-budget` lane with profile-specific telemetry limits:

- `TELEMETRY_MAX_RECORD_DRAW_COMMANDS=4`
- `TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL=5`
- `TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS=4`

Local equivalent:

```bash
EMBEDDED_3DGFX_CAPS=m55 \
TELEMETRY_MAX_RECORD_DRAW_COMMANDS=4 \
TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL=5 \
TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS=4 \
cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture 2>&1 \
  | .github/scripts/check_telemetry_budget.py
```

## Triple buffering

Enable with:

```bash
cargo check --lib --features triple-buffering
```

`triple-buffering` exposes `TripleSwapChain` APIs for smoother frame pacing at higher memory cost.

## Fixed transform path

Enable with:

```bash
cargo check --lib --features fixed-transform
```

`fixed-transform` routes projection math through 16.16 fixed-point helpers (`src/fixed_math.rs`) to support non-FPU oriented deployments.

## Hardware profiling hooks

Optional profiling hooks are available for board instrumentation:

```bash
cargo check --lib --no-default-features --features "row_width_240 perfcounter dwt-profiler"
```

Additional trace sink flags:

- `rtt-trace`
- `itm-trace`

See `docs/hardware-profiling.md` for board setup and trace guidance.

## Demo HUD consistency

Telemetry HUD formatting is shared in:

- `examples/shared/perf_hud.rs`

Current telemetry demos:

- `examples/dma_rendering_demo.rs`
- `examples/lod_demo.rs`
