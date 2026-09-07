#![no_std]
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::needless_range_loop,
    clippy::result_unit_err,
    clippy::manual_div_ceil,
    clippy::manual_is_multiple_of,
    clippy::explicit_counter_loop,
    clippy::needless_return,
    clippy::useless_conversion,
    clippy::manual_checked_ops,
    clippy::collapsible_if,
    clippy::new_without_default,
    clippy::doc_overindented_list_items,
    clippy::unnecessary_cast
)]
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "depth-u16")]
pub type ZDepth = u16;
#[cfg(feature = "depth-u16")]
pub const Z_MAX_VALUE: ZDepth = u16::MAX;
#[cfg(feature = "depth-u16")]
pub const DEPTH_EPSILON: ZDepth = 1;

#[cfg(not(feature = "depth-u16"))]
pub type ZDepth = u32;
#[cfg(not(feature = "depth-u16"))]
pub const Z_MAX_VALUE: ZDepth = u32::MAX;
#[cfg(not(feature = "depth-u16"))]
pub const DEPTH_EPSILON: ZDepth = 128;

#[inline(always)]
pub const fn to_zdepth(z: u32) -> ZDepth {
    #[cfg(feature = "depth-u16")]
    {
        (z >> 16) as u16
    }
    #[cfg(not(feature = "depth-u16"))]
    {
        z
    }
}

#[cfg(feature = "dma2d")]
unsafe extern "Rust" {
    #[cfg(feature = "depth-u16")]
    fn dma2d_clear_zbuffer_u16(ptr: *mut u16, len: usize, value: u16);
    #[cfg(not(feature = "depth-u16"))]
    fn dma2d_clear_zbuffer_u32(ptr: *mut u32, len: usize, value: u32);
}

#[inline(always)]
pub fn clear_zbuffer(zbuffer: &mut [ZDepth], value: ZDepth) {
    #[cfg(feature = "dma2d")]
    {
        #[cfg(feature = "depth-u16")]
        unsafe {
            dma2d_clear_zbuffer_u16(zbuffer.as_mut_ptr(), zbuffer.len(), value);
        }
        #[cfg(not(feature = "depth-u16"))]
        unsafe {
            dma2d_clear_zbuffer_u32(zbuffer.as_mut_ptr(), zbuffer.len(), value);
        }
    }
    #[cfg(not(feature = "dma2d"))]
    {
        zbuffer.fill(value);
    }
}

#[allow(unused_imports)]
use crate::mesh::K3dMesh;
#[allow(unused_imports)]
use nalgebra::Vector4;
use nalgebra::{Matrix4, Point3, Vector3};

// ComplexField provides sqrt() for f32 in no_std via libm
// It appears "unused" in tests because tests use std, but it's required for no_std builds
#[allow(unused_imports)]
use nalgebra::ComplexField;

pub mod animation;
#[cfg(feature = "scene")]
pub mod billboard;
pub mod engine;
pub mod primitive;

/// Result of a ray cast against triangle geometry.
#[derive(Debug, Clone, Copy)]
pub struct MeshRayCastHit {
    /// Distance along the ray to the hit point
    pub distance: f32,
    /// Hit point in world space
    pub point: Vector3<f32>,
    /// Face normal (from cross product of edges, not per-vertex normals)
    pub normal: Vector3<f32>,
    /// Index of the triangle face that was hit
    pub face_index: usize,
    /// Barycentric-interpolated UV at the hit point (or [0.0, 0.0] if no UVs present)
    pub uv: [f32; 2],
}

#[cfg(feature = "aabb-cull")]
pub mod bounds;
pub mod bridge;
#[cfg(feature = "raycast")]
pub mod bsp;
pub mod camera;
pub mod camera_controller;
#[cfg(feature = "scene")]
pub mod character;
pub mod color;
pub mod command_buffer;
pub mod completion;
pub mod config;
pub mod display_backend;
pub mod dither;
pub mod draw;
#[cfg(feature = "embassy")]
pub mod embassy;
pub mod error;
#[cfg(feature = "gizmos")]
pub mod gizmos;
pub mod hardware_profile;
#[cfg(feature = "hud")]
pub mod hud;
pub mod input;
pub mod lens_flare;
#[cfg(feature = "lighting")]
pub mod lights;
pub mod lod;
pub mod matcap;
pub mod mesh;
#[cfg(feature = "painters")]
pub mod painters;
#[cfg(feature = "scene")]
pub mod particles;
#[cfg(feature = "perfcounter")]
pub mod perfcounter;
#[cfg(feature = "physics")]
pub mod physics;
pub mod raster;
pub mod ray_primitive;
pub mod raycast;
#[cfg(feature = "render-layers")]
pub mod render_layers;
pub mod renderer;
#[cfg(feature = "gizmos")]
pub mod simplex_stroke_font;
// retro types (palette/tint/stipple) are used by the always-on draw path;
// the heavier `painters` helpers stay opt-in.
pub mod retro;
pub mod shader;
pub mod shapes;
pub mod simd_dsp;

