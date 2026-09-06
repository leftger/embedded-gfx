//! Debug wireframe helpers (AABB / frustum) emitted as line draw primitives.
//!
//! Intended for `std` / simulator builds when tuning culling. All helpers are
//! `no_std`-safe and write into a caller-provided callback or command buffer.

use crate::bounds::Aabb;
use crate::camera::Camera;
use crate::primitive::DrawPrimitive;
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

/// Emit a wireframe grid on the XZ plane centered at `center`.
///
/// * `center`: World-space center point of the grid.
/// * `cell_size`: Distance between adjacent grid lines.
/// * `half_count`: Number of grid cells in positive and negative directions (total lines: `2 * half_count + 1` per axis).
/// * `grid_color`: Line color for standard grid lines.
/// * `axis_color`: Optional highlight color for the primary center axes passing through `center`.
/// * `project`: World-to-screen projection closure.
/// * `emit`: Callback receiving emitted line primitives.
pub fn emit_ground_grid<F>(
    center: Point3<f32>,
    cell_size: f32,
    half_count: i32,
    grid_color: Rgb565,
    axis_color: Option<Rgb565>,
    project: impl Fn([f32; 3]) -> Option<Point3<i32>>,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    let extent = half_count as f32 * cell_size;
    let min_x = center.x - extent;
    let max_x = center.x + extent;
    let min_z = center.z - extent;
    let max_z = center.z + extent;
    let y = center.y;

    // Lines parallel to Z (varying X)
    for i in -half_count..=half_count {
        let x = center.x + i as f32 * cell_size;
        let color = if i == 0 {
            axis_color.unwrap_or(grid_color)
        } else {
            grid_color
        };

        if let (Some(p0), Some(p1)) = (project([x, y, min_z]), project([x, y, max_z])) {
            emit(DrawPrimitive::Line([p0.xy(), p1.xy()], color));
        }
    }

    // Lines parallel to X (varying Z)
    for j in -half_count..=half_count {
        let z = center.z + j as f32 * cell_size;
        let color = if j == 0 {
            axis_color.unwrap_or(grid_color)
        } else {
            grid_color
        };

        if let (Some(p0), Some(p1)) = (project([min_x, y, z]), project([max_x, y, z])) {
            emit(DrawPrimitive::Line([p0.xy(), p1.xy()], color));
        }
    }
}

/// Emit 3D RGB coordinate axes (Red = +X, Green = +Y, Blue = +Z) at `origin`.
///
/// * `origin`: World-space pivot point.
/// * `model_matrix`: Model-to-world transform matrix.
/// * `length`: Length of each axis line.
/// * `project`: World-to-screen projection closure.
/// * `emit`: Callback receiving emitted line primitives.
pub fn emit_transform_axes<F>(
    origin: Point3<f32>,
    model_matrix: &Matrix4<f32>,
    length: f32,
    project: impl Fn([f32; 3]) -> Option<Point3<i32>>,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    let red = Rgb565::new(31, 0, 0);
    let green = Rgb565::new(0, 63, 0);
    let blue = Rgb565::new(0, 0, 31);

    let p_orig = model_matrix.transform_point(&origin);
    let p_x = model_matrix.transform_point(&(origin + Vector3::new(length, 0.0, 0.0)));
    let p_y = model_matrix.transform_point(&(origin + Vector3::new(0.0, length, 0.0)));
    let p_z = model_matrix.transform_point(&(origin + Vector3::new(0.0, 0.0, length)));

    if let (Some(s0), Some(sx)) = (
        project([p_orig.x, p_orig.y, p_orig.z]),
        project([p_x.x, p_x.y, p_x.z]),
    ) {
        emit(DrawPrimitive::Line([s0.xy(), sx.xy()], red));
    }
    if let (Some(s0), Some(sy)) = (
        project([p_orig.x, p_orig.y, p_orig.z]),
        project([p_y.x, p_y.y, p_y.z]),
    ) {
        emit(DrawPrimitive::Line([s0.xy(), sy.xy()], green));
    }
    if let (Some(s0), Some(sz)) = (
        project([p_orig.x, p_orig.y, p_orig.z]),
        project([p_z.x, p_z.y, p_z.z]),
    ) {
        emit(DrawPrimitive::Line([s0.xy(), sz.xy()], blue));
    }
}

