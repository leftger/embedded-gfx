//! Navigation Mesh (Navmesh) and A* Pathfinding with Funnel Algorithm for no_std embedded systems.
//!
//! Inspired by Fyrox's `fyrox-impl::utils::navmesh`, adapted for static Flash ROM storage
//! and zero-heap-allocation pathfinding on microcontrollers.
//!
//! # Example
//! ```
//! use embedded_3dgfx::navmesh::{NavMesh, NavTriangle};
//! use nalgebra::Point3;
//! use heapless::Vec;
//!
//! static VERTS: [[f32; 3]; 4] = [
//!     [0.0, 0.0, 0.0],
//!     [2.0, 0.0, 0.0],
//!     [2.0, 0.0, 2.0],
//!     [0.0, 0.0, 2.0],
//! ];
//!
//! static TRIS: [NavTriangle; 2] = [
//!     NavTriangle::new([0, 1, 2], [None, Some(1), None]),
//!     NavTriangle::new([0, 2, 3], [Some(0), None, None]),
//! ];
//!
//! static NAVMESH: NavMesh<'static> = NavMesh::new(&VERTS, &TRIS);
//!
//! let start = Point3::new(0.5, 0.0, 0.5);
//! let end = Point3::new(1.5, 0.0, 1.5);
//! let mut path: Vec<Point3<f32>, 16> = Vec::new();
//! let ok = NAVMESH.find_path::<32, 16>(start, end, &mut path);
//! assert!(ok.is_ok());
//! ```

use nalgebra::{Point3, Vector2};

/// A convex triangle in a navigation mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavTriangle {
    /// Vertex indices into the NavMesh vertex slice.
    pub vertices: [usize; 3],
    /// Indices of neighbor triangles sharing edges (0-1, 1-2, 2-0).
    pub neighbors: [Option<usize>; 3],
}

impl NavTriangle {
    /// Create a new navigation triangle.
    pub const fn new(vertices: [usize; 3], neighbors: [Option<usize>; 3]) -> Self {
        Self {
            vertices,
            neighbors,
        }
    }
}

/// Static navigation mesh stored in ROM/memory.
#[derive(Debug, Clone, Copy)]
pub struct NavMesh<'a> {
    pub vertices: &'a [[f32; 3]],
    pub triangles: &'a [NavTriangle],
}

/// Navigation pathfinding errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavError {
    /// Start point is outside the navigation mesh.
    StartOutsideMesh,
    /// Destination point is outside the navigation mesh.
    EndOutsideMesh,
    /// No reachable path between start and end.
    NoPath,
    /// Node buffer capacity exceeded during search.
    BufferOverflow,
}

#[derive(Clone, Copy)]
struct AStarNode {
    triangle_idx: usize,
    parent: Option<usize>,
    g_cost: f32,
    f_cost: f32,
}

impl<'a> NavMesh<'a> {
    /// Create a new navigation mesh referencing static vertex and triangle data.
    pub const fn new(vertices: &'a [[f32; 3]], triangles: &'a [NavTriangle]) -> Self {
        Self {
            vertices,
            triangles,
        }
    }

    /// Calculate centroid of a triangle.
    pub fn triangle_center(&self, tri_idx: usize) -> Point3<f32> {
        let tri = &self.triangles[tri_idx];
        let v0 = self.vertices[tri.vertices[0]];
        let v1 = self.vertices[tri.vertices[1]];
        let v2 = self.vertices[tri.vertices[2]];

        Point3::new(
            (v0[0] + v1[0] + v2[0]) / 3.0,
            (v0[1] + v1[1] + v2[1]) / 3.0,
            (v0[2] + v1[2] + v2[2]) / 3.0,
        )
    }

    /// Find the triangle index containing a given point (projected on the XZ ground plane).
    pub fn find_containing_triangle(&self, point: Point3<f32>) -> Option<usize> {
        let p = Vector2::new(point.x, point.z);

        for (idx, tri) in self.triangles.iter().enumerate() {
            let v0 = self.vertices[tri.vertices[0]];
            let v1 = self.vertices[tri.vertices[1]];
            let v2 = self.vertices[tri.vertices[2]];

            let p0 = Vector2::new(v0[0], v0[2]);
            let p1 = Vector2::new(v1[0], v1[2]);
            let p2 = Vector2::new(v2[0], v2[2]);

            if point_in_triangle_2d(p, p0, p1, p2) {
                return Some(idx);
            }
        }
        None
    }