#[cfg(feature = "anim-blend")]
pub mod absm;
pub mod color_gradient;
pub mod curve;
pub mod decal;
pub mod navmesh;
pub mod pool;

pub mod occlusion;
#[cfg(feature = "scene")]
pub mod scene_format;
#[cfg(feature = "scene")]
pub mod scene_stream;
#[cfg(feature = "raycast")]
pub mod sector_lights;
#[cfg(feature = "scene")]
pub mod skeleton;
#[cfg(feature = "physics")]
pub mod softbody;
pub mod state_machine;
pub mod swapchain;
pub mod telemetry;
#[cfg(feature = "textured")]
pub mod texture;
pub mod tilebin;
pub mod timer;
#[cfg(feature = "scene")]
pub mod transform_anim;
#[cfg(feature = "scene")]
pub mod tween;
pub mod view_frustum;

/// Ray-cast against triangle geometry using Möller–Trumbore intersection.
///
/// `ray_origin` and `ray_dir` are in world space. `model_matrix` transforms mesh
/// vertices to world space. Returns the closest hit within `max_distance`, or `None`.
pub fn mesh_ray_cast(
    ray_origin: Vector3<f32>,
    ray_dir: Vector3<f32>,
    geometry: &mesh::Geometry<'_>,
    model_matrix: &Matrix4<f32>,
    max_distance: f32,
) -> Option<MeshRayCastHit> {
    #[cfg(feature = "aabb-cull")]
    {
        return mesh_ray_cast_bounded(
            ray_origin,
            ray_dir,
            geometry,
            model_matrix,
            max_distance,
            None,
        );
    }
    #[cfg(not(feature = "aabb-cull"))]
    {
        mesh_ray_cast_world(ray_origin, ray_dir, geometry, model_matrix, max_distance)
    }
}

#[cfg(not(feature = "aabb-cull"))]
fn mesh_ray_cast_world(
    ray_origin: Vector3<f32>,
    ray_dir: Vector3<f32>,
    geometry: &mesh::Geometry<'_>,
    model_matrix: &Matrix4<f32>,
    max_distance: f32,
) -> Option<MeshRayCastHit> {
    let mut nearest: Option<MeshRayCastHit> = None;
    let mut min_dist = max_distance;

    for (face_index, face) in geometry.faces.iter().enumerate() {
        let raw_v0 = geometry.vertices[face[0]];
        let raw_v1 = geometry.vertices[face[1]];
        let raw_v2 = geometry.vertices[face[2]];

        let v0 = model_matrix
            .transform_point(&Point3::new(raw_v0[0], raw_v0[1], raw_v0[2]))
            .coords;
        let v1 = model_matrix
            .transform_point(&Point3::new(raw_v1[0], raw_v1[1], raw_v1[2]))
            .coords;
        let v2 = model_matrix
            .transform_point(&Point3::new(raw_v2[0], raw_v2[1], raw_v2[2]))
            .coords;

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = ray_dir.cross(&edge2);
        let det = edge1.dot(&h);
        if det.abs() < 1e-6 {
            continue;
        }
        let inv_det = 1.0 / det;
        let s = ray_origin - v0;
        let bary_u = inv_det * s.dot(&h);
        if !(0.0..=1.0).contains(&bary_u) {
            continue;
        }
        let q = s.cross(&edge1);
        let bary_v = inv_det * ray_dir.dot(&q);
        if bary_v < 0.0 || bary_u + bary_v > 1.0 {
            continue;
        }
        let t = inv_det * edge2.dot(&q);
        if t <= 0.0 || t >= min_dist {
            continue;
        }
        let normal = edge1.cross(&edge2).normalize();
        let bary_w = 1.0 - bary_u - bary_v;
        let uv = if geometry.uvs.len() > face[0]
            && geometry.uvs.len() > face[1]
            && geometry.uvs.len() > face[2]
        {
            let uv0 = geometry.uvs[face[0]];
            let uv1 = geometry.uvs[face[1]];
            let uv2 = geometry.uvs[face[2]];
            [
                bary_w * uv0[0] + bary_u * uv1[0] + bary_v * uv2[0],
                bary_w * uv0[1] + bary_u * uv1[1] + bary_v * uv2[1],
            ]
        } else {
            [0.0, 0.0]
        };
        let point = ray_origin + ray_dir * t;
        min_dist = t;
        nearest = Some(MeshRayCastHit {
            distance: t,
            point,
            normal,
            face_index,
            uv,
        });
    }
    nearest
}