/// Emit 2D screen-space text lines using the Hershey simplex stroke font.
///
/// * `text`: ASCII string (characters 32..=126).
/// * `origin`: Screen-space top-left coordinate.
/// * `scale`: Scaling factor applied to glyphs (height ≈ `21.0 * scale` pixels).
/// * `color`: Line color.
/// * `emit`: Callback receiving emitted line primitives.
pub fn emit_stroke_text_2d<F>(
    text: &str,
    origin: Point2<i32>,
    scale: f32,
    color: Rgb565,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    use crate::simplex_stroke_font::{SIMPLEX_CAP_HEIGHT, SIMPLEX_STROKE_FONT};

    let mut cursor_x = origin.x as f32;
    let baseline_y = origin.y as f32 + SIMPLEX_CAP_HEIGHT * scale;

    for c in text.chars() {
        if c == ' ' {
            cursor_x += SIMPLEX_STROKE_FONT.advance as f32 * scale;
            continue;
        }

        if let Some((advance, stroke_range)) = SIMPLEX_STROKE_FONT.get_glyph(c) {
            for s in stroke_range {
                let point_range = SIMPLEX_STROKE_FONT.strokes[s].clone();
                if point_range.len() < 2 {
                    continue;
                }

                let mut prev: Option<Point2<i32>> = None;
                for p_idx in point_range {
                    let [px, py] = SIMPLEX_STROKE_FONT.positions[p_idx];
                    let sx = (cursor_x + px as f32 * scale) as i32;
                    let sy = (baseline_y - py as f32 * scale) as i32;
                    let curr = Point2::new(sx, sy);

                    if let Some(p) = prev {
                        emit(DrawPrimitive::Line([p, curr], color));
                    }
                    prev = Some(curr);
                }
            }
            cursor_x += advance as f32 * scale;
        } else {
            cursor_x += SIMPLEX_STROKE_FONT.advance as f32 * scale;
        }
    }
}

/// Emit 3D text in world space projected to screen lines using the Hershey simplex stroke font.
///
/// Glyphs are placed on the XY plane in world space (advancing along +X, with +Y up)
/// starting at `origin`, and projected to screen space.
///
/// * `text`: ASCII string (characters 32..=126).
/// * `origin`: World-space position of first glyph baseline.
/// * `scale`: World scale factor for glyphs (height in world units ≈ `21.0 * scale`).
/// * `color`: Line color.
/// * `project`: World-to-screen projection closure.
/// * `emit`: Callback receiving emitted line primitives.
pub fn emit_stroke_text_projected<F>(
    text: &str,
    origin: Point3<f32>,
    scale: f32,
    color: Rgb565,
    project: impl Fn([f32; 3]) -> Option<Point3<i32>>,
    mut emit: F,
) where
    F: FnMut(DrawPrimitive),
{
    use crate::simplex_stroke_font::SIMPLEX_STROKE_FONT;

    let mut cursor_x = origin.x;

    for c in text.chars() {
        if c == ' ' {
            cursor_x += SIMPLEX_STROKE_FONT.advance as f32 * scale;
            continue;
        }

        if let Some((advance, stroke_range)) = SIMPLEX_STROKE_FONT.get_glyph(c) {
            for s in stroke_range {
                let point_range = SIMPLEX_STROKE_FONT.strokes[s].clone();
                if point_range.len() < 2 {
                    continue;
                }

                let mut prev: Option<Point2<i32>> = None;
                for p_idx in point_range {
                    let [px, py] = SIMPLEX_STROKE_FONT.positions[p_idx];
                    let wx = cursor_x + px as f32 * scale;
                    let wy = origin.y + py as f32 * scale;
                    let wz = origin.z;

                    let curr = project([wx, wy, wz]).map(|p| p.xy());

                    if let (Some(p), Some(c)) = (prev, curr) {
                        emit(DrawPrimitive::Line([p, c], color));
                    }
                    prev = curr;
                }
            }
            cursor_x += advance as f32 * scale;
        } else {
            cursor_x += SIMPLEX_STROKE_FONT.advance as f32 * scale;
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

    #[test]
    fn test_emit_ground_grid() {
        let mut line_count = 0;
        emit_ground_grid(
            Point3::new(0.0, 0.0, 0.0),
            1.0,
            2,
            Rgb565::CSS_GRAY,
            Some(Rgb565::CSS_WHITE),
            |p| Some(Point3::new(p[0] as i32, p[2] as i32, 1)),
            |_| line_count += 1,
        );
        // 2 * half_count + 1 = 5 lines per axis * 2 axes = 10 lines
        assert_eq!(line_count, 10);
    }

    #[test]
    fn test_emit_transform_axes() {
        let mut line_count = 0;
        emit_transform_axes(
            Point3::new(0.0, 0.0, 0.0),
            &Matrix4::identity(),
            1.0,
            |p| Some(Point3::new(p[0] as i32, p[1] as i32, 1)),
            |_| line_count += 1,
        );
        assert_eq!(line_count, 3);
    }

    #[test]
    fn test_emit_stroke_text_2d() {
        let mut line_count = 0;
        emit_stroke_text_2d(
            "BEVY 3D",
            Point2::new(10, 10),
            1.0,
            Rgb565::CSS_WHITE,
            |_| line_count += 1,
        );
        assert!(line_count > 10);
    }

    #[test]
    fn test_emit_stroke_text_projected() {
        let mut line_count = 0;
        emit_stroke_text_projected(
            "HI",
            Point3::new(0.0, 0.0, 0.0),
            0.1,
            Rgb565::CSS_YELLOW,
            |p| Some(Point3::new((p[0] * 100.0) as i32, (p[1] * 100.0) as i32, 1)),
            |_| line_count += 1,
        );
        assert!(line_count > 0);
    }
}
