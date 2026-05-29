# no_std Frame-Path Architecture

This document describes the frame-path boundaries used to keep rendering deterministic in `no_std` builds.

## Core boundary

- Crate root is `#![no_std]`.
- `std` is only enabled behind the `std` feature.
- Runtime frame APIs (`render`, `record_render_commands*`, `execute_recorded_frame*`) are designed to operate on caller-provided buffers.

## Frame-path memory model

- Command recording writes into bounded `CommandBuffer<N>` (heapless storage).
- Execution writes into caller-provided framebuffer and z-buffer.
- Tile binning uses compile-time bounded `heapless` vectors.
- No runtime allocation APIs are expected in frame functions.

## Guard rails

- CI runs `.github/scripts/check_no_alloc_frame_path.py` to reject obvious heap-allocation patterns in frame-path functions.
- `--no-default-features` checks remain in CI to validate constrained/no_std-style builds.

## Expected usage

1. Pre-allocate command buffer and z-buffer.
2. Record commands.
3. Execute recorded commands (standard, dirty-region, or tiled path).
4. Present full frame or region through backend/swapchain APIs.
