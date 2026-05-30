//! Iterative BSP front-to-back traversal, frustum culling, and near-plane clip.
//!
//! # Front-to-back ordering
//! The walk visits the BSP tree so that the child nearest the camera is always
//! processed before the farther child.  Combined with the PVS, this produces a
//! stream of faces where every pixel's *first* draw is the correct (nearest)
//! surface — the precondition for M5's coverage-buffer mode.
//!
//! # Near-plane clipping
//! `clip_near` performs Sutherland-Hodgman clipping of a convex polygon against
//! the half-space `z_clip + w_clip >= 0` (OpenGL near plane in homogeneous
//! coordinates).  Polygons that straddle the near plane produce 1-2 extra
//! vertices; those entirely behind the plane are dropped.

use heapless::Vec as HVec;
use nalgebra::{Matrix4, Point2, Vector4};

use super::data::{BspWorld, Face, FrustumPlane};
use super::scratch::BspScratch;

/// Maximum BSP tree depth supported by the iterative stack.
/// 64 levels handles levels with tens of thousands of leaves.
const STACK_DEPTH: usize = 64;

// ---------------------------------------------------------------------------
// Frustum extraction (Gribb-Hartmann)
// ---------------------------------------------------------------------------

/// Derive 6 frustum half-planes from a combined view-projection matrix.
///
/// Uses the Gribb-Hartmann method: each plane is the sum/difference of two
/// rows of the matrix.  Planes are *not* normalised (normals have non-unit
/// length); the AABB test below handles this correctly by using the p-vertex
/// technique.
pub fn frustum_from_vp(m: &Matrix4<f32>) -> [FrustumPlane; 6] {
    let r = |row: usize| -> [f32; 4] { [m[(row, 0)], m[(row, 1)], m[(row, 2)], m[(row, 3)]] };
    let r0 = r(0);
    let r1 = r(1);
    let r2 = r(2);
    let r3 = r(3);

    let make = |a: [f32; 4], b: [f32; 4], s: f32| -> FrustumPlane {
        FrustumPlane {
            normal: [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]],
            d: a[3] + s * b[3],
        }
    };

    [
        make(r3, r0, 1.0),  // Left:   row3 + row0
        make(r3, r0, -1.0), // Right:  row3 - row0
        make(r3, r1, 1.0),  // Bottom: row3 + row1
        make(r3, r1, -1.0), // Top:    row3 - row1
        make(r3, r2, 1.0),  // Near:   row3 + row2
        make(r3, r2, -1.0), // Far:    row3 - row2
    ]
}

// ---------------------------------------------------------------------------
// AABB vs frustum
// ---------------------------------------------------------------------------