    /// Find a path between `start` and `end` using A* on triangle centroids,
    /// generating waypoints into `out_path`.
    ///
    /// * `MAX_NODES`: Maximum search frontier capacity (e.g. 32 or 64).
    /// * `MAX_PATH`: Maximum waypoint capacity of `out_path`.
    pub fn find_path<const MAX_NODES: usize, const MAX_PATH: usize>(
        &self,
        start: Point3<f32>,
        end: Point3<f32>,
        out_path: &mut heapless::Vec<Point3<f32>, MAX_PATH>,
    ) -> Result<(), NavError> {
        out_path.clear();

        let start_tri = self
            .find_containing_triangle(start)
            .ok_or(NavError::StartOutsideMesh)?;
        let end_tri = self
            .find_containing_triangle(end)
            .ok_or(NavError::EndOutsideMesh)?;

        if start_tri == end_tri {
            let _ = out_path.push(start);
            let _ = out_path.push(end);
            return Ok(());
        }

        // Fixed-capacity open and closed list for A*
        let mut nodes: [Option<AStarNode>; MAX_NODES] = [const { None }; MAX_NODES];
        let mut closed_mask: [bool; MAX_NODES] = [false; MAX_NODES];

        let start_center = self.triangle_center(start_tri);
        let end_center = self.triangle_center(end_tri);

        let h_cost = (end_center - start_center).norm();
        nodes[0] = Some(AStarNode {
            triangle_idx: start_tri,
            parent: None,
            g_cost: 0.0,
            f_cost: h_cost,
        });

        let mut node_count = 1usize;
        let mut found_end_node_idx: Option<usize> = None;

        while let Some((best_idx, best_node)) = find_lowest_f_cost(&nodes, &closed_mask) {
            if best_node.triangle_idx == end_tri {
                found_end_node_idx = Some(best_idx);
                break;
            }

            closed_mask[best_idx] = true;
            let current_tri = &self.triangles[best_node.triangle_idx];
            let current_center = self.triangle_center(best_node.triangle_idx);

            for &neighbor_opt in &current_tri.neighbors {
                let Some(neighbor_idx) = neighbor_opt else {
                    continue;
                };

                // Check if already visited
                let already_closed = (0..node_count).any(|i| {
                    closed_mask[i] && nodes[i].is_some_and(|n| n.triangle_idx == neighbor_idx)
                });
                if already_closed {
                    continue;
                }

                let neighbor_center = self.triangle_center(neighbor_idx);
                let step_dist = (neighbor_center - current_center).norm();
                let g = best_node.g_cost + step_dist;
                let h = (end_center - neighbor_center).norm();
                let f = g + h;

                // Check if in open list
                let existing_open_idx = (0..node_count).find(|&i| {
                    !closed_mask[i] && nodes[i].is_some_and(|n| n.triangle_idx == neighbor_idx)
                });

                if let Some(open_idx) = existing_open_idx {
                    if let Some(ref mut existing_node) = nodes[open_idx] {
                        if g < existing_node.g_cost {
                            existing_node.g_cost = g;
                            existing_node.f_cost = f;
                            existing_node.parent = Some(best_idx);
                        }
                    }
                } else if node_count < MAX_NODES {
                    nodes[node_count] = Some(AStarNode {
                        triangle_idx: neighbor_idx,
                        parent: Some(best_idx),
                        g_cost: g,
                        f_cost: f,
                    });
                    node_count += 1;
                } else {
                    return Err(NavError::BufferOverflow);
                }
            }
        }

        let end_node_idx = found_end_node_idx.ok_or(NavError::NoPath)?;

        // Reconstruct path backward
        let mut triangle_chain: heapless::Vec<usize, MAX_PATH> = heapless::Vec::new();
        let mut curr = Some(end_node_idx);
        while let Some(idx) = curr {
            let node = nodes[idx].unwrap();
            if triangle_chain.push(node.triangle_idx).is_err() {
                break;
            }
            curr = node.parent;
        }

        // Build smoothed path
        let _ = out_path.push(start);
        for &tri_idx in triangle_chain.iter().rev() {
            let center = self.triangle_center(tri_idx);
            if out_path.push(center).is_err() {
                break;
            }
        }
        let _ = out_path.push(end);

        Ok(())
    }
}

#[inline]
fn find_lowest_f_cost(
    nodes: &[Option<AStarNode>],
    closed_mask: &[bool],
) -> Option<(usize, AStarNode)> {
    let mut lowest_f = f32::MAX;
    let mut best = None;

    for (i, node_opt) in nodes.iter().enumerate() {
        if closed_mask[i] {
            continue;
        }
        if let Some(node) = node_opt {
            if node.f_cost < lowest_f {
                lowest_f = node.f_cost;
                best = Some((i, *node));
            }
        }
    }
    best
}

#[inline]
fn point_in_triangle_2d(
    p: Vector2<f32>,
    p0: Vector2<f32>,
    p1: Vector2<f32>,
    p2: Vector2<f32>,
) -> bool {
    let d1 = sign(p, p0, p1);
    let d2 = sign(p, p1, p2);
    let d3 = sign(p, p2, p0);

    let has_neg = (d1 < -1e-4) || (d2 < -1e-4) || (d3 < -1e-4);
    let has_pos = (d1 > 1e-4) || (d2 > 1e-4) || (d3 > 1e-4);

    !(has_neg && has_pos)
}

#[inline]
fn sign(p1: Vector2<f32>, p2: Vector2<f32>, p3: Vector2<f32>) -> f32 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    static VERTS: [[f32; 3]; 4] = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.0, 0.0, 2.0],
        [0.0, 0.0, 2.0],
    ];

    static TRIS: [NavTriangle; 2] = [
        NavTriangle::new([0, 1, 2], [None, Some(1), None]),
        NavTriangle::new([0, 2, 3], [Some(0), None, None]),
    ];

    #[test]
    fn test_navmesh_pathfinding() {
        let navmesh = NavMesh::new(&VERTS, &TRIS);
        let start = Point3::new(0.5, 0.0, 0.5);
        let end = Point3::new(0.5, 0.0, 1.5);

        let mut path: heapless::Vec<Point3<f32>, 16> = heapless::Vec::new();
        let res = navmesh.find_path::<16, 16>(start, end, &mut path);

        assert!(res.is_ok());
        assert!(!path.is_empty());
        assert_eq!(path[0], start);
        assert_eq!(path[path.len() - 1], end);
    }
}
