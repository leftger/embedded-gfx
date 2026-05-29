# Rendering Performance Evidence

This note captures deterministic evidence artifacts used for performance-oriented roadmap items.

## Evidence sources in this repository

- Dirty-region reporting tests:
  - `test_execute_recorded_frame_reports_dirty_region`
  - `test_dirty_region_smaller_than_full_frame_for_small_scene`
- Tile/bin pipeline tests:
  - `test_build_tile_bins_for_recorded_commands`
  - `test_tiled_execute_matches_non_tiled_pixel_count`
- Telemetry baseline tests:
  - `test_ci_telemetry_snapshot_record_execute`
  - `test_ci_telemetry_snapshot_stress_points_record_execute`
  - `test_ci_telemetry_snapshot_failsoft_record_execute`

## What these prove

- Dirty-rect APIs produce bounded update regions for small scenes.
- Tile/bin classification is active and can execute without changing rendered pixel count.
- Telemetry command counts are enforced in CI for standard, stress, and fail-soft lanes.

## Triple-buffer memory impact

At `W x H`, `Rgb565`:

- Single framebuffer: `W * H * 2`
- Double buffering: `2 * W * H * 2`
- Triple buffering: `3 * W * H * 2`

For `240x135`:

- Single: `64,800` bytes
- Double: `129,600` bytes
- Triple: `194,400` bytes

## Fixed-transform error bounds

The fixed-point path uses 16.16 arithmetic (`src/fixed_math.rs`).
Unit tests enforce near-roundtrip fidelity and stable mul/div behavior; pixel-space tolerance target is <= 1 px for typical scene ranges.
