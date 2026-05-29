# Backend Integration Checklist

Use this checklist when bringing up a new board/backend (SPI, LTDC, DMA2D, etc.).

## 1) Platform Profile and Budget Selection

- [ ] Pick a baseline profile from `embedded_3dgfx::config` (`m3`, `m4`, `m33`, `m55`).
- [ ] Confirm target resolution and color format (`Rgb565`).
- [ ] Verify framebuffer + z-buffer fit within available RAM.
- [ ] Verify command-buffer max (`CommandBuffer<N>`) fits frame complexity.

## 2) DrawTarget and Present Path

- [ ] Implement or adapt an `embedded_graphics` `DrawTarget<Color = Rgb565>`.
- [ ] Confirm correct origin/dimensions via `OriginDimensions`.
- [ ] Validate full-frame present path first (no partial update optimization yet).
- [ ] Add a smoke render that draws points, lines, and filled triangles.

## 3) Render Loop Wiring (Record/Execute)

- [ ] Use command-buffer flow in frame loop:
  1. `record_render_commands(...)`
  2. `execute_recorded_frame(...)`
- [ ] Reuse pre-allocated z-buffer and command buffer across frames.
- [ ] Apply caps at startup (`apply_default_caps(...)` or `set_caps(...)`).
- [ ] Verify budget errors are surfaced and logged (`RenderError::OutOfBudget(...)`).

## 4) Performance and Telemetry

- [ ] Capture `RecordTelemetry` and `ExecuteTelemetry` for representative scenes.
- [ ] Confirm fallback behavior for constrained profiles where expected.
- [ ] Record at least one "steady" and one "stress" snapshot for your board.
- [ ] Keep measurements with build flags and profile noted.

## 5) Reliability Checks

- [ ] Run deterministic integration tests locally before hardware pass.
- [ ] Validate no panics under camera movement, culling, and empty-scene cases.
- [ ] Validate over-budget behavior (fallback or typed failure) is deterministic.
- [ ] Confirm render output remains stable across repeated runs.

## 6) Board Smoke Test Definition

- [ ] Add a board smoke command to CI docs (or board-specific automation).
- [ ] Include: startup render, moving camera, and one constrained-budget scene.
- [ ] Define pass/fail signals (console output, telemetry line, screenshot hash).
- [ ] Link this backend row in `docs/compatibility-matrix.md` when validated.

## Example Bring-Up Sequence (Reference)

Use this as a concrete first-pass runbook for a new board:

1. Build a constrained profile:
   - `EMBEDDED_3DGFX_CAPS=m4 cargo check --lib --no-default-features --features row_width_240`
2. Run deterministic telemetry snapshots:
   - `EMBEDDED_3DGFX_CAPS=m4 cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture`
3. Validate telemetry limits:
   - `... | .github/scripts/check_telemetry_budget.py`
4. Run at least one interactive scene on hardware using record/execute path.
5. Capture one "steady" and one "stress" snapshot in board notes.
6. Mark backend status in `docs/compatibility-matrix.md` once repeatable.

## Exit Criteria

Consider backend integration complete when:

- The board renders the canonical command-buffer path.
- Profile/cap limits are configured and validated.
- A repeatable smoke procedure exists with actionable failure signals.
