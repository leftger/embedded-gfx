use crate::camera::Camera;
use crate::mesh::K3dMesh;
use crate::primitive::DrawPrimitive;
use nalgebra::ComplexField;
use nalgebra::{Matrix4, Point3, Vector3, Vector4};

#[inline]
pub(crate) fn frustum_planes_from_vp(m: &Matrix4<f32>) -> [[f32; 4]; 6] {
    [
        [
            m[(3, 0)] + m[(0, 0)],
            m[(3, 1)] + m[(0, 1)],
            m[(3, 2)] + m[(0, 2)],
            m[(3, 3)] + m[(0, 3)],
        ],
        [
            m[(3, 0)] - m[(0, 0)],
            m[(3, 1)] - m[(0, 1)],
            m[(3, 2)] - m[(0, 2)],
            m[(3, 3)] - m[(0, 3)],
        ],
        [
            m[(3, 0)] + m[(1, 0)],
            m[(3, 1)] + m[(1, 1)],
            m[(3, 2)] + m[(1, 2)],
            m[(3, 3)] + m[(1, 3)],
        ],
        [
            m[(3, 0)] - m[(1, 0)],
            m[(3, 1)] - m[(1, 1)],
            m[(3, 2)] - m[(1, 2)],
            m[(3, 3)] - m[(1, 3)],
        ],
        [
            m[(3, 0)] + m[(2, 0)],
            m[(3, 1)] + m[(2, 1)],
            m[(3, 2)] + m[(2, 2)],
            m[(3, 3)] + m[(2, 3)],
        ],
        [
            m[(3, 0)] - m[(2, 0)],
            m[(3, 1)] - m[(2, 1)],
            m[(3, 2)] - m[(2, 2)],
            m[(3, 3)] - m[(2, 3)],
        ],
    ]
}

#[inline]
pub(crate) fn should_cull_mesh(camera: &Camera, mesh: &K3dMesh) -> bool {
    #[cfg(feature = "render-layers")]
    if !camera.layers.intersects(mesh.layers) {
        return true;
    }

    #[cfg(feature = "aabb-cull")]
    {
        let aabb = mesh.model_aabb();
        let world_center = mesh
            .model_matrix
            .transform_point(&nalgebra::Point3::from(aabb.center));
        let scale = mesh.similarity.scaling();
        let radius = aabb.radius() * scale;
        let m = camera.vp_matrix;
        let planes = frustum_planes_from_vp(&m);

        for plane in &planes {
            let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
            let len = ComplexField::sqrt(a * a + b * b + c * c);
            if len <= 0.0 {
                continue;
            }
            let dist = (a * world_center.x + b * world_center.y + c * world_center.z + d) / len;
            if dist < -radius {
                return true;
            }
        }
        for plane in &planes {
            let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
            let len = ComplexField::sqrt(a * a + b * b + c * c);
            if len <= 0.0 {
                continue;
            }
            if aabb.plane_signed_overshoot(a, b, c, d, len, &mesh.model_matrix) < 0.0 {
                return true;
            }
        }
        return false;
    }

    #[cfg(not(feature = "aabb-cull"))]
    {
        let mesh_pos = mesh.get_position();
        let radius_sq = mesh.compute_bounding_radius_sq();
        let radius = ComplexField::sqrt(radius_sq);
        let planes = frustum_planes_from_vp(&camera.vp_matrix);
        for plane in &planes {
            let (a, b, c, d) = (plane[0], plane[1], plane[2], plane[3]);
            let len = ComplexField::sqrt(a * a + b * b + c * c);
            if len > 0.0 {
                let dist = (a * mesh_pos.x + b * mesh_pos.y + c * mesh_pos.z + d) / len;
                if dist < -radius {
                    return true;
                }
            }
        }
        false
    }
}

#[inline(always)]
pub(crate) fn transform_point(
    camera: &Camera,
    width: u16,
    height: u16,
    point: &[f32; 3],
    model_matrix: Matrix4<f32>,
) -> Option<Point3<i32>> {
    #[cfg(feature = "fixed-transform")]
    {
        return transform_point_fixed(camera, width, height, point, model_matrix);
    }
    #[cfg(not(feature = "fixed-transform"))]
    {
        let point = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
        let point = model_matrix * point;

        if point.w < 0.0 {
            return None;
        }
        if point.w < camera.near || point.w > camera.far {
            return None;
        }

        let point = Point3::from_homogeneous(point)?;

        let x = ((1.0 + point.x) * 0.5 * width as f32) as i32;
        let y = ((1.0 - point.y) * 0.5 * height as f32) as i32;

        if x < 0 || x >= width as i32 || y < 0 || y >= height as i32 {
            return None;
        }

        Some(Point3::new(
            x,
            y,
            (point.z * (camera.far - camera.near) + camera.near) as i32,
        ))
    }
}

