# embedded-3dgfx Blender Add-on

A full-featured Blender 3.6+ / 4.x add-on for exporting 3D meshes, LOD chains, vertex animations, RGB565 textures, chunked scenes, and BSP sectors directly into [`embedded-3dgfx`](../../) native formats.

---

## Features

- **Static Mesh Export (`.rs`)**:
  - Automatically transforms Blender coordinate system ($Z$-up, RH) to `embedded-3dgfx` ($Y$-up, RH).
  - Triangulates polygons on-the-fly and bakes unit face normals and unique wireframe edge lines.
  - Optional per-vertex normals (Gouraud shading), UV coordinates, and vertex colors packed as `Rgb565`.
  - Emits zero-allocation, `no_std`-ready `Geometry<'static>` constants.

- **Level of Detail (LOD) Chains (`.rs`)**:
  - Automatically generates High / Medium / Low decimation tiers and generates a ready-to-use `K3dMesh::set_lod()` constructor with `LODLevels`.

- **Vertex Animation (`.rs`)**:
  - Bakes timeline or action keyframes into discrete vertex frames for [`VertexAnimation`](../../src/animation.rs).

- **Quantized Binary Mesh (`.e3dm`)**:
  - Compresses vertex coordinates into fixed-point 16-bit integers (`i16`) and face indices (`u16`) matching `asset_cli` and `scene_stream`.

- **Texture Baking (`.e3dt`, `.rs`)**:
  - Downsamples and bakes active materials or diffuse textures to power-of-two resolutions (32×32, 64×64, 128×128, 256×256) in little-endian packed RGB565.

- **Chunked Scene Container (`.e3dscene`)**:
  - Packs multiple selected meshes and textures into a single `E3DS` v1 chunk container.

- **BSP / Sector Level Export (`.rs`)**:
  - Exports 2D floor boundary loops, floor/ceiling heights, and sector definitions for retro 2.5D rendering.

- **Sidebar Inspector & MCU Budget Monitor**:
  - Live triangle count and vertex count tracking in the 3D Viewport sidebar (`embedded-3dgfx` tab).
  - ROM and RAM footprint estimates.
  - Compatibility badges for target MCU architectures (Cortex-M0+, Cortex-M4, Cortex-M7, ESP32).

---

## Installation

### Option A: Install as an Add-on
1. Open Blender (3.6 or 4.x).
2. Go to **Edit** > **Preferences** > **Add-ons**.
3. Click **Install...** (or the dropdown menu in Blender 4.2+), navigate to `tools/blender_addon/io_export_embedded_3dgfx.py`, and select it.
4. Enable the checkbox for **Import-Export: embedded-3dgfx Asset Exporter**.

### Option B: Quick Script Run
1. Open Blender.
2. Switch to the **Scripting** workspace tab.
3. Open `io_export_embedded_3dgfx.py` and click **Run Script**.

---

## Usage

### 1. Viewport Budget Inspector
Open the 3D Viewport sidebar (press `N` in the 3D Viewport) and select the **embedded-3dgfx** tab:
- Displays live triangle count and estimated ROM footprint for the selected mesh.
- Click **Add Triangulate Modifier** to preview triangulation.
- Click **Bake RGB565 Texture** to bake materials down to 64×64 RGB565.

### 2. Exporting Assets
Go to **File** > **Export** > **embedded-3dgfx (.rs, .e3dm, .e3dt, .e3dscene)**:
- **Export Mode**: Choose `Static Mesh (.rs)`, `LOD Chain (.rs)`, `Vertex Animation (.rs)`, `Binary Mesh (.e3dm)`, `Scene Container (.e3dscene)`, or `BSP / Sectors (.rs)`.
- Configure export parameters (scale, UV export, LOD distances, start/end frames) in the export dialog.
- Click **Export embedded-3dgfx**.
