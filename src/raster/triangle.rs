use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::Point;

use crate::ZDepth;
use crate::raster::scanline::EdgeStepper;
use crate::shader::FragmentShader;

/// Screen-space 2D triangle area (cross product).
#[inline(always)]
pub fn tri_area2(a: Point, b: Point, c: Point) -> i32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Generic depth-buffered triangle rasterizer with composable fragment shader.
///
/// Sweeps scanlines top-to-bottom, interpolates depth (16.16 fixed-point or float),
/// and delegates pixel shading to the zero-cost generic `shader`.
pub fn draw_triangle_zbuffered<D, S>(
    points: [Point; 3],
    depths: [f32; 3],
    shader: &S,
    fb: &mut D,
    zbuffer: &mut [ZDepth],
) where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
    S: FragmentShader<Interpolants = ()>,
{
    // Sort vertices by Y coordinate (p1.y <= p2.y <= p3.y)
    let mut indices = [0, 1, 2];
    if points[indices[0]].y > points[indices[1]].y {
        indices.swap(0, 1);
    }
    if points[indices[1]].y > points[indices[2]].y {
        indices.swap(1, 2);
    }
    if points[indices[0]].y > points[indices[1]].y {
        indices.swap(0, 1);
    }

    let p1 = points[indices[0]];
    let p2 = points[indices[1]];
    let p3 = points[indices[2]];

    let z1 = depths[indices[0]];
    let z2 = depths[indices[1]];
    let z3 = depths[indices[2]];

    let total_height = p3.y - p1.y;
    if total_height == 0 {
        return;
    }

    let area = tri_area2(p1, p2, p3);
    if area == 0 {
        return;
    }

    let target_width = fb.bounding_box().size.width as i32;
    let target_height = fb.bounding_box().size.height as i32;

    for y in p1.y..=p3.y {
        if y < 0 || y >= target_height {
            continue;
        }

        let second_half = y > p2.y || p2.y == p1.y;
        let segment_height = if second_half {
            p3.y - p2.y
        } else {
            p2.y - p1.y
        };
        if segment_height == 0 {
            continue;
        }

        let edge1 = EdgeStepper::new(p1, p3, y);
        let edge2 = if second_half {
            EdgeStepper::new(p2, p3, y)
        } else {
            EdgeStepper::new(p1, p2, y)
        };

        let mut x1 = edge1.current_x();
        let mut x2 = edge2.current_x();

        if x1 > x2 {
            core::mem::swap(&mut x1, &mut x2);
        }

        let min_x = x1.max(0);
        let max_x = x2.min(target_width - 1);

        for x in min_x..=max_x {
            // Barycentric depth interpolation
            let w1 = tri_area2(p2, p3, Point::new(x, y));
            let w2 = tri_area2(p3, p1, Point::new(x, y));
            let w3 = tri_area2(p1, p2, Point::new(x, y));

            if area > 0 && (w1 < 0 || w2 < 0 || w3 < 0) {
                continue;
            }
            if area < 0 && (w1 > 0 || w2 > 0 || w3 > 0) {
                continue;
            }

            let inv_area = 1.0 / (area as f32);
            let alpha = (w1 as f32) * inv_area;
            let beta = (w2 as f32) * inv_area;
            let gamma = (w3 as f32) * inv_area;

            let interp_z = alpha * z1 + beta * z2 + gamma * z3;
            if interp_z < 0.0 {
                continue;
            }

            let z_int = crate::to_zdepth((interp_z * 65536.0) as u32);
            let zb_idx = (y as usize) * (target_width as usize) + (x as usize);

            if zb_idx < zbuffer.len() && z_int < zbuffer[zb_idx] {
                zbuffer[zb_idx] = z_int;
                let color = shader.shade(x, y, z_int, ());
                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), color)])
                    .unwrap();
            }
        }
    }
}
