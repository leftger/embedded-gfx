# Hardware Profiling Guide (DWT + RTT/ITM)

This guide documents cycle-level profiling hooks for embedded targets.

## Features

- `dwt-profiler`: enables DWT cycle-counter sampling hooks.
- `rtt-trace`: emits `RTT_TRACE ...` markers.
- `itm-trace`: emits `ITM_TRACE ...` markers.

Example build:

```bash
cargo check --lib --no-default-features --features "row_width_240 perfcounter dwt-profiler"
```

## How it works

- `PerformanceCounter::start_of_frame()` initializes DWT counter hooks.
- `add_measurement()` records microseconds and cycle deltas where available.
- `print()` records frame-level cycle totals.
- Hooks are implemented in `src/hardware_profile.rs`.

## Board integration notes

- DWT cycle reads are active only on ARM targets with `dwt-profiler`.
- RTT/ITM sinks currently emit textual markers suitable for integration with board logging pipelines.
- On non-ARM or when feature-disabled, hooks degrade safely to no-op cycle sampling.

## Suggested board workflow

1. Flash a profile-constrained build.
2. Capture `perf.frame` and `perf.measurement` traces for steady + stress scenes.
3. Track cycle deltas across commits to detect regressions.
4. Attach trace snippets to PRs that modify rendering hot paths.
