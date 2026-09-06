//! Procedural 3D mesh generators for low-poly primitives.
//!
//! Inspired by Bevy's `bevy_mesh::primitives::dim3`, adapted for zero-allocation,
//! fixed-capacity embedded execution. All shapes produce precomputed [`Geometry`]
//! views suitable for [`crate::mesh::K3dMesh`].

use crate::mesh::Geometry;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use micromath::F32Ext;

/// A statically-allocated procedural mesh containing vertex positions, triangle faces,
/// face normals, per-vertex normals, and texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProceduralMesh<const V: usize, const F: usize> {
    /// Vertex positions in model space.
    pub vertices: [[f32; 3]; V],
    /// Triangular face indices.
    pub faces: [[usize; 3]; F],
    /// Per-face outward surface normals.
    pub normals: [[f32; 3]; F],
    /// Per-vertex normals (for Gouraud shading).
    pub vertex_normals: [[f32; 3]; V],
    /// UV texture coordinates.
    pub uvs: [[f32; 2]; V],
}

impl<const V: usize, const F: usize> ProceduralMesh<V, F> {
    /// Create a zero-initialized procedural mesh.
    pub const fn empty() -> Self {
        Self {
            vertices: [[0.0; 3]; V],
            faces: [[0; 3]; F],
            normals: [[0.0; 3]; F],
            vertex_normals: [[0.0; 3]; V],
            uvs: [[0.0; 2]; V],
        }
    }

    /// Return a [`Geometry`] view referencing this procedural mesh's buffers.
    #[inline]
    pub fn geometry(&self) -> Geometry<'_> {
        Geometry {
            vertices: &self.vertices,
            faces: &self.faces,
            colors: &[],
            lines: &[],
            normals: &self.normals,
            vertex_normals: &self.vertex_normals,
            uvs: &self.uvs,
            texture_id: None,
        }
    }
}

/// Type alias for a box / cuboid with 24 vertices and 12 triangular faces.
pub type CubeMesh = ProceduralMesh<24, 12>;

/// Type alias for a flat quad plane with 4 vertices and 2 triangular faces.
pub type QuadMesh = ProceduralMesh<4, 2>;

/// Pre-computed unit cube of size 1.0 x 1.0 x 1.0 centered at the origin.
pub const UNIT_CUBE: CubeMesh = cube([0.5, 0.5, 0.5]);

/// Pre-computed unit horizontal plane of size 1.0 x 1.0 on the XZ plane with normal +Y.
pub const UNIT_PLANE: QuadMesh = plane(1.0, 1.0);

/// Construct a cuboid / box mesh with custom half-extents.
///
/// Each of the 6 cube faces has 4 distinct vertices with outward-facing normals
/// and standard `[0.0, 0.0]` to `[1.0, 1.0]` UV mapping.
pub const fn cube(half_extents: [f32; 3]) -> CubeMesh {
    let hx = half_extents[0];
    let hy = half_extents[1];
    let hz = half_extents[2];

    let vertices = [
        // Front face (+Z)
        [-hx, -hy, hz],
        [hx, -hy, hz],
        [hx, hy, hz],
        [-hx, hy, hz],
        // Back face (-Z)
        [hx, -hy, -hz],
        [-hx, -hy, -hz],
        [-hx, hy, -hz],
        [hx, hy, -hz],
        // Right face (+X)
        [hx, -hy, hz],
        [hx, -hy, -hz],
        [hx, hy, -hz],
        [hx, hy, hz],
        // Left face (-X)
        [-hx, -hy, -hz],
        [-hx, -hy, hz],
        [-hx, hy, hz],
        [-hx, hy, -hz],
        // Top face (+Y)
        [-hx, hy, hz],
        [hx, hy, hz],
        [hx, hy, -hz],
        [-hx, hy, -hz],
        // Bottom face (-Y)
        [-hx, -hy, -hz],
        [hx, -hy, -hz],
        [hx, -hy, hz],
        [-hx, -hy, hz],
    ];

    let uvs = [
        // Front
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        // Back
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        // Right
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        // Left
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        // Top
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        // Bottom
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];

    let vertex_normals = [
        // Front
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        // Back
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        // Right
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        // Left
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        // Top
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        // Bottom
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
    ];

    let faces = [
        // Front
        [0, 1, 2],
        [0, 2, 3],
        // Back
        [4, 5, 6],
        [4, 6, 7],
        // Right
        [8, 9, 10],
        [8, 10, 11],
        // Left
        [12, 13, 14],
        [12, 14, 15],
        // Top
        [16, 17, 18],
        [16, 18, 19],
        // Bottom
        [20, 21, 22],
        [20, 22, 23],
    ];

    let normals = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
    ];

    CubeMesh {
        vertices,
        faces,
        normals,
        vertex_normals,
        uvs,
    }
}