#[cfg(feature = "fixed-transform")]
#[inline(always)]
fn transform_point_fixed(
    camera: &Camera,
    width: u16,
    height: u16,
    point: &[f32; 3],
    model_matrix: Matrix4<f32>,
) -> Option<Point3<i32>> {
    use embedded_dsp::fixed_point::{Q16, from_q16, to_q16};

    #[inline(always)]
    fn div_checked(a: Q16, b: Q16) -> Option<Q16> {
        if b == 0 {
            None
        } else {
            Some(embedded_dsp::fixed_point::div_q16(a, b))
        }
    }

    let point = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
    let point = model_matrix * point;

    if point.w <= 0.0 {
        return None;
    }
    if point.w < camera.near || point.w > camera.far {
        return None;
    }

    let x_fp = div_checked(to_q16(point.x), to_q16(point.w))?;
    let y_fp = div_checked(to_q16(point.y), to_q16(point.w))?;
    let z_fp = div_checked(to_q16(point.z), to_q16(point.w))?;

    let half_w = (width as i32) << 15;
    let half_h = (height as i32) << 15;
    let x = (half_w + ((x_fp as i64 * half_w as i64) >> 16) as i32) >> 16;
    let y = (half_h - ((y_fp as i64 * half_h as i64) >> 16) as i32) >> 16;

    if x < 0 || x >= width as i32 || y < 0 || y >= height as i32 {
        return None;
    }

    let z_ndc = from_q16(z_fp);
    Some(Point3::new(
        x,
        y,
        (z_ndc * (camera.far - camera.near) + camera.near) as i32,
    ))
}

#[inline(always)]
pub(crate) fn transform_points<const N: usize>(
    camera: &Camera,
    width: u16,
    height: u16,
    indices: &[usize; N],
    vertices: &[[f32; 3]],
    model_matrix: Matrix4<f32>,
) -> Option<[Point3<i32>; N]> {
    let mut ret = [Point3::new(0, 0, 0); N];

    for i in 0..N {
        ret[i] = transform_point(camera, width, height, &vertices[indices[i]], model_matrix)?;
    }

    Some(ret)
}

