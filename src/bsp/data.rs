//! BSP world data structures.
//!
//! All lump slices borrow from a `&'static` blob produced by `asset_cli bsp`
//! (or embedded via `include_bytes!`) — no heap allocation at runtime.
//!
//! # Node child encoding
//! `Node::children[0]` = front child (positive side of splitting plane).
//! `Node::children[1]` = back child.
//! Values ≥ 0 are internal node indices; values < 0 encode leaf indices as
//! `leaf_idx = !child` (one's complement), matching Quake BSP conventions.

/// An axis-aligned splitting plane: `normal · p = dist`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plane {
    pub normal: [f32; 3],
    /// Distance from origin along normal.
    pub dist: f32,
}

/// Internal BSP node.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// Index into `BspWorld::planes`.
    pub plane: u16,
    /// Child pointers: ≥0 = node index, <0 = `!leaf_index`.
    pub children: [i32; 2],
    /// World-space AABB quantised to i16 for compact storage.
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    /// Faces associated directly with this node (unused by most renderers).
    pub first_face: u16,
    pub num_faces: u16,
}

/// BSP leaf — the camera lives in exactly one leaf at any moment.
#[derive(Clone, Copy, Debug)]
pub struct Leaf {
    /// PVS cluster id.  -1 = no vis data; always visible.
    pub cluster: i16,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    /// Slice into `BspWorld::marksurfaces`.
    pub first_marksurface: u16,
    pub num_marksurfaces: u16,
}

/// One renderable world surface, triangulated into a vertex fan.
///
/// The fan spans `world.vertices[first_vert .. first_vert + num_verts]`.
/// Triangle k (0-based) is `(v0, v[k+1], v[k+2])`.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    /// Start index into `BspWorld::vertices` (and `uvs`, `lm_uvs`).
    pub first_vert: u32,
    /// Number of vertices; the face emits `num_verts - 2` triangles.
    pub num_verts: u16,
    /// Index into the `TextureManager` (surface albedo).
    pub texture_id: u32,
    /// Index into the `TextureManager` for the baked lightmap.
    /// `0xFFFF` = no lightmap — use full-bright shading.
    pub lightmap_id: u16,
    /// Splitting plane this face lies on (for optional back-face skip).
    pub plane: u16,
    /// Non-zero if the face's outward normal faces the *back* of `plane`.
    pub side: u8,
}

/// The complete borrowed BSP world.
///
/// Construct with [`BspWorld::new`] from your static lumps, then pass a
/// `&BspWorld` into [`K3dengine::record_bsp`] every frame.  No allocator needed.
pub struct BspWorld<'a> {
    pub planes: &'a [Plane],
    pub nodes: &'a [Node],
    pub leaves: &'a [Leaf],
    pub faces: &'a [Face],
    /// Indirection table: `leaves[i].first_marksurface` indexes into this.
    pub marksurfaces: &'a [u16],
    pub vertices: &'a [[f32; 3]],
    /// Per-vertex surface UV coordinates (parallel to `vertices`).
    pub uvs: &'a [[f32; 2]],
    /// Per-vertex lightmap UV coordinates (parallel to `vertices`).
    /// May be empty when lightmaps are unused.
    pub lm_uvs: &'a [[f32; 2]],
    /// Quake-style RLE-compressed PVS blob.  Empty = always visible.
    pub vis: &'a [u8],
    /// `vis_offsets[cluster_id]` = byte offset into `vis` for that cluster.
    pub vis_offsets: &'a [u32],
    pub num_clusters: u16,
}

impl<'a> BspWorld<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        planes: &'a [Plane],
        nodes: &'a [Node],
        leaves: &'a [Leaf],
        faces: &'a [Face],
        marksurfaces: &'a [u16],
        vertices: &'a [[f32; 3]],
        uvs: &'a [[f32; 2]],
        lm_uvs: &'a [[f32; 2]],
        vis: &'a [u8],
        vis_offsets: &'a [u32],
        num_clusters: u16,
    ) -> Self {
        Self {
            planes,
            nodes,
            leaves,
            faces,
            marksurfaces,
            vertices,
            uvs,
            lm_uvs,
            vis,
            vis_offsets,
            num_clusters,
        }
    }
}

/// A frustum half-space extracted via Gribb-Hartmann.
///
/// A world point `p` is *inside* this plane when:
/// `normal[0]*p.x + normal[1]*p.y + normal[2]*p.z + d >= 0`.
#[derive(Clone, Copy, Debug)]
pub struct FrustumPlane {
    pub normal: [f32; 3],
    pub d: f32,
}