/// Construct a horizontal flat quad plane on the XZ plane with normal +Y.
pub const fn plane(width: f32, depth: f32) -> QuadMesh {
    let hw = width * 0.5;
    let hd = depth * 0.5;

    let vertices = [
        [-hw, 0.0, hd],
        [hw, 0.0, hd],
        [hw, 0.0, -hd],
        [-hw, 0.0, -hd],
    ];

    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    let vertex_normals = [[0.0, 1.0, 0.0]; 4];

    let faces = [[0, 1, 2], [0, 2, 3]];

    let normals = [[0.0, 1.0, 0.0]; 2];

    QuadMesh {
        vertices,
        faces,
        normals,
        vertex_normals,
        uvs,
    }
}

/// Low-poly UV sphere (8 longitude segments, 6 latitude rings = 63 vertices, 96 faces).
pub type Sphere8x6 = ProceduralMesh<63, 96>;

/// Medium-poly UV sphere (12 longitude segments, 8 latitude rings = 117 vertices, 192 faces).
pub type Sphere12x8 = ProceduralMesh<117, 192>;

/// High-detail UV sphere (16 longitude segments, 12 latitude rings = 221 vertices, 384 faces).
pub type Sphere16x12 = ProceduralMesh<221, 384>;

/// Generate UV sphere geometry into caller-provided slices.
///
/// * `vertices` must have length at least `(rings + 1) * (segs + 1)`.
/// * `faces` and `normals` must have length at least `2 * rings * segs`.
/// * `vertex_normals` and `uvs` must match `vertices`.
pub fn compute_uv_sphere(
    radius: f32,
    segs: usize,
    rings: usize,
    vertices: &mut [[f32; 3]],
    faces: &mut [[usize; 3]],
    normals: &mut [[f32; 3]],
    vertex_normals: &mut [[f32; 3]],
    uvs: &mut [[f32; 2]],
) {
    let pi = core::f32::consts::PI;
    let two_pi = 2.0 * pi;
    let inv_radius = if radius.abs() > 1e-6 {
        1.0 / radius
    } else {
        1.0
    };

    let mut vi = 0;
    for lat in 0..=rings {
        let theta = pi * (lat as f32) / (rings as f32);
        #[cfg(feature = "std")]
        let (sin_theta, cos_theta) = theta.sin_cos();
        #[cfg(not(feature = "std"))]
        let (sin_theta, cos_theta) = (theta.sin(), theta.cos());

        let y = radius * cos_theta;
        let r_ring = radius * sin_theta;
        let v = (lat as f32) / (rings as f32);

        for lon in 0..=segs {
            let phi = two_pi * (lon as f32) / (segs as f32);
            #[cfg(feature = "std")]
            let (sin_phi, cos_phi) = phi.sin_cos();
            #[cfg(not(feature = "std"))]
            let (sin_phi, cos_phi) = (phi.sin(), phi.cos());

            let x = r_ring * sin_phi;
            let z = r_ring * cos_phi;
            let u = (lon as f32) / (segs as f32);

            if vi < vertices.len() {
                vertices[vi] = [x, y, z];
                vertex_normals[vi] = [x * inv_radius, y * inv_radius, z * inv_radius];
                uvs[vi] = [u, v];
            }
            vi += 1;
        }
    }

    let mut fi = 0;
    for lat in 0..rings {
        for lon in 0..segs {
            let v00 = lat * (segs + 1) + lon;
            let v01 = v00 + 1;
            let v10 = (lat + 1) * (segs + 1) + lon;
            let v11 = v10 + 1;

            if fi < faces.len() {
                faces[fi] = [v00, v10, v01];
                let c0 = vertices[v00];
                let c1 = vertices[v10];
                let c2 = vertices[v01];
                normals[fi] = triangle_normal(c0, c1, c2);
                fi += 1;
            }

            if fi < faces.len() {
                faces[fi] = [v01, v10, v11];
                let c0 = vertices[v01];
                let c1 = vertices[v10];
                let c2 = vertices[v11];
                normals[fi] = triangle_normal(c0, c1, c2);
                fi += 1;
            }
        }
    }
}