#[inline(always)]
pub(crate) fn transform_point_with_w(
    camera: &Camera,
    width: u16,
    height: u16,
    point: &[f32; 3],
    model_matrix: Matrix4<f32>,
) -> Option<(Point3<i32>, f32)> {
    let v = nalgebra::Vector4::new(point[0], point[1], point[2], 1.0);
    let clip = model_matrix * v;
    if clip.w < camera.near || clip.w > camera.far {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let ndc_z = clip.z / clip.w;
    let x = ((1.0 + ndc_x) * 0.5 * width as f32) as i32;
    let y = ((1.0 - ndc_y) * 0.5 * height as f32) as i32;
    if x < 0 || x >= width as i32 || y < 0 || y >= height as i32 {
        return None;
    }
    let z = (ndc_z * (camera.far - camera.near) + camera.near) as i32;
    Some((Point3::new(x, y, z), clip.w))
}

#[inline(always)]
pub(crate) fn transform_points_with_w<const N: usize>(
    camera: &Camera,
    width: u16,
    height: u16,
    indices: &[usize; N],
    vertices: &[[f32; 3]],
    model_matrix: Matrix4<f32>,
) -> Option<([Point3<i32>; N], [f32; N])> {
    let mut pts = [Point3::new(0, 0, 0); N];
    let mut ws = [1.0f32; N];
    for i in 0..N {
        let (p, w) =
            transform_point_with_w(camera, width, height, &vertices[indices[i]], model_matrix)?;
        pts[i] = p;
        ws[i] = w;
    }
    Some((pts, ws))
}

#[inline]
pub(crate) fn is_backface(
    camera: &Camera,
    face: &[usize; 3],
    vertices: &[[f32; 3]],
    model_matrix: Matrix4<f32>,
    world_normal: &Vector3<f32>,
) -> bool {
    let v0 = vertices[face[0]];
    let v0_world = if model_matrix == Matrix4::identity() {
        Point3::new(v0[0], v0[1], v0[2])
    } else {
        model_matrix.transform_point(&Point3::new(v0[0], v0[1], v0[2]))
    };
    (camera.position - v0_world).dot(world_normal) < 0.0
}

#[inline]
fn clip_to_screen(
    camera: &Camera,
    width: u16,
    height: u16,
    vertex_snap_bits: u8,
    c: Vector4<f32>,
) -> Option<Point3<i32>> {
    if c.w <= 0.0 {
        return None;
    }
    let mut ndc = Point3::from_homogeneous(c)?;
    if vertex_snap_bits > 0 {
        let scale = (1u32 << vertex_snap_bits) as f32;
        ndc.x = ComplexField::round(ndc.x * scale) / scale;
        ndc.y = ComplexField::round(ndc.y * scale) / scale;
    }
    let w = width as f32;
    let h = height as f32;
    let x = ((1.0 + ndc.x) * 0.5 * w).clamp(-w * 8.0, w * 9.0) as i32;
    let y = ((1.0 - ndc.y) * 0.5 * h).clamp(-h * 8.0, h * 9.0) as i32;
    let depth = (ndc.z * (camera.far - camera.near) + camera.near) as i32;
    Some(Point3::new(x, y, depth))
}

#[inline]
fn project_and_emit<F>(
    camera: &Camera,
    width: u16,
    height: u16,
    vertex_snap_bits: u8,
    c0: Vector4<f32>,
    c1: Vector4<f32>,
    c2: Vector4<f32>,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    callback: &mut F,
) where
    F: FnMut(DrawPrimitive),
{
    if let (Some(p0), Some(p1), Some(p2)) = (
        clip_to_screen(camera, width, height, vertex_snap_bits, c0),
        clip_to_screen(camera, width, height, vertex_snap_bits, c1),
        clip_to_screen(camera, width, height, vertex_snap_bits, c2),
    ) {
        callback(DrawPrimitive::ColoredTriangleWithDepth {
            points: [p0.xy(), p1.xy(), p2.xy()],
            depths: [p0.z as f32, p1.z as f32, p2.z as f32],
            color,
        });
    }
}

fn clip_polygon_plane(
    input: &[Vector4<f32>],
    output: &mut [Vector4<f32>; 8],
    dist: impl Fn(Vector4<f32>) -> f32,
) -> usize {
    let n = input.len();
    let mut m = 0usize;
    for i in 0..n {
        let prev = input[(n + i - 1) % n];
        let curr = input[i];
        let d_prev = dist(prev);
        let d_curr = dist(curr);
        if d_curr >= 0.0 {
            if d_prev < 0.0 {
                let t = d_prev / (d_prev - d_curr);
                if m < 8 {
                    output[m] = prev + (curr - prev) * t;
                    m += 1;
                }
            }
            if m < 8 {
                output[m] = curr;
                m += 1;
            }
        } else if d_prev >= 0.0 {
            let t = d_prev / (d_prev - d_curr);
            if m < 8 {
                output[m] = prev + (curr - prev) * t;
                m += 1;
            }
        }
    }
    m
}

pub(crate) fn emit_clipped<F>(
    camera: &Camera,
    width: u16,
    height: u16,
    vertex_snap_bits: u8,
    clip: [Vector4<f32>; 3],
    color: embedded_graphics_core::pixelcolor::Rgb565,
    callback: &mut F,
) where
    F: FnMut(DrawPrimitive),
{
    let nw = camera.near;

    let mut a = [Vector4::zeros(); 8];
    let mut b = [Vector4::zeros(); 8];
    a[0] = clip[0];
    a[1] = clip[1];
    a[2] = clip[2];

    let n = clip_polygon_plane(&a[..3], &mut b, |v| v.w - nw);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane(&b[..n], &mut a, |v| v.x + v.w);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane(&a[..n], &mut b, |v| v.w - v.x);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane(&b[..n], &mut a, |v| v.y + v.w);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane(&a[..n], &mut b, |v| v.w - v.y);
    if n < 3 {
        return;
    }

    for i in 1..n - 1 {
        project_and_emit(
            camera,
            width,
            height,
            vertex_snap_bits,
            b[0],
            b[i],
            b[i + 1],
            color,
            callback,
        );
    }
}
