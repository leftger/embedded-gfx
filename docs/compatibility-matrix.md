# Compatibility Matrix

This matrix tracks what is currently validated, what is expected to work, and what is still pending hardware confirmation.

Legend:

- `Validated`: covered by CI and/or deterministic integration checks
- `Expected`: should work based on implementation and constraints, not yet formally validated here
- `Planned`: roadmap item not yet implemented/verified

## Build and Feature Profiles

| Profile | Status | Validation Source | Notes |
|---|---|---|---|
| `std` default feature set | Validated | `.github/workflows/rust.yml` (`check`, `test`, `examples`) | Main desktop/simulator validation path |
| `--no-default-features --features row_width_240` | Validated | `.github/workflows/rust.yml` (`embedded-budget` matrix) | Constrained/no_std-style check + clippy + telemetry guard |
| `desktop-unbounded` | Expected | Local/demo usage | Intended for unconstrained desktop profiling |
| Alternate row widths (`row_width_96/160/320`) | Validated | `.github/workflows/rust.yml` (`embedded-budget` matrix) | Per-feature constrained check + clippy + telemetry guard |

## Rendering Pipeline Compatibility

| Capability | Status | Validation Source | Notes |
|---|---|---|---|
| Record/execute command buffer pipeline | Validated | Integration tests + examples | Canonical rendering path |
| Deterministic command recording | Validated | `tests/integration_tests.rs` | Snapshot-style command parity tests |
| Golden-output regression digests | Validated | `tests/integration_tests.rs` | Points + lines digest checks |
| Telemetry counters (record/execute) | Validated | Integration tests + demo HUD | Includes fallback signal |
| Fail-soft fallback APIs | Validated | Integration tests + demos | Explicit/selector/decimation fallback variants |

## CI Budget Guard Coverage

| Snapshot Class | Status | Validation Source | Expected Signals |
|---|---|---|---|
| `CI_TELEMETRY` | Validated | telemetry guard script + tests | `fallback_used=0` |
| `CI_TELEMETRY_STRESS` | Validated | telemetry guard script + tests | `fallback_used=0` |
| `CI_TELEMETRY_FAILSOFT` | Validated | telemetry guard script + tests | `fallback_used=1`, expected budget kind |

## Backends and Runtime Targets

| Target / Backend | Status | Validation Source | Notes |
|---|---|---|---|
| `embedded-graphics-simulator` desktop path | Validated | Examples + CI examples build | Primary interactive demo environment |
| Generic `DrawTarget` execution path | Validated | Integration tests | Core renderer backend-agnostic path |
| Hardware SPI/LTDC/DMA2D board backends | Planned | Roadmap | Needs board-specific implementation and smoke tests |
| Hardware-in-the-loop runs | Planned | Roadmap | Not yet in CI |

## Architecture Coverage

| MCU Class | Status | Validation Source | Notes |
|---|---|---|---|
| Cortex-M33 balanced profile flow | Validated | Default caps + demos/tests + `profile-budget` CI matrix | Current default embedded profile path |
| Cortex-M4 balanced profile flow | Validated | `.github/workflows/rust.yml` (`profile-budget` matrix, `EMBEDDED_3DGFX_CAPS=m4`) | Explicit CI telemetry/budget validation path |
| Cortex-M3 balanced profile flow | Validated | `.github/workflows/rust.yml` (`profile-budget` matrix, `EMBEDDED_3DGFX_CAPS=m3`) | Explicit CI telemetry/budget validation path |
| Cortex-M55 perf profile flow | Validated | `.github/workflows/rust.yml` (`profile-budget` matrix + `m55-perf-budget`) | Explicit CI telemetry/budget validation with profile-specific lane |

## Operational Docs Coverage

| Artifact | Status | Source | Notes |
|---|---|---|---|
| Backend integration checklist | Validated | `docs/backend-integration-checklist.md` | Board/backend bring-up steps and smoke criteria |
| Memory sizing/tuning guide | Validated | `docs/memory-sizing-guide.md` | Buffer formulas, cap sizing, and pre-ship checklist |
| no_std frame-path architecture | Validated | `docs/no-std-architecture.md` | Frame-path allocation boundaries and guard rails |
| CI performance baseline policy | Validated | `docs/perf-baselines.md` | Telemetry thresholds, lanes, and update process |
| Rendering performance evidence | Validated | `docs/rendering-performance-evidence.md` | Dirty-region/tile-bin evidence and buffering impact |
| Hardware profiling guide | Validated | `docs/hardware-profiling.md` | DWT/RTT/ITM hook usage and board profiling flow |
| Hardware smoke workflow guide | Validated | `docs/hardware-smoke-tests.md` | Self-hosted workflow, log markers, and artifact process |
| Asset pipeline and streaming guide | Validated | `docs/asset-pipeline.md` | Offline conversion, chunk format, cooperative loader, and CI budget reporting |

## How to Extend This Matrix

When adding a new backend/profile/board:

1. Add a deterministic test or CI job that exercises it.
2. Add threshold/telemetry assertions where possible.
3. Update this matrix row from `Expected`/`Planned` to `Validated`.
4. Link the validating workflow step or test name in the row notes.