/// Construct a low-poly UV sphere (8 segments, 6 rings).
pub fn uv_sphere_8x6(radius: f32) -> Sphere8x6 {
    let mut mesh = Sphere8x6::empty();
    compute_uv_sphere(
        radius,
        8,
        6,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

/// Construct a medium-poly UV sphere (12 segments, 8 rings).
pub fn uv_sphere_12x8(radius: f32) -> Sphere12x8 {
    let mut mesh = Sphere12x8::empty();
    compute_uv_sphere(
        radius,
        12,
        8,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

/// Construct a high-detail UV sphere (16 segments, 12 rings).
pub fn uv_sphere_16x12(radius: f32) -> Sphere16x12 {
    let mut mesh = Sphere16x12::empty();
    compute_uv_sphere(
        radius,
        16,
        12,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

/// Low-poly cylinder (8 segments = 36 vertices, 32 faces).
pub type Cylinder8 = ProceduralMesh<36, 32>;

/// Medium-poly cylinder (12 segments = 52 vertices, 48 faces).
pub type Cylinder12 = ProceduralMesh<52, 48>;

/// High-poly cylinder (16 segments = 68 vertices, 64 faces).
pub type Cylinder16 = ProceduralMesh<68, 64>;

/// Generate cylinder geometry into caller-provided slices.
///
/// Cylinder is centered at origin along the Y axis, extending from `-height/2` to `+height/2`.
/// Includes flat circular top and bottom caps with independent normals.
pub fn compute_cylinder(
    radius: f32,
    height: f32,
    segs: usize,
    vertices: &mut [[f32; 3]],
    faces: &mut [[usize; 3]],
    normals: &mut [[f32; 3]],
    vertex_normals: &mut [[f32; 3]],
    uvs: &mut [[f32; 2]],
) {
    let half_h = height * 0.5;
    let two_pi = 2.0 * core::f32::consts::PI;

    // Layout:
    // Top cap: center vertex (0) + rim vertices (1..=segs) -> (segs + 1)
    // Bottom cap: center vertex + rim vertices -> (segs + 1)
    // Side mantle: top rim (segs + 1) + bottom rim (segs + 1) -> 2 * (segs + 1)
    let top_center_idx = 0;
    let top_rim_start = 1;
    let bot_center_idx = top_rim_start + segs;
    let bot_rim_start = bot_center_idx + 1;
    let side_top_start = bot_rim_start + segs;
    let side_bot_start = side_top_start + (segs + 1);

    // 1. Top cap center
    if top_center_idx < vertices.len() {
        vertices[top_center_idx] = [0.0, half_h, 0.0];
        vertex_normals[top_center_idx] = [0.0, 1.0, 0.0];
        uvs[top_center_idx] = [0.5, 0.5];
    }

    // Top cap rim
    for i in 0..segs {
        let phi = two_pi * (i as f32) / (segs as f32);
        #[cfg(feature = "std")]
        let (sin_phi, cos_phi) = phi.sin_cos();
        #[cfg(not(feature = "std"))]
        let (sin_phi, cos_phi) = (phi.sin(), phi.cos());

        let idx = top_rim_start + i;
        if idx < vertices.len() {
            vertices[idx] = [radius * cos_phi, half_h, radius * sin_phi];
            vertex_normals[idx] = [0.0, 1.0, 0.0];
            uvs[idx] = [0.5 + 0.5 * cos_phi, 0.5 + 0.5 * sin_phi];
        }
    }

    // 2. Bottom cap center
    if bot_center_idx < vertices.len() {
        vertices[bot_center_idx] = [0.0, -half_h, 0.0];
        vertex_normals[bot_center_idx] = [0.0, -1.0, 0.0];
        uvs[bot_center_idx] = [0.5, 0.5];
    }

    // Bottom cap rim
    for i in 0..segs {
        let phi = two_pi * (i as f32) / (segs as f32);
        #[cfg(feature = "std")]
        let (sin_phi, cos_phi) = phi.sin_cos();
        #[cfg(not(feature = "std"))]
        let (sin_phi, cos_phi) = (phi.sin(), phi.cos());

        let idx = bot_rim_start + i;
        if idx < vertices.len() {
            vertices[idx] = [radius * cos_phi, -half_h, radius * sin_phi];
            vertex_normals[idx] = [0.0, -1.0, 0.0];
            uvs[idx] = [0.5 + 0.5 * cos_phi, 0.5 + 0.5 * sin_phi];
        }
    }

    // 3. Side mantle
    for i in 0..=segs {
        let phi = two_pi * (i as f32) / (segs as f32);
        #[cfg(feature = "std")]
        let (sin_phi, cos_phi) = phi.sin_cos();
        #[cfg(not(feature = "std"))]
        let (sin_phi, cos_phi) = (phi.sin(), phi.cos());

        let u = (i as f32) / (segs as f32);
        let norm = [cos_phi, 0.0, sin_phi];

        let top_i = side_top_start + i;
        if top_i < vertices.len() {
            vertices[top_i] = [radius * cos_phi, half_h, radius * sin_phi];
            vertex_normals[top_i] = norm;
            uvs[top_i] = [u, 1.0];
        }

        let bot_i = side_bot_start + i;
        if bot_i < vertices.len() {
            vertices[bot_i] = [radius * cos_phi, -half_h, radius * sin_phi];
            vertex_normals[bot_i] = norm;
            uvs[bot_i] = [u, 0.0];
        }
    }

    let mut fi = 0;
    // Top cap faces (pointing +Y)
    for i in 0..segs {
        let next_i = (i + 1) % segs;
        if fi < faces.len() {
            faces[fi] = [top_center_idx, top_rim_start + next_i, top_rim_start + i];
            normals[fi] = [0.0, 1.0, 0.0];
            fi += 1;
        }
    }

    // Bottom cap faces (pointing -Y)
    for i in 0..segs {
        let next_i = (i + 1) % segs;
        if fi < faces.len() {
            faces[fi] = [bot_center_idx, bot_rim_start + i, bot_rim_start + next_i];
            normals[fi] = [0.0, -1.0, 0.0];
            fi += 1;
        }
    }

    // Side mantle faces
    for i in 0..segs {
        let t0 = side_top_start + i;
        let t1 = side_top_start + i + 1;
        let b0 = side_bot_start + i;
        let b1 = side_bot_start + i + 1;

        if fi < faces.len() {
            faces[fi] = [t0, b0, t1];
            normals[fi] = triangle_normal(vertices[t0], vertices[b0], vertices[t1]);
            fi += 1;
        }

        if fi < faces.len() {
            faces[fi] = [t1, b0, b1];
            normals[fi] = triangle_normal(vertices[t1], vertices[b0], vertices[b1]);
            fi += 1;
        }
    }
}

/// Construct a low-poly cylinder (8 segments).
pub fn cylinder_8(radius: f32, height: f32) -> Cylinder8 {
    let mut mesh = Cylinder8::empty();
    compute_cylinder(
        radius,
        height,
        8,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

/// Construct a medium-poly cylinder (12 segments).
pub fn cylinder_12(radius: f32, height: f32) -> Cylinder12 {
    let mut mesh = Cylinder12::empty();
    compute_cylinder(
        radius,
        height,
        12,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

/// Construct a high-poly cylinder (16 segments).
pub fn cylinder_16(radius: f32, height: f32) -> Cylinder16 {
    let mut mesh = Cylinder16::empty();
    compute_cylinder(
        radius,
        height,
        16,
        &mut mesh.vertices,
        &mut mesh.faces,
        &mut mesh.normals,
        &mut mesh.vertex_normals,
        &mut mesh.uvs,
    );
    mesh
}

fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];

    let nx = u[1] * v[2] - u[2] * v[1];
    let ny = u[2] * v[0] - u[0] * v[2];
    let nz = u[0] * v[1] - u[1] * v[0];

    #[cfg(feature = "std")]
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    #[cfg(not(feature = "std"))]
    let len = (nx * nx + ny * ny + nz * nz).sqrt();

    if len > 1e-6 {
        [nx / len, ny / len, nz / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_cube_validity() {
        let cube = UNIT_CUBE;
        let geom = cube.geometry();
        assert_eq!(geom.vertices.len(), 24);
        assert_eq!(geom.faces.len(), 12);
        assert_eq!(geom.vertex_normals.len(), 24);
        assert_eq!(geom.uvs.len(), 24);
    }

    #[test]
    fn test_unit_plane_validity() {
        let plane = UNIT_PLANE;
        let geom = plane.geometry();
        assert_eq!(geom.vertices.len(), 4);
        assert_eq!(geom.faces.len(), 2);
        assert_eq!(geom.normals[0], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_uv_sphere_8x6() {
        let sphere = uv_sphere_8x6(1.0);
        let geom = sphere.geometry();
        assert_eq!(geom.vertices.len(), 63);
        assert_eq!(geom.faces.len(), 96);
        // Verify radius of all vertices
        for v in geom.vertices {
            let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((r - 1.0).abs() < 1e-4, "Radius was {r}");
        }
    }

    #[test]
    fn test_cylinder_8() {
        let cyl = cylinder_8(0.5, 2.0);
        let geom = cyl.geometry();
        assert_eq!(geom.vertices.len(), 36);
        assert_eq!(geom.faces.len(), 32);
    }

    #[test]
    fn test_cylinder_12_and_16() {
        let cyl12 = cylinder_12(0.5, 2.0);
        let geom12 = cyl12.geometry();
        assert_eq!(geom12.vertices.len(), 52);
        assert_eq!(geom12.faces.len(), 48);

        let cyl16 = cylinder_16(0.5, 2.0);
        let geom16 = cyl16.geometry();
        assert_eq!(geom16.vertices.len(), 68);
        assert_eq!(geom16.faces.len(), 64);
    }

    #[test]
    fn test_uv_sphere_12x8_and_16x12() {
        let sphere12 = uv_sphere_12x8(2.0);
        let geom12 = sphere12.geometry();
        assert!(!geom12.vertices.is_empty());
        assert!(!geom12.faces.is_empty());

        let sphere16 = uv_sphere_16x12(1.5);
        let geom16 = sphere16.geometry();
        assert!(!geom16.vertices.is_empty());
        assert!(!geom16.faces.is_empty());
    }
}