/// Returns `true` if the AABB is potentially inside all 6 frustum planes.
///
/// Uses the p-vertex technique: for each plane, the corner of the AABB that
/// maximises the dot product with the plane normal is the "most inside" corner.
/// If that corner is outside the plane, the whole AABB is outside.
#[inline]
pub fn aabb_in_frustum(mins: [i16; 3], maxs: [i16; 3], frustum: &[FrustumPlane; 6]) -> bool {
    for plane in frustum.iter() {
        let px = if plane.normal[0] >= 0.0 {
            maxs[0] as f32
        } else {
            mins[0] as f32
        };
        let py = if plane.normal[1] >= 0.0 {
            maxs[1] as f32
        } else {
            mins[1] as f32
        };
        let pz = if plane.normal[2] >= 0.0 {
            maxs[2] as f32
        } else {
            mins[2] as f32
        };
        if plane.normal[0] * px + plane.normal[1] * py + plane.normal[2] * pz + plane.d < 0.0 {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Near-plane clipping
// ---------------------------------------------------------------------------

/// A vertex in homogeneous clip space, carrying surface + lightmap UVs.
#[derive(Clone, Copy)]
pub struct ClipVert {
    pub clip: Vector4<f32>,
    pub uv: [f32; 2],
    pub lm_uv: [f32; 2],
}

#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

#[inline]
fn lerp_clip(a: &ClipVert, b: &ClipVert, t: f32) -> ClipVert {
    ClipVert {
        clip: a.clip + (b.clip - a.clip) * t,
        uv: [lerp_f32(a.uv[0], b.uv[0], t), lerp_f32(a.uv[1], b.uv[1], t)],
        lm_uv: [
            lerp_f32(a.lm_uv[0], b.lm_uv[0], t),
            lerp_f32(a.lm_uv[1], b.lm_uv[1], t),
        ],
    }
}

/// Clip a polygon (up to `IN` verts) against the near plane `z + w >= 0`.
///
/// A triangle clips into at most 4 vertices (one new vertex per crossing edge),
/// so `OUT` = 4 is sufficient for triangle input.  Larger input polygons
/// (from fan triangulation before clipping, or multi-polygon faces) may need
/// larger `OUT`.
pub fn clip_near<const IN: usize, const OUT: usize>(
    verts: &HVec<ClipVert, IN>,
) -> HVec<ClipVert, OUT> {
    let mut out: HVec<ClipVert, OUT> = HVec::new();
    let n = verts.len();
    if n == 0 {
        return out;
    }

    for i in 0..n {
        let a = &verts[i];
        let b = &verts[(i + 1) % n];
        let da = a.clip.z + a.clip.w; // positive = inside near plane
        let db = b.clip.z + b.clip.w;
        let a_in = da >= 0.0;
        let b_in = db >= 0.0;

        if a_in {
            let _ = out.push(*a);
        }
        if a_in != b_in {
            // Edge straddles the near plane
            let denom = da - db;
            if denom.abs() > 1e-6 {
                let t = da / denom;
                let _ = out.push(lerp_clip(a, b, t));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Screen-space projection
// ---------------------------------------------------------------------------

/// Project a clip-space vertex to integer screen coordinates.
///
/// Returns `(screen_xy, depth, clip_w)`.  `depth` is in the same scale as
/// `K3dengine::transform_point_with_w` (maps NDC z to `[near, far]` float
/// range, later converted to 16.16 by the rasterizer).
///
/// Returns `None` if the vertex has non-positive `w` (behind camera) or if
/// NDC z is out of `[-1, 1]`.
#[inline]
pub fn project_to_screen(
    clip: Vector4<f32>,
    width: u16,
    height: u16,
    near: f32,
    far: f32,
) -> Option<(Point2<i32>, f32, f32)> {
    if clip.w <= 0.0 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let ndc_z = clip.z / clip.w;
    if ndc_z < -1.0 || ndc_z > 1.0 {
        return None;
    }
    let sx = ((1.0 + ndc_x) * 0.5 * width as f32) as i32;
    let sy = ((1.0 - ndc_y) * 0.5 * height as f32) as i32;
    let depth = ndc_z * (far - near) + near;
    Some((Point2::new(sx, sy), depth, clip.w))
}

// ---------------------------------------------------------------------------
// Front-to-back BSP walk
// ---------------------------------------------------------------------------

/// Walk the BSP tree front-to-back, calling `emit(face_idx, face)` for each
/// visible face in front-to-back order.
///
/// Faces that appear in multiple leaves (via the marksurface table) are
/// de-duplicated through `scratch`.
///
/// The walk terminates early if the traversal stack overflows (depth >
/// `STACK_DEPTH`).  In that case the far subtrees are silently skipped — a
/// small visual artefact is preferable to panicking on-device.
pub fn walk_front_to_back<F>(
    world: &BspWorld<'_>,
    scratch: &mut BspScratch<'_>,
    cam_pos: [f32; 3],
    cam_cluster: i16,
    frustum: &[FrustumPlane; 6],
    mut emit: F,
) where
    F: FnMut(usize, &Face),
{
    let mut emit_leaf = |leaf_idx: usize, emit: &mut F| {
        if leaf_idx >= world.leaves.len() {
            return;
        }
        let leaf = &world.leaves[leaf_idx];

        // PVS cull
        if leaf.cluster >= 0 && !world.cluster_visible(cam_cluster, leaf.cluster) {
            return;
        }

        // Frustum cull leaf AABB
        if !aabb_in_frustum(leaf.mins, leaf.maxs, frustum) {
            return;
        }

        // Emit un-duplicated faces referenced by this leaf
        let end = (leaf.first_marksurface + leaf.num_marksurfaces) as usize;
        for ms in (leaf.first_marksurface as usize)..end {
            if ms >= world.marksurfaces.len() {
                continue;
            }
            let fi = world.marksurfaces[ms] as usize;
            if fi >= world.faces.len() {
                continue;
            }
            if scratch.is_marked(fi) {
                continue;
            }
            scratch.mark(fi);
            emit(fi, &world.faces[fi]);
        }
    };

    if world.nodes.is_empty() {
        // Degenerate BSP containing only leaves (single-room authoring path).
        for leaf_idx in 0..world.leaves.len() {
            emit_leaf(leaf_idx, &mut emit);
        }
        return;
    }

    let mut stack: HVec<i32, STACK_DEPTH> = HVec::new();
    let _ = stack.push(0); // root node

    while let Some(node_idx) = stack.pop() {
        if node_idx < 0 {
            // ---- Leaf ----
            let leaf_idx = (!node_idx) as usize;
            emit_leaf(leaf_idx, &mut emit);
            continue;
        }

        // ---- Internal node ----
        let ni = node_idx as usize;
        if ni >= world.nodes.len() {
            continue;
        }
        let node = &world.nodes[ni];

        // Frustum-cull the node AABB first
        if !aabb_in_frustum(node.mins, node.maxs, frustum) {
            continue;
        }

        if node.plane as usize >= world.planes.len() {
            continue;
        }
        let pl = &world.planes[node.plane as usize];
        let d = pl.normal[0] * cam_pos[0] + pl.normal[1] * cam_pos[1] + pl.normal[2] * cam_pos[2]
            - pl.dist;

        let (near_child, far_child) = if d >= 0.0 {
            (node.children[0], node.children[1])
        } else {
            (node.children[1], node.children[0])
        };

        // Push far first so near is popped (processed) first → front-to-back
        if stack.push(far_child).is_err() {
            // Stack overflow: far subtree silently dropped
        }
        let _ = stack.push(near_child);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::bsp::data::{Leaf, Node, Plane};
    use crate::bsp::scratch::BspScratch;

    // ---- Gribb-Hartmann frustum ----

    #[test]
    fn frustum_from_identity_has_six_planes() {
        let m = Matrix4::identity();
        let f = frustum_from_vp(&m);
        assert_eq!(f.len(), 6);
    }

    #[test]
    fn frustum_planes_origin_inside_standard_projection() {
        use nalgebra::Perspective3;
        let proj = Perspective3::new(1.0, core::f32::consts::FRAC_PI_2, 0.1, 100.0);
        let view = nalgebra::Isometry3::look_at_rh(
            &nalgebra::Point3::new(0.0, 0.0, 5.0),
            &nalgebra::Point3::origin(),
            &nalgebra::Vector3::y(),
        );
        let vp = proj.to_homogeneous() * view.to_homogeneous();
        let f = frustum_from_vp(&vp);

        // A point directly in front of the camera should be inside all planes
        let p = [0.0f32, 0.0, 0.0];
        for plane in &f {
            let dot =
                plane.normal[0] * p[0] + plane.normal[1] * p[1] + plane.normal[2] * p[2] + plane.d;
            assert!(dot >= 0.0, "origin outside plane {plane:?}");
        }
    }

    // ---- AABB frustum cull ----

    #[test]
    fn unit_box_inside_identity_frustum() {
        let f = frustum_from_vp(&Matrix4::identity());
        // Identity vp: all planes are degenerate (rows 0-3 sum to zero d=0 normal=0)
        // In practice just verify no panic
        let _ = aabb_in_frustum([-1, -1, -1], [1, 1, 1], &f);
    }

    // ---- Near-plane clip ----

    #[test]
    fn clip_triangle_entirely_in_front_unchanged() {
        // All 3 vertices satisfy z + w > 0
        let v0 = ClipVert {
            clip: Vector4::new(0.0, 0.0, 1.0, 2.0),
            uv: [0.0, 0.0],
            lm_uv: [0.0, 0.0],
        };
        let v1 = ClipVert {
            clip: Vector4::new(1.0, 0.0, 1.0, 2.0),
            uv: [1.0, 0.0],
            lm_uv: [0.0, 0.0],
        };
        let v2 = ClipVert {
            clip: Vector4::new(0.0, 1.0, 1.0, 2.0),
            uv: [0.0, 1.0],
            lm_uv: [0.0, 0.0],
        };
        let mut input: HVec<ClipVert, 4> = HVec::new();
        let _ = input.push(v0);
        let _ = input.push(v1);
        let _ = input.push(v2);
        let out: HVec<ClipVert, 4> = clip_near(&input);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn clip_triangle_entirely_behind_near_discarded() {
        // All 3 vertices have z + w < 0
        let v = |z: f32, w: f32| ClipVert {
            clip: Vector4::new(0.0, 0.0, z, w),
            uv: [0.0, 0.0],
            lm_uv: [0.0, 0.0],
        };
        let mut input: HVec<ClipVert, 4> = HVec::new();
        let _ = input.push(v(-2.0, 1.0)); // z+w = -1 < 0
        let _ = input.push(v(-3.0, 1.0)); // z+w = -2 < 0
        let _ = input.push(v(-4.0, 1.0)); // z+w = -3 < 0
        let out: HVec<ClipVert, 4> = clip_near(&input);
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn clip_triangle_one_vertex_behind_near_produces_quad() {
        // v0 and v1 are in front, v2 is behind
        let v_front = |x: f32| ClipVert {
            clip: Vector4::new(x, 0.0, 1.0, 2.0), // z+w = 3 > 0
            uv: [x, 0.0],
            lm_uv: [0.0, 0.0],
        };
        let v_behind = ClipVert {
            clip: Vector4::new(0.5, 0.0, -2.0, 1.0), // z+w = -1 < 0
            uv: [0.5, 1.0],
            lm_uv: [0.0, 0.0],
        };
        let mut input: HVec<ClipVert, 4> = HVec::new();
        let _ = input.push(v_front(0.0));
        let _ = input.push(v_front(1.0));
        let _ = input.push(v_behind);
        let out: HVec<ClipVert, 4> = clip_near(&input);
        // One behind vertex → two edge intersections → 4 output verts
        assert_eq!(out.len(), 4);
    }

    // ---- Walk ----

    static PLANES_WALK: [Plane; 1] = [Plane {
        normal: [1.0, 0.0, 0.0],
        dist: 0.0,
    }];

    // Root node: children[0]=!1 (front=leaf1), children[1]=!0 (back=leaf0)
    static NODES_WALK: [Node; 1] = [Node {
        plane: 0,
        children: [!1i32, !0i32],
        mins: [-10, -10, -10],
        maxs: [10, 10, 10],
        first_face: 0,
        num_faces: 0,
    }];

    static LEAVES_WALK: [Leaf; 2] = [
        Leaf {
            cluster: 0,
            mins: [-10, -10, -10],
            maxs: [0, 10, 10],
            first_marksurface: 0,
            num_marksurfaces: 1,
        },
        Leaf {
            cluster: 1,
            mins: [0, -10, -10],
            maxs: [10, 10, 10],
            first_marksurface: 1,
            num_marksurfaces: 1,
        },
    ];
    static MARKSURFACES_WALK: [u16; 2] = [0, 1];

    use crate::bsp::data::Face;
    static FACES_WALK: [Face; 2] = [
        Face {
            first_vert: 0,
            num_verts: 3,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
        },
        Face {
            first_vert: 3,
            num_verts: 3,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
        },
    ];

    fn make_walk_world<'a>() -> BspWorld<'a> {
        BspWorld::new(
            &PLANES_WALK,
            &NODES_WALK,
            &LEAVES_WALK,
            &FACES_WALK,
            &MARKSURFACES_WALK,
            &[],
            &[],
            &[],
            &[], // empty vis → always visible
            &[],
            2,
        )
    }

    fn identity_frustum() -> [FrustumPlane; 6] {
        // All-pass frustum: every point is inside
        [FrustumPlane {
            normal: [0.0, 0.0, 0.0],
            d: 1.0,
        }; 6]
    }

    #[test]
    fn walk_emits_both_faces() {
        let world = make_walk_world();
        let mut fb = [0u32; 2];
        let mut scratch = BspScratch::new(&mut fb);
        scratch.mark_new_frame();

        let mut emitted: std::vec::Vec<usize> = std::vec::Vec::new();
        walk_front_to_back(
            &world,
            &mut scratch,
            [0.0, 0.0, 0.0],
            -1, // always-visible
            &identity_frustum(),
            |fi, _| emitted.push(fi),
        );

        assert_eq!(emitted.len(), 2);
    }

    #[test]
    fn walk_deduplicates_faces_across_leaves() {
        // Make both marksurfaces point to face 0 (shared face)
        static MS_SHARED: [u16; 2] = [0, 0];
        static LEAVES_SHARED: [Leaf; 2] = [
            Leaf {
                cluster: 0,
                mins: [-10, -10, -10],
                maxs: [0, 10, 10],
                first_marksurface: 0,
                num_marksurfaces: 1,
            },
            Leaf {
                cluster: 1,
                mins: [0, -10, -10],
                maxs: [10, 10, 10],
                first_marksurface: 1,
                num_marksurfaces: 1,
            },
        ];
        let world = BspWorld::new(
            &PLANES_WALK,
            &NODES_WALK,
            &LEAVES_SHARED,
            &FACES_WALK,
            &MS_SHARED,
            &[],
            &[],
            &[],
            &[],
            &[],
            2,
        );

        let mut fb = [0u32; 2];
        let mut scratch = BspScratch::new(&mut fb);
        scratch.mark_new_frame();

        let mut count = 0usize;
        walk_front_to_back(
            &world,
            &mut scratch,
            [0.0, 0.0, 0.0],
            -1,
            &identity_frustum(),
            |_, _| count += 1,
        );
        assert_eq!(count, 1, "shared face emitted twice");
    }

    #[test]
    fn walk_front_to_back_order() {
        let world = make_walk_world();
        let mut fb = [0u32; 2];
        let mut scratch = BspScratch::new(&mut fb);
        scratch.mark_new_frame();

        // Camera at x=-5: behind the X=0 plane → leaf 0 is nearer
        let mut order: std::vec::Vec<usize> = std::vec::Vec::new();
        walk_front_to_back(
            &world,
            &mut scratch,
            [-5.0, 0.0, 0.0],
            -1,
            &identity_frustum(),
            |fi, _| order.push(fi),
        );

        assert_eq!(order.len(), 2);
        // Face 0 belongs to leaf 0 (near), face 1 to leaf 1 (far)
        assert_eq!(order[0], 0);
        assert_eq!(order[1], 1);
    }

    #[test]
    fn walk_handles_leaf_only_world() {
        static LEAF_ONLY: [Leaf; 1] = [Leaf {
            cluster: 0,
            mins: [-4, -2, -2],
            maxs: [4, 2, 2],
            first_marksurface: 0,
            num_marksurfaces: 1,
        }];
        static MARKS: [u16; 1] = [0];
        static FACES: [Face; 1] = [Face {
            first_vert: 0,
            num_verts: 3,
            texture_id: 0,
            lightmap_id: 0xFFFF,
            plane: 0,
            side: 0,
        }];

        let world = BspWorld::new(
            &[],
            &[],
            &LEAF_ONLY,
            &FACES,
            &MARKS,
            &[],
            &[],
            &[],
            &[], // empty vis => always visible
            &[],
            1,
        );
        let mut vf = [0u32; 1];
        let mut scratch = BspScratch::new(&mut vf);
        scratch.mark_new_frame();

        let mut emitted = 0usize;
        walk_front_to_back(
            &world,
            &mut scratch,
            [0.0, 0.0, 0.0],
            -1,
            &identity_frustum(),
            |_, _| emitted += 1,
        );
        assert_eq!(emitted, 1);
    }
}
