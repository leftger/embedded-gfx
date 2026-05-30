//! PVS (Potentially Visible Set) queries on a [`BspWorld`].
//!
//! Two operations are provided:
//! - [`BspWorld::leaf_for_point`] — O(log N) camera-leaf location.
//! - [`BspWorld::cluster_visible`] — O(N/8) RLE vis decode to test whether
//!   one cluster can see another.

use super::data::BspWorld;

impl<'a> BspWorld<'a> {
    /// Walk the BSP tree to find which leaf contains world-space point `p`.
    ///
    /// Iterative — zero stack usage beyond the function call frame.
    /// Handles degenerate trees (0 nodes) by returning leaf 0.
    pub fn leaf_for_point(&self, p: [f32; 3]) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }
        let mut node = 0i32;
        while node >= 0 {
            let ni = node as usize;
            if ni >= self.nodes.len() {
                break;
            }
            let n = &self.nodes[ni];
            if n.plane as usize >= self.planes.len() {
                break;
            }
            let pl = &self.planes[n.plane as usize];
            let d = pl.normal[0] * p[0] + pl.normal[1] * p[1] + pl.normal[2] * p[2] - pl.dist;
            node = n.children[if d >= 0.0 { 0 } else { 1 }];
        }
        (!node) as usize
    }

    /// Test whether `test_cluster` is potentially visible from `from_cluster`.
    ///
    /// Returns `true` unconditionally when:
    /// - `from_cluster < 0` (leaf with no PVS — Quake convention: always visible), **or**
    /// - The PVS blob is empty (no vis data compiled — developer/test use), **or**
    /// - `test_cluster == from_cluster` (every cluster sees itself).
    ///
    /// Otherwise decodes Quake-style RLE: the vis row for `from_cluster`
    /// is a stream of non-zero bytes (each byte is 8 cluster bits) and zero
    /// runs encoded as `[0x00, count]` (skips `count * 8` clusters).
    pub fn cluster_visible(&self, from_cluster: i16, test_cluster: i16) -> bool {
        // Always-visible cases
        if from_cluster < 0 {
            return true;
        }
        if self.vis.is_empty() {
            return true;
        }
        if test_cluster < 0 {
            return false;
        }
        if from_cluster == test_cluster {
            return true;
        }

        let fc = from_cluster as usize;
        if fc >= self.vis_offsets.len() {
            return false;
        }

        let mut v = self.vis_offsets[fc] as usize;
        let mut c = 0i32;
        let target = test_cluster as i32;

        while c < self.num_clusters as i32 {
            if v >= self.vis.len() {
                return false;
            }
            if self.vis[v] == 0 {
                // RLE zero-run: next byte = count of zero bytes to skip
                if v + 1 >= self.vis.len() {
                    return false;
                }
                c += 8 * self.vis[v + 1] as i32;
                v += 2;
            } else {
                // Non-zero byte: test each of 8 cluster bits
                let byte = self.vis[v];
                for bit in 0u8..8 {
                    if c == target {
                        return byte & (1 << bit) != 0;
                    }
                    c += 1;
                }
                v += 1;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::bsp::data::{Leaf, Node, Plane};

    // Minimal two-leaf BSP: root node splits on X=0.
    // Plane normal=[1,0,0], dist=0:  d = p.x
    //   d >= 0 → right side (Room B) → children[0] = !1  (leaf 1)
    //   d <  0 → left  side (Room A) → children[1] = !0  (leaf 0)
    static PLANES: [Plane; 1] = [Plane {
        normal: [1.0, 0.0, 0.0],
        dist: 0.0,
    }];
    static NODES: [Node; 1] = [Node {
        plane: 0,
        children: [!1i32, !0i32], // front(d>=0)→leaf1=RoomB, back(d<0)→leaf0=RoomA
        mins: [-5, -2, -3],
        maxs: [5, 2, 3],
        first_face: 0,
        num_faces: 0,
    }];
    static LEAVES: [Leaf; 2] = [
        Leaf {
            cluster: 0,
            mins: [-5, -2, -3],
            maxs: [0, 2, 3],
            first_marksurface: 0,
            num_marksurfaces: 0,
        },
        Leaf {
            cluster: 1,
            mins: [0, -2, -3],
            maxs: [5, 2, 3],
            first_marksurface: 0,
            num_marksurfaces: 0,
        },
    ];

    fn make_world_no_vis<'a>() -> BspWorld<'a> {
        BspWorld::new(
            &PLANES,
            &NODES,
            &LEAVES,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            2,
        )
    }

    #[test]
    fn leaf_for_point_room_a() {
        let w = make_world_no_vis();
        // d = -2 < 0 → children[1] = !0 → leaf 0
        let leaf = w.leaf_for_point([-2.0, 0.0, 0.0]);
        assert_eq!(leaf, 0);
    }

    #[test]
    fn leaf_for_point_room_b() {
        let w = make_world_no_vis();
        // d = 2 >= 0 → children[0] = !1 → leaf 1
        let leaf = w.leaf_for_point([2.0, 0.0, 0.0]);
        assert_eq!(leaf, 1);
    }

    #[test]
    fn leaf_for_point_on_plane_goes_front() {
        let w = make_world_no_vis();
        let leaf = w.leaf_for_point([0.0, 0.0, 0.0]);
        assert_eq!(leaf, 1); // d=0 → front child → leaf 1
    }

    #[test]
    fn cluster_visible_empty_vis_always_true() {
        let w = make_world_no_vis();
        assert!(w.cluster_visible(0, 1));
        assert!(w.cluster_visible(1, 0));
    }

    #[test]
    fn cluster_visible_negative_from_always_true() {
        let w = make_world_no_vis();
        assert!(w.cluster_visible(-1, 0));
        assert!(w.cluster_visible(-1, 99));
    }

    #[test]
    fn cluster_visible_same_cluster() {
        let w = make_world_no_vis();
        assert!(w.cluster_visible(0, 0));
        assert!(w.cluster_visible(1, 1));
    }

    // PVS with 2 clusters, fully visible (no zero runs):
    // Cluster 0 row: byte 0b00000011 → clusters 0 and 1 visible
    // Cluster 1 row: byte 0b00000011 → clusters 0 and 1 visible
    static VIS_FULL: [u8; 2] = [0b00000011, 0b00000011];
    static VIS_OFFSETS_FULL: [u32; 2] = [0, 1];

    fn make_world_full_vis<'a>() -> BspWorld<'a> {
        BspWorld::new(
            &PLANES,
            &NODES,
            &LEAVES,
            &[],
            &[],
            &[],
            &[],
            &[],
            &VIS_FULL,
            &VIS_OFFSETS_FULL,
            2,
        )
    }

    #[test]
    fn cluster_visible_rle_full_vis() {
        let w = make_world_full_vis();
        assert!(w.cluster_visible(0, 0));
        assert!(w.cluster_visible(0, 1));
        assert!(w.cluster_visible(1, 0));
        assert!(w.cluster_visible(1, 1));
    }

    // PVS where cluster 0 can see itself but NOT cluster 1:
    // Cluster 0 row: byte 0b00000001 → only cluster 0 visible
    // Cluster 1 row: byte 0b00000011 → both visible
    static VIS_PARTIAL: [u8; 2] = [0b00000001, 0b00000011];
    static VIS_OFFSETS_PARTIAL: [u32; 2] = [0, 1];

    fn make_world_partial_vis<'a>() -> BspWorld<'a> {
        BspWorld::new(
            &PLANES,
            &NODES,
            &LEAVES,
            &[],
            &[],
            &[],
            &[],
            &[],
            &VIS_PARTIAL,
            &VIS_OFFSETS_PARTIAL,
            2,
        )
    }

    #[test]
    fn cluster_visible_rle_partial_vis() {
        let w = make_world_partial_vis();
        assert!(w.cluster_visible(0, 0)); // same-cluster shortcut
        assert!(!w.cluster_visible(0, 1)); // bit 1 not set in row 0
        assert!(w.cluster_visible(1, 0)); // bit 0 set in row 1
        assert!(w.cluster_visible(1, 1)); // bit 1 set in row 1
    }

    // PVS with RLE zero-run: 16 clusters, row 0 visible only in cluster 0.
    // Encoding: [0x01, 0x00, 0x00] — first byte 0x01 (cluster 0 visible),
    // then RLE run [0x00, 0x01] skips 8 clusters (1..8), rest don't matter.
    // Total clusters: 9, cluster 0 in row 0 visible only.
    static VIS_RLE: [u8; 3] = [0b00000001, 0x00, 0x01];
    static VIS_OFFSETS_RLE: [u32; 1] = [0];

    fn make_world_rle<'a>() -> BspWorld<'a> {
        BspWorld::new(
            &PLANES,
            &NODES,
            &LEAVES,
            &[],
            &[],
            &[],
            &[],
            &[],
            &VIS_RLE,
            &VIS_OFFSETS_RLE,
            9,
        )
    }

    #[test]
    fn cluster_visible_rle_zero_run() {
        let w = make_world_rle();
        assert!(w.cluster_visible(0, 0));
        // Clusters 1-8 should be invisible (zero run covers them)
        assert!(!w.cluster_visible(0, 1));
        assert!(!w.cluster_visible(0, 5));
        assert!(!w.cluster_visible(0, 8));
    }
}
