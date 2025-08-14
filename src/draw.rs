#[cfg(feature = "row_width_320")]
const MAX_ROW_WIDTH: usize = 320;
#[cfg(feature = "row_width_240")]
const MAX_ROW_WIDTH: usize = 240;
#[cfg(feature = "row_width_160")]
const MAX_ROW_WIDTH: usize = 160;
#[cfg(not(any(
    feature = "row_width_320",
    feature = "row_width_240",
    feature = "row_width_160"
)))]
const MAX_ROW_WIDTH: usize = 100;

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::prelude::Point;

use crate::DrawPrimitive;

#[inline(always)]
fn is_backfacing(a: Point, b: Point, c: Point) -> bool {
    let dx1 = b.x - a.x;
    let dy1 = b.y - a.y;
    let dx2 = c.x - a.x;
    let dy2 = c.y - a.y;
    dx1 * dy2 - dy1 * dx2 <= 0
}

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
            vertices
                .as_mut_slice()
                .sort_unstable_by(|a, b| a.y.cmp(&b.y));

            // backface culling: skip triangle if it's not front-facing
            let [a, b, c] = [
                Point::new(vertices[0].x, vertices[0].y),
                Point::new(vertices[1].x, vertices[1].y),
                Point::new(vertices[2].x, vertices[2].y),
            ];

            if is_backfacing(a, b, c) {
                return;
            }

            let [p1, p2, p3] = [
                Point::new(vertices[0].x, vertices[0].y),
                Point::new(vertices[1].x, vertices[1].y),
                Point::new(vertices[2].x, vertices[2].y),
            ];

            let screen_rect = embedded_graphics_core::primitives::Rectangle::new(
                Point::new(0, 0),
                fb.bounding_box().size,
            );
            let triangle_bounds =
                embedded_graphics_core::primitives::Rectangle::with_corners(p1, p1.max(p2).max(p3));
            if screen_rect.intersection(&triangle_bounds).is_zero_sized() {
                return;
            }

            fill_triangle(p1, p2, p3, color, fb);
        }
    }
}

struct Interpolator {
    x: i32,
    dx: i32,
    dy: i32,
    error: i32,
}

impl Interpolator {
    fn new(p_start: Point, p_end: Point) -> Self {
        Self {
            x: p_start.x,
            dx: p_end.x - p_start.x,
            dy: p_end.y - p_start.y,
            error: 0,
        }
    }

    fn next(&mut self) -> i32 {
        self.x += self.dx / self.dy;
        self.error += self.dx % self.dy;
        if self.error >= self.dy {
            self.x += 1;
            self.error -= self.dy;
        }
        self.x
    }
}

#[inline(always)]
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

    let bounds = fb.bounding_box();
    let min_x = bounds.top_left.x;
    let max_x = bounds.bottom_right().unwrap().x;

    let mut pixel_row: [embedded_graphics_core::Pixel<embedded_graphics_core::pixelcolor::Rgb565>;
        MAX_ROW_WIDTH] = [embedded_graphics_core::Pixel(
        Point::new(0, 0),
        embedded_graphics_core::pixelcolor::RgbColor::BLACK,
    ); MAX_ROW_WIDTH];

    // Top part (p1 to p2)
    if p2.y - p1.y > 0 {
        let mut a = Interpolator::new(p1, p2);
        let mut b = Interpolator::new(p1, p3);

        for y in p1.y..p2.y {
            let ax = a.next();
            let bx = b.next();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);

            let mut i = 0;
            for x in start_x..=end_x {
                pixel_row[i] = embedded_graphics_core::Pixel(Point::new(x, y), color);
                i += 1;
            }

            fb.draw_iter(pixel_row[..(end_x - start_x + 1) as usize].iter().copied())
                .unwrap();
        }
    }

    // Bottom part (p2 to p3)
    if p3.y - p2.y > 0 {
        let mut a = Interpolator::new(p2, p3);
        let mut b = Interpolator::new(p1, p3);

        for y in p2.y..=p3.y {
            let ax = a.next();
            let bx = b.next();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);

            let mut i = 0;
            for x in start_x..=end_x {
                pixel_row[i] = embedded_graphics_core::Pixel(Point::new(x, y), color);
                i += 1;
            }

            fb.draw_iter(pixel_row[..(end_x - start_x + 1) as usize].iter().copied())
                .unwrap();
        }
    }
}
