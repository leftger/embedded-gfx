//! Debug wireframe helpers (AABB / frustum) emitted as line draw primitives.
//!
//! Intended for `std` / simulator builds when tuning culling. All helpers are
//! `no_std`-safe and write into a caller-provided callback or command buffer.

use crate::DrawPrimitive;
use crate::bounds::Aabb;
use crate::camera::Camera;
use embedded_graphics_core::pixelcolor::Rgb565;
use nalgebra::{Matrix4, Point2, Point3, Vector3};

#[allow(unused_imports)]
use nalgebra::ComplexField;

const AABB_EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Emit 12 wireframe edges for a model-space AABB transformed by `model_matrix`.
///
/// Vertices are projected with `vp * model`. Degenerate (behind-camera) edges
/// are skipped.
pub fn emit_aabb_wireframe<F>(
    aabb: &Aabb,
    model_matrix: &Matrix4<f32>,
    vp_matrix: &Matrix4<f32>,
    color: Rgb565,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    let mvp = vp_matrix * model_matrix;
    let corners = aabb.corners();
    let mut screen: [Option<Point2<i32>>; 8] = [None; 8];
    for (i, c) in corners.iter().enumerate() {
        let clip = mvp * nalgebra::Vector4::new(c.x, c.y, c.z, 1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let inv_w = 1.0 / clip.w;
        // NDC → placeholder pixel space is caller-agnostic; use raw NDC*1000
        // style ints only if engine projection already baked screen scale.
        // Prefer transforming like the engine: assume vp already maps to pixels
        // when used with K3dengine (it does — see transform_point).
        screen[i] = Some(Point2::new(
            (clip.x * inv_w) as i32,
            (clip.y * inv_w) as i32,
        ));
    }
    for &(a, b) in &AABB_EDGES {
        if let (Some(pa), Some(pb)) = (screen[a], screen[b]) {
            emit(DrawPrimitive::Line([pa, pb], color));
        }
    }
}

/// Emit AABB wireframe using the engine's screen-space projection path.
pub fn emit_aabb_wireframe_projected<F>(
    aabb: &Aabb,
    model_matrix: &Matrix4<f32>,
    project: impl Fn([f32; 3]) -> Option<Point3<i32>>,
    color: Rgb565,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    let corners = aabb.corners();
    let mut screen: [Option<Point2<i32>>; 8] = [None; 8];
    for (i, c) in corners.iter().enumerate() {
        let world = model_matrix.transform_point(&Point3::new(c.x, c.y, c.z));
        screen[i] = project([world.x, world.y, world.z]).map(|p| p.xy());
    }
    for &(a, b) in &AABB_EDGES {
        if let (Some(pa), Some(pb)) = (screen[a], screen[b]) {
            emit(DrawPrimitive::Line([pa, pb], color));
        }
    }
}

/// Approximate frustum corner rays at `near` and `far` along the view axes,
/// then emit the 12 edges of the view frustum wireframe.
///
/// Uses the camera's FOV/aspect/near/far rather than extracting planes from
/// `vp_matrix`, which keeps the math small and deterministic on MCU.
pub fn emit_frustum_wireframe<F>(
    camera: &Camera,
    project: impl Fn([f32; 3]) -> Option<Point3<i32>>,
    color: Rgb565,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    let aspect = camera.get_aspect_ratio();
    // Reconstruct eye-space frustum corners from near/far and a synthetic FOV.
    // Camera stores fov privately; approximate from projection by sampling.
    // Use a fixed vertical FOV estimate via near-plane height from vp if needed.
    // Fallback: 90° vertical FOV when we cannot read fov — Camera exposes no getter.
    // We use look direction + orthonormal basis from the view matrix.
    let view = camera.view_matrix;
    // view maps world→eye; inverse rotation is view^T for orthonormal view.
    let r = view.fixed_view::<3, 3>(0, 0);
    let right = Vector3::new(r[(0, 0)], r[(0, 1)], r[(0, 2)]);
    let up = Vector3::new(r[(1, 0)], r[(1, 1)], r[(1, 2)]);
    let forward = -Vector3::new(r[(2, 0)], r[(2, 1)], r[(2, 2)]); // camera looks down -Z in eye space

    // Derive half-angles from the camera's vertical FOV.
    let half_v = (camera.fovy() * 0.5).tan();
    let half_h = half_v * aspect;

    let eye = camera.position.coords;
    let mut corners = [Vector3::zeros(); 8];
    for (i, &z) in [camera.near, camera.far].iter().enumerate() {
        let center = eye + forward * z;
        let h = half_h * z;
        let v = half_v * z;
        let base = i * 4;
        corners[base] = center - right * h - up * v;
        corners[base + 1] = center + right * h - up * v;
        corners[base + 2] = center + right * h + up * v;
        corners[base + 3] = center - right * h + up * v;
    }

    let mut screen: [Option<Point2<i32>>; 8] = [None; 8];
    for (i, c) in corners.iter().enumerate() {
        screen[i] = project([c.x, c.y, c.z]).map(|p| p.xy());
    }
    for &(a, b) in &AABB_EDGES {
        if let (Some(pa), Some(pb)) = (screen[a], screen[b]) {
            emit(DrawPrimitive::Line([pa, pb], color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::pixelcolor::WebColors;

    #[test]
    fn aabb_emits_up_to_twelve_lines() {
        let aabb = Aabb::from_min_max(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
        let mut count = 0;
        emit_aabb_wireframe_projected(
            &aabb,
            &Matrix4::identity(),
            |p| Some(Point3::new(p[0] as i32, p[1] as i32, 1)),
            Rgb565::CSS_LIME,
            |_| count += 1,
        );
        assert_eq!(count, 12);
    }
}
