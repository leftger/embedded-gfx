use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::prelude::Point;

use crate::DrawPrimitive;

#[inline]
pub fn draw<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::Line([p1, p2], color) => {
            fb.draw_iter(
                line_drawing::Bresenham::new((p1.x, p1.y), (p2.x, p2.y))
                    .map(|(x, y)| embedded_graphics_core::Pixel(Point::new(x, y), color)),
            )
            .unwrap();
        }
        DrawPrimitive::ColoredPoint(p, c) => {
            let p = embedded_graphics_core::geometry::Point::new(p.x, p.y);

            fb.draw_iter([embedded_graphics_core::Pixel(p, c)]).unwrap();
        }
        DrawPrimitive::ColoredTriangle(mut vertices, color) => {
            // sort vertices by y using sort_unstable_by
            vertices.as_mut_slice().sort_unstable_by(|a, b| a.y.cmp(&b.y));

            // backface culling: skip triangle if it's not front-facing
            let dx1 = vertices[1].x - vertices[0].x;
            let dy1 = vertices[1].y - vertices[0].y;
            let dx2 = vertices[2].x - vertices[0].x;
            let dy2 = vertices[2].y - vertices[0].y;
            if dx1 * dy2 - dy1 * dx2 <= 0 {
                return;
            }

            let [p1, p2, p3] = [
                Point::new(vertices[0].x, vertices[0].y),
                Point::new(vertices[1].x, vertices[1].y),
                Point::new(vertices[2].x, vertices[2].y),
            ];

            let screen_rect = embedded_graphics_core::primitives::Rectangle::new(Point::new(0, 0), fb.bounding_box().size);
            if !screen_rect.contains(p1) && !screen_rect.contains(p2) && !screen_rect.contains(p3) {
                return;
            }

            fill_triangle(p1, p2, p3, color, fb);
        }
    }
}

fn fill_triangle<D: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: embedded_graphics_core::pixelcolor::Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let area = (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x);
    if area <= 0 {
        return;
    }

    let interpolate_x = |y: i32, p_start: Point, p_end: Point| -> i32 {
        let dy = p_end.y - p_start.y;
        let dx = p_end.x - p_start.x;
        if dy == 0 {
            p_start.x
        } else {
            p_start.x + dx * (y - p_start.y) / dy
        }
    };

    // Top part (p1 to p2)
    if p2.y - p1.y > 0 {
        for y in p1.y..p2.y {
            let ax = interpolate_x(y, p1, p2);
            let bx = interpolate_x(y, p1, p3);
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let pixels = (start_x..=end_x).map(|x| embedded_graphics_core::Pixel(Point::new(x, y), color));
            fb.draw_iter(pixels).unwrap();
        }
    }

    // Bottom part (p2 to p3)
    if p3.y - p2.y > 0 {
        for y in p2.y..=p3.y {
            let ax = interpolate_x(y, p2, p3);
            let bx = interpolate_x(y, p1, p3);
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let pixels = (start_x..=end_x).map(|x| embedded_graphics_core::Pixel(Point::new(x, y), color));
            fb.draw_iter(pixels).unwrap();
        }
    }
}