/// Like [`mesh_ray_cast`], with an optional model-space AABB broadphase.
/// Requires the `aabb-cull` feature.
#[cfg(feature = "aabb-cull")]
pub fn mesh_ray_cast_bounded(
    ray_origin: Vector3<f32>,
    ray_dir: Vector3<f32>,
    geometry: &mesh::Geometry<'_>,
    model_matrix: &Matrix4<f32>,
    max_distance: f32,
    model_aabb: Option<&bounds::Aabb>,
) -> Option<MeshRayCastHit> {
    let inv = model_matrix.try_inverse()?;
    let origin4 = inv * Vector4::new(ray_origin.x, ray_origin.y, ray_origin.z, 1.0);
    let dir4 = inv * Vector4::new(ray_dir.x, ray_dir.y, ray_dir.z, 0.0);
    if origin4.w.abs() < 1e-8 {
        return None;
    }
    let local_origin = Vector3::new(origin4.x, origin4.y, origin4.z) / origin4.w;
    let local_dir = Vector3::new(dir4.x, dir4.y, dir4.z);
    let dir_len = local_dir.norm();
    if dir_len < 1e-8 {
        return None;
    }
    let local_dir_n = local_dir / dir_len;
    let local_max = max_distance * dir_len;

    if let Some(aabb) = model_aabb
        && aabb
            .intersect_ray(local_origin, local_dir_n, local_max)
            .is_none()
    {
        return None;
    }

    let mut nearest: Option<MeshRayCastHit> = None;
    let mut min_dist = local_max;

    for (face_index, face) in geometry.faces.iter().enumerate() {
        let v0 = Vector3::new(
            geometry.vertices[face[0]][0],
            geometry.vertices[face[0]][1],
            geometry.vertices[face[0]][2],
        );
        let v1 = Vector3::new(
            geometry.vertices[face[1]][0],
            geometry.vertices[face[1]][1],
            geometry.vertices[face[1]][2],
        );
        let v2 = Vector3::new(
            geometry.vertices[face[2]][0],
            geometry.vertices[face[2]][1],
            geometry.vertices[face[2]][2],
        );

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = local_dir_n.cross(&edge2);
        let det = edge1.dot(&h);
        if det.abs() < 1e-6 {
            continue;
        }
        let inv_det = 1.0 / det;
        let s = local_origin - v0;
        let bary_u = inv_det * s.dot(&h);
        if !(0.0..=1.0).contains(&bary_u) {
            continue;
        }
        let q = s.cross(&edge1);
        let bary_v = inv_det * local_dir_n.dot(&q);
        if bary_v < 0.0 || bary_u + bary_v > 1.0 {
            continue;
        }
        let t_local = inv_det * edge2.dot(&q);
        if t_local <= 0.0 || t_local >= min_dist {
            continue;
        }

        let normal_local = edge1.cross(&edge2).normalize();
        let rot = model_matrix.fixed_view::<3, 3>(0, 0);
        let normal = (rot * normal_local).normalize();

        let bary_w = 1.0 - bary_u - bary_v;
        let uv = if geometry.uvs.len() > face[0]
            && geometry.uvs.len() > face[1]
            && geometry.uvs.len() > face[2]
        {
            let uv0 = geometry.uvs[face[0]];
            let uv1 = geometry.uvs[face[1]];
            let uv2 = geometry.uvs[face[2]];
            [
                bary_w * uv0[0] + bary_u * uv1[0] + bary_v * uv2[0],
                bary_w * uv0[1] + bary_u * uv1[1] + bary_v * uv2[1],
            ]
        } else {
            [0.0, 0.0]
        };

        let local_hit = local_origin + local_dir_n * t_local;
        let world_hit = model_matrix
            .transform_point(&Point3::from(local_hit))
            .coords;
        let world_t = (world_hit - ray_origin).norm();
        if world_t >= max_distance {
            continue;
        }

        min_dist = t_local;
        nearest = Some(MeshRayCastHit {
            distance: world_t,
            point: world_hit,
            normal,
            face_index,
            uv,
        });
    }

    nearest
}

