# Asset Pipeline and Streaming

This document covers the final asset/streaming stack:

- Blender export add-on (`tools/blender_addon/io_export_embedded_3dgfx.py`)
- offline conversion CLI (`asset_cli`)
- versioned chunked scene format (`E3DS`, version `1`)
- cooperative loader + non-blocking upload API
- CI build-time asset budget report

## Blender Add-on

Located at [`tools/blender_addon/io_export_embedded_3dgfx.py`](../tools/blender_addon/io_export_embedded_3dgfx.py).

Features:
- Direct export to static Rust code (`Geometry`), LOD chains, vertex animations, quantized binary meshes (`.e3dm`), RGB565 textures (`.e3dt`), and chunked scene containers (`.e3dscene`).
- 3D Viewport sidebar with live MCU polygon/ROM budget monitoring (Cortex-M0+, Cortex-M4, Cortex-M7, ESP32).
- One-click material to RGB565 texture baking.

## Offline converter CLI

Workspace member:

- `asset_cli`

Commands:

- `convert-mesh`: quantizes mesh text format into deterministic binary (`.e3dm`)
- `transcode-texture`: transcodes PPM P6 into RGB565 payload (`.e3dt`)
- `pack-scene`: writes chunked scene container (`.e3dscene`)

Examples:

```bash
cargo run -p asset_cli -- convert-mesh --input mesh.txt --output mesh.e3dm --scale 1024
cargo run -p asset_cli -- transcode-texture --input albedo.ppm --output albedo.e3dt
cargo run -p asset_cli -- pack-scene --output scene.e3dscene --chunk mesh:mesh.e3dm --chunk texture:albedo.e3dt
```

## Chunked scene format

Runtime module:

- `src/scene_format.rs`

Container layout:

- magic: `E3DS`
- version: `u16` (currently `1`)
- chunk count: `u16`
- repeated chunks:
  - kind: `u16` (`mesh=1`, `texture=2`, `meta=3`)
  - payload_len: `u32`
  - payload bytes

## Cooperative streaming loader

Runtime module:

- `src/scene_stream.rs`

Key pieces:

- `ResourceUploader` trait for app/backend-specific upload hooks
- `CooperativeChunkLoader` with bounded work per poll
- `UploadStatus::Busy` support for non-blocking upload pipelines

On busy upload, loader emits typed recoverable errors so callers can retry next frame.

## Build-time budget report and enforcement

Script:

- `.github/scripts/asset_budget_report.py`

CI job:

- `asset-budget` in `.github/workflows/rust.yml`

Budget env vars:

- `ASSET_BUDGET_MAX_TOTAL_BYTES`
- `ASSET_BUDGET_MAX_TEXTURE_BYTES`
- `ASSET_BUDGET_MAX_SCENE_BYTES`

The script emits an `ASSET_BUDGET {...}` JSON line and fails CI when limits are exceeded.
