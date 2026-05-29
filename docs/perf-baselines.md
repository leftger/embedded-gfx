# CI Performance Baselines

This document defines the current CI telemetry baseline policy and where it is enforced.

## Scope

Baselines are enforced from deterministic telemetry snapshots emitted by integration tests:

- `CI_TELEMETRY`
- `CI_TELEMETRY_STRESS`
- `CI_TELEMETRY_FAILSOFT`

The checker is `.github/scripts/check_telemetry_budget.py`.

## Baseline Thresholds

Default lanes:

- `CI_TELEMETRY`: `record_draw_commands <= 3`, `execute_commands_total <= 4`, `execute_draw_commands <= 3`
- `CI_TELEMETRY_STRESS`: `record_draw_commands <= 24`, `execute_commands_total <= 25`, `execute_draw_commands <= 24`
- `CI_TELEMETRY_FAILSOFT`: `record_draw_commands <= 1`, `execute_commands_total <= 2`, `execute_draw_commands <= 1`

Expected fallback state:

- `CI_TELEMETRY`, `CI_TELEMETRY_STRESS`: `fallback_used=0`
- `CI_TELEMETRY_FAILSOFT`: `fallback_used=1`
- `CI_TELEMETRY_FAILSOFT`: `primary_budget_kind=MeshesPerFrame` (configurable by env)

M55 perf lane override:

- `record_draw_commands <= 4`
- `execute_commands_total <= 5`
- `execute_draw_commands <= 4`

## Per-Profile Baseline Matrix

| Profile lane | record_draw_commands | execute_commands_total | execute_draw_commands |
|---|---:|---:|---:|
| `m3` | <= 3 | <= 4 | <= 3 |
| `m4` | <= 3 | <= 4 | <= 3 |
| `m33` | <= 3 | <= 4 | <= 3 |
| `m55` (`profile-budget`) | <= 3 | <= 4 | <= 3 |
| `m55-perf-budget` | <= 4 | <= 5 | <= 4 |

Stress and fail-soft expectations apply to all profile lanes:

- `CI_TELEMETRY_STRESS`: `<= 24`, `<= 25`, `<= 24`
- `CI_TELEMETRY_FAILSOFT`: `<= 1`, `<= 2`, `<= 1`, `fallback_used=1`

## Golden Scene Regression Matrix

Canonical deterministic scene tests currently enforced in `tests/integration_tests.rs`:

| Scene class | Test function |
|---|---|
| Points hash | `test_golden_hash_points_scene_record_execute` |
| Lines hash | `test_golden_hash_lines_scene_record_execute` |
| Solid hash | `test_golden_hash_solid_scene_record_execute` |
| Gouraud hash | `test_golden_hash_gouraud_scene_record_execute` |
| Telemetry baseline | `test_ci_telemetry_snapshot_record_execute` |
| Telemetry lines baseline | `test_ci_telemetry_snapshot_lines_record_execute` |
| Telemetry stress baseline | `test_ci_telemetry_snapshot_stress_points_record_execute` |
| Telemetry fail-soft baseline | `test_ci_telemetry_snapshot_failsoft_record_execute` |

## CI Jobs That Enforce Baselines

Defined in `.github/workflows/rust.yml`:

- `test` (default profile)
- `embedded-budget` matrix (`row_width_96/160/240/320`)
- `profile-budget` matrix (`m3/m4/m33/m55`)
- `m55-perf-budget`

## Baseline Update Process

Only update thresholds when there is intentional architectural change.

1. Capture old/new telemetry snapshots in PR notes.
2. Explain why the increase is necessary (or why reduction is expected).
3. Update workflow env thresholds.
4. Keep compatibility docs aligned (`docs/compatibility-matrix.md`, `docs/caps-and-telemetry.md`).

## Review Gate

A telemetry threshold change should not merge without:

- A short rationale,
- A profile impact note, and
- Confirmation that fail-soft semantics remain correct.

PRs that change telemetry thresholds should include the checklist in
`.github/PULL_REQUEST_TEMPLATE.md`.