/// Convenience: ray-cast a [`K3dMesh`] using its model matrix and cached AABB.
#[cfg(feature = "aabb-cull")]
pub fn mesh_ray_cast_mesh(
    ray_origin: Vector3<f32>,
    ray_dir: Vector3<f32>,
    mesh: &K3dMesh<'_>,
    max_distance: f32,
) -> Option<MeshRayCastHit> {
    let distance = (mesh.get_position() - Point3::from(ray_origin)).norm();
    let geometry = mesh.select_lod(distance);
    let aabb = mesh.model_aabb();
    mesh_ray_cast_bounded(
        ray_origin,
        ray_dir,
        geometry,
        &mesh.model_matrix,
        max_distance,
        Some(&aabb),
    )
}

#[cfg(feature = "dma2d")]
mod dma2d_stubs {
    #[cfg(feature = "depth-u16")]
    #[unsafe(no_mangle)]
    extern "Rust" fn dma2d_clear_zbuffer_u16(ptr: *mut u16, len: usize, value: u16) {
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        slice.fill(value);
    }

    #[cfg(not(feature = "depth-u16"))]
    #[unsafe(no_mangle)]
    extern "Rust" fn dma2d_clear_zbuffer_u32(ptr: *mut u32, len: usize, value: u32) {
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        slice.fill(value);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::engine::K3dengine;
    use crate::primitive::DrawPrimitive;

    #[test]
    fn test_engine_creation() {
        let engine = K3dengine::new(640, 480);
        assert_eq!(engine.width, 640);
        assert_eq!(engine.height, 480);
        assert!((engine.camera.get_aspect_ratio() - 640.0 / 480.0).abs() < 0.001);
    }

    #[test]
    fn test_transform_point_basic() {
        let engine = K3dengine::new(640, 480);
        // Use camera's VP matrix directly
        let transform_matrix = engine.camera.vp_matrix;

        // Point in front of default camera, within view frustum
        // Default camera is at origin looking at origin, so we need a point in front
        let point = [0.0, 0.0, -5.0];
        let result = engine.transform_point(&point, transform_matrix);

        if let Some(transformed) = result {
            // Should be within screen bounds
            assert!(transformed.x >= 0 && transformed.x < 640);
            assert!(transformed.y >= 0 && transformed.y < 480);
        }
        // If None, the point was culled which is also valid behavior
    }

    #[test]
    fn test_transform_point_clamps_out_of_bounds() {
        let engine = K3dengine::new(640, 480);
        let model_matrix = nalgebra::Matrix4::identity();

        // Point way outside the viewport should be clamped/rejected
        let point = [100.0, 100.0, -5.0];
        let result = engine.transform_point(&point, model_matrix);
        // Should return None because coordinates are clamped out
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_point_behind_camera() {
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        // Point with positive z (behind default camera orientation)
        let point = [0.0, 0.0, 1.0];
        let _result = engine.transform_point(&point, transform_matrix);
        // Point behind camera or outside frustum should return None
        // (actual behavior depends on camera setup and projection)
        // This test just verifies the function doesn't panic
    }

    #[test]
    fn test_transform_point_near_plane_clipping() {
        let engine = K3dengine::new(640, 480);
        // Use the camera's real projection, not a raw identity matrix: the
        // near/far check now tests clip-space W (view depth), which is
        // only meaningful under an actual perspective projection -- an
        // identity "model_matrix" leaves W pinned at the homogeneous 1.0
        // regardless of the point's z, so it can't exercise this check.
        let transform_matrix = engine.camera.vp_matrix;

        // Point too close to camera: distance 0.1, before the near=0.4 plane.
        let point = [0.0, 0.0, -0.1];
        let result = engine.transform_point(&point, transform_matrix);
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_point_far_plane_clipping() {
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        // Point too far from camera: distance 1000, beyond the far=20 plane.
        let point = [0.0, 0.0, -1000.0];
        let result = engine.transform_point(&point, transform_matrix);
        assert!(result.is_none());
    }

    #[test]
    fn test_transform_point_within_near_far_not_culled() {
        // Regression test: `transform_point`'s near/far check used to
        // compare pre-divide clip.z against camera.near/far, which for the
        // default near=0.4/far=20 camera silently culled anything closer
        // than roughly 1.17 units -- even though it's well within
        // [near, far]. z=-0.8 falls in that dead zone.
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        let point = [0.0, 0.0, -0.8];
        let result = engine.transform_point(&point, transform_matrix);
        assert!(
            result.is_some(),
            "a point at distance 0.8 (within [near=0.4, far=20]) should not be culled"
        );
    }

    #[test]
    fn test_transform_points_array() {
        let engine = K3dengine::new(640, 480);
        let transform_matrix = engine.camera.vp_matrix;

        let vertices = [[0.0, 0.0, -5.0], [0.1, 0.0, -5.0], [0.0, 0.1, -5.0]];
        let indices = [0, 1, 2];

        let result = engine.transform_points(&indices, &vertices, transform_matrix);

        // If transform succeeds, verify we get 3 points
        if let Some(points) = result {
            assert_eq!(points.len(), 3);
        }
        // If None, one or more points were culled which is valid
    }

    #[test]
    fn test_render_empty_faces_mesh() {
        let engine = K3dengine::new(640, 480);
        let vertices = [[0.0, 0.0, -5.0]]; // At least one vertex required
        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mesh = mesh::K3dMesh::new(geometry);

        let mut callback_count = 0;
        engine.render(std::iter::once(&mesh), |_| {
            callback_count += 1;
        });

        // Mesh with no faces/lines should trigger one point callback (default is Points mode)
        assert!(callback_count > 0);
    }

    #[test]
    fn test_render_points_mode() {
        let engine = K3dengine::new(640, 480);

        let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &[],
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::Points);

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should render points
        assert!(!primitives.is_empty());
        for prim in primitives {
            assert!(matches!(prim, DrawPrimitive::ColoredPoint(_, _)));
        }
    }

    #[test]
    fn test_render_lines_mode_with_faces() {
        let engine = K3dengine::new(640, 480);

        let vertices = [[0.0, 0.0, -5.0], [0.5, 0.0, -5.0], [0.0, 0.5, -5.0]];

        let faces = [[0, 1, 2]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::Lines);

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should render 3 lines (edges of triangle)
        assert_eq!(primitives.len(), 3);
        for prim in primitives {
            assert!(matches!(prim, DrawPrimitive::Line(_, _)));
        }
    }

    #[test]
    #[cfg(feature = "lighting")]
    fn test_render_gouraud_light_dir() {
        let mut engine = K3dengine::new(640, 480);
        engine.camera.set_position(Point3::new(0.0, 0.0, -10.0));
        engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

        let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let faces = [[0, 1, 2]];
        let normals = [[0.0, 0.0, -1.0]]; // face normal pointing toward camera
        let vertex_normals = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];

        let geometry = mesh::Geometry {
            vertices: &vertices,
            faces: &faces,
            colors: &[],
            lines: &[],
            normals: &normals,
            vertex_normals: &vertex_normals,
            uvs: &[],
            texture_id: None,
        };

        let mut mesh = mesh::K3dMesh::new(geometry);
        mesh.set_render_mode(mesh::RenderMode::GouraudLightDir(Vector3::new(
            0.0, 0.0, 1.0,
        )));

        let mut primitives = std::vec::Vec::new();
        engine.render(std::iter::once(&mesh), |prim| {
            primitives.push(prim);
        });

        // Should emit GouraudTriangleWithDepth primitives
        assert!(!primitives.is_empty());
        for prim in &primitives {
            assert!(matches!(
                prim,
                DrawPrimitive::GouraudTriangleWithDepth { .. }
            ));
        }
    }

    /// Verify that Solid-with-normals renders an interior box correctly.
    ///
    /// Places a camera at the centre of a simple box whose face normals point
    /// inward (toward the camera).  The Solid render path must emit at least
    /// one primitive — if backface culling incorrectly fires for all faces
    /// this test will catch it.
    #[test]
    fn test_solid_inward_normals_interior_camera() {
        let mut engine = K3dengine::new(320, 240);
        // Camera inside the box, looking north (–Z).
        engine
            .camera
            .set_position(nalgebra::Point3::new(0.0, 0.0, 0.0));
        engine
            .camera
            .set_target(nalgebra::Point3::new(0.0, 0.0, -1.0));

        // Single north wall: z = –2, vertices form a quad centred on the axis.
        // Inward normal points toward the camera = +Z.
        #[rustfmt::skip]
        let vertices: &[[f32; 3]] = &[
            [-1.0, -1.0, -2.0],
            [ 1.0, -1.0, -2.0],
            [ 1.0,  1.0, -2.0],
            [-1.0,  1.0, -2.0],
        ];
        let faces: &[[usize; 3]] = &[[0, 1, 2], [0, 2, 3]];
        let normals: &[[f32; 3]] = &[[0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]; // inward (+Z)

        let geometry = mesh::Geometry {
            vertices,
            faces,
            normals,
            colors: &[],
            lines: &[],
            vertex_normals: &[],
            uvs: &[],
            texture_id: None,
        };
        let mut m = mesh::K3dMesh::new(geometry);
        m.set_render_mode(mesh::RenderMode::Solid);

        let mut count = 0usize;
        engine.render(std::iter::once(&m), |_| count += 1);

        assert!(
            count > 0,
            "interior Solid-with-inward-normals emitted 0 primitives — culling is wrong"
        );
    }
}
