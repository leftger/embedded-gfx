//! 2D un-zbuffered triangle filling, polygon clipping, and primitive draw dispatch.

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::Point;
use heapless::Vec;

use super::blend::MAX_ROW_WIDTH;
use crate::primitive::DrawPrimitive;

const FP_SHIFT: i64 = 16;

#[inline(always)]
pub(crate) fn fixed_to_i32(value: i64) -> i32 {
    if value >= 0 {
        (value >> FP_SHIFT) as i32
    } else {
        -((-value) >> FP_SHIFT) as i32
    }
}

pub(crate) struct EdgeStepper {
    pub(crate) x: i64,
    pub(crate) step: i64,
}

impl EdgeStepper {
    pub(crate) fn new(start: Point, end: Point, y: i32) -> Self {
        let dy = (end.y - start.y) as i64;
        let (step, x) = if dy != 0 {
            let s = (((end.x - start.x) as i64) << FP_SHIFT) / dy;
            let x = ((start.x as i64) << FP_SHIFT) + s * (y - start.y) as i64;
            (s, x)
        } else {
            (0, (start.x as i64) << FP_SHIFT)
        };
        Self { x, step }
    }

    #[inline(always)]
    pub(crate) fn current_x(&self) -> i32 {
        fixed_to_i32(self.x)
    }

    #[inline(always)]
    pub(crate) fn advance(&mut self) {
        self.x += self.step;
    }
}

#[inline]
pub fn fill_triangle<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let bounds = fb.bounding_box();
    let min_x = bounds.top_left.x;
    let max_x = bounds.bottom_right().unwrap().x;
    fill_triangle_core(p1, p2, p3, color, min_x, max_x, |chunk| {
        fb.draw_iter(chunk.iter().copied()).unwrap();
    });
}

fn fill_triangle_core<F>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: Rgb565,
    min_x: i32,
    max_x: i32,
    mut emit_chunk: F,
) where
    F: FnMut(&[embedded_graphics_core::Pixel<Rgb565>]),
{
    let area = (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x);
    if area == 0 {
        return;
    }

    let mut pixel_row: [embedded_graphics_core::Pixel<Rgb565>; MAX_ROW_WIDTH] =
        [embedded_graphics_core::Pixel(Point::new(0, 0), RgbColor::BLACK); MAX_ROW_WIDTH];

    if p2.y - p1.y > 0 {
        let mut a = EdgeStepper::new(p1, p2, p1.y);
        let mut b = EdgeStepper::new(p1, p3, p1.y);

        for y in p1.y..p2.y {
            let ax = a.current_x();
            let bx = b.current_x();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);
            let mut x = start_x;
            while x <= end_x {
                let chunk_end = (x + MAX_ROW_WIDTH as i32 - 1).min(end_x);
                let mut i = 0usize;
                for sx in x..=chunk_end {
                    pixel_row[i] = embedded_graphics_core::Pixel(Point::new(sx, y), color);
                    i += 1;
                }
                emit_chunk(&pixel_row[..i]);
                x = chunk_end + 1;
            }
            a.advance();
            b.advance();
        }
    }

    if p3.y - p2.y > 0 {
        let mut a = EdgeStepper::new(p2, p3, p2.y);
        let mut b = EdgeStepper::new(p1, p3, p2.y);

        for y in p2.y..=p3.y {
            let ax = a.current_x();
            let bx = b.current_x();
            let (start_x, end_x) = if ax < bx { (ax, bx) } else { (bx, ax) };
            let start_x = start_x.clamp(min_x, max_x);
            let end_x = end_x.clamp(min_x, max_x);
            let mut x = start_x;
            while x <= end_x {
                let chunk_end = (x + MAX_ROW_WIDTH as i32 - 1).min(end_x);
                let mut i = 0usize;
                for sx in x..=chunk_end {
                    pixel_row[i] = embedded_graphics_core::Pixel(Point::new(sx, y), color);
                    i += 1;
                }
                emit_chunk(&pixel_row[..i]);
                x = chunk_end + 1;
            }
            a.advance();
            b.advance();
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ScreenVert {
    x: f32,
    y: f32,
}

#[inline]
fn clip_polygon_plane_2d(
    input: &[ScreenVert],
    output: &mut [ScreenVert; 8],
    dist: impl Fn(ScreenVert) -> f32,
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
                    output[m] = ScreenVert {
                        x: prev.x + (curr.x - prev.x) * t,
                        y: prev.y + (curr.y - prev.y) * t,
                    };
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
                output[m] = ScreenVert {
                    x: prev.x + (curr.x - prev.x) * t,
                    y: prev.y + (curr.y - prev.y) * t,
                };
                m += 1;
            }
        }
    }
    m
}

#[inline]
fn tri_area2(a: Point, b: Point, c: Point) -> i32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[inline]
fn round_to_i32(v: f32) -> i32 {
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

#[inline]
fn fill_triangle_screen_clipped<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    color: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let bounds = fb.bounding_box();
    let max_x = bounds.size.width.saturating_sub(1) as f32;
    let max_y = bounds.size.height.saturating_sub(1) as f32;
    if max_x < 0.0 || max_y < 0.0 {
        return;
    }

    let mut a = [ScreenVert::default(); 8];
    let mut b = [ScreenVert::default(); 8];
    a[0] = ScreenVert {
        x: p1.x as f32,
        y: p1.y as f32,
    };
    a[1] = ScreenVert {
        x: p2.x as f32,
        y: p2.y as f32,
    };
    a[2] = ScreenVert {
        x: p3.x as f32,
        y: p3.y as f32,
    };

    let n = clip_polygon_plane_2d(&a[..3], &mut b, |v| v.x);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane_2d(&b[..n], &mut a, |v| max_x - v.x);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane_2d(&a[..n], &mut b, |v| v.y);
    if n < 3 {
        return;
    }
    let n = clip_polygon_plane_2d(&b[..n], &mut a, |v| max_y - v.y);
    if n < 3 {
        return;
    }

    for i in 1..n - 1 {
        let t0 = Point::new(round_to_i32(a[0].x), round_to_i32(a[0].y));
        let t1 = Point::new(round_to_i32(a[i].x), round_to_i32(a[i].y));
        let t2 = Point::new(round_to_i32(a[i + 1].x), round_to_i32(a[i + 1].y));
        if tri_area2(t0, t1, t2) == 0 {
            continue;
        }
        fill_triangle(t0, t1, t2, color, fb);
    }
}

/// Draw a single 3D primitive directly onto a target.
#[inline]
pub fn draw<D: DrawTarget<Color = Rgb565>>(primitive: DrawPrimitive, fb: &mut D)
where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::Line([p1, p2], color) => {
            fb.draw_iter(
                crate::raster::Bresenham::new((p1.x, p1.y), (p2.x, p2.y))
                    .map(|(x, y)| embedded_graphics_core::Pixel(Point::new(x, y), color)),
            )
            .unwrap();
        }

        DrawPrimitive::ColoredPoint(p, c) => {
            let p = Point::new(p.x, p.y);
            fb.draw_iter([embedded_graphics_core::Pixel(p, c)]).unwrap();
        }
        DrawPrimitive::ColoredTriangle(mut vertices, color) => {
            vertices.as_mut_slice().sort_unstable_by_key(|a| a.y);
            let [p1, p2, p3] = [
                Point::new(vertices[0].x, vertices[0].y),
                Point::new(vertices[1].x, vertices[1].y),
                Point::new(vertices[2].x, vertices[2].y),
            ];
            fill_triangle_screen_clipped(p1, p2, p3, color, fb);
        }
        DrawPrimitive::ColoredTriangleWithDepth { points, color, .. }
        | DrawPrimitive::TranslucentTriangleWithDepth { points, color, .. } => {
            let mut vertices = points;
            if vertices[0].y > vertices[1].y {
                vertices.swap(0, 1);
            }
            if vertices[0].y > vertices[2].y {
                vertices.swap(0, 2);
            }
            if vertices[1].y > vertices[2].y {
                vertices.swap(1, 2);
            }

            let mut buf: Vec<_, 3> = Vec::new();
            for p in vertices.iter() {
                buf.push(Point::new(p.x, p.y)).unwrap();
            }
            let [p1, p2, p3] = buf.into_array().unwrap();
            fill_triangle_screen_clipped(p1, p2, p3, color, fb);
        }
        #[cfg(feature = "lighting")]
        DrawPrimitive::GouraudTriangle {
            mut points,
            mut colors,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                colors.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                colors.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                colors.swap(1, 2);
            }

            let mut buf: Vec<_, 3> = Vec::new();
            for p in points.iter() {
                buf.push(Point::new(p.x, p.y)).unwrap();
            }
            let [p1, p2, p3] = buf.into_array().unwrap();
            let [c1, c2, c3] = colors;

            let bounds = fb.bounding_box();
            let scr_w = bounds.size.width as i32;
            let scr_h = bounds.size.height as i32;
            if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                return;
            }
            if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                return;
            }
            if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                return;
            }
            if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                return;
            }

            if p2.y == p3.y {
                fill_bottom_flat_gouraud(p1, p2, p3, c1, c2, c3, fb);
            } else if p1.y == p2.y {
                fill_top_flat_gouraud(p1, p2, p3, c1, c2, c3, fb);
            } else {
                let t = (p2.y - p1.y) as f32 / (p3.y - p1.y) as f32;
                let p4 = Point::new((p1.x as f32 + t * (p3.x - p1.x) as f32) as i32, p2.y);
                let c4 = interpolate_color(c1, c3, t);

                fill_bottom_flat_gouraud(p1, p2, p4, c1, c2, c4, fb);
                fill_top_flat_gouraud(p2, p4, p3, c2, c4, c3, fb);
            }
        }
        #[cfg(feature = "lighting")]
        DrawPrimitive::GouraudTriangleWithDepth { points, colors, .. } => {
            let prim = DrawPrimitive::GouraudTriangle { points, colors };
            draw(prim, fb);
        }
        #[cfg(feature = "textured")]
        DrawPrimitive::TexturedTriangle { .. }
        | DrawPrimitive::TexturedTriangleWithDepth { .. }
        | DrawPrimitive::TexturedGouraudTriangleWithDepth { .. }
        | DrawPrimitive::LightmappedTriangle { .. } => {}
    }
}

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn interpolate_color(c1: Rgb565, c2: Rgb565, t: f32) -> Rgb565 {
    let r1 = c1.r() as f32;
    let g1 = c1.g() as f32;
    let b1 = c1.b() as f32;

    let r2 = c2.r() as f32;
    let g2 = c2.g() as f32;
    let b2 = c2.b() as f32;

    let r = (r1 + t * (r2 - r1)) as u8;
    let g = (g1 + t * (g2 - g1)) as u8;
    let b = (b1 + t * (b2 - b1)) as u8;

    Rgb565::new(r, g, b)
}

#[cfg(feature = "lighting")]
fn fill_bottom_flat_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = (p2.y - p1.y) as f32;
    if height == 0.0 {
        return;
    }
    let invslope1 = (p2.x - p1.x) as f32 / height;
    let invslope2 = (p3.x - p1.x) as f32 / height;

    let mut curx1 = p1.x as f32;
    let mut curx2 = p1.x as f32;

    for scanline_y in p1.y..=p2.y {
        let dy = (scanline_y - p1.y) as f32;
        let t = dy / height;
        let color_left = interpolate_color(c1, c2, t);
        let color_right = interpolate_color(c1, c3, t);

        draw_horizontal_line_gouraud(
            curx1 as i32,
            curx2 as i32,
            scanline_y,
            color_left,
            color_right,
            fb,
        );
        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[cfg(feature = "lighting")]
fn fill_top_flat_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = (p3.y - p1.y) as f32;
    if height == 0.0 {
        return;
    }
    let invslope1 = (p3.x - p1.x) as f32 / height;
    let invslope2 = (p3.x - p2.x) as f32 / height;

    let mut curx1 = p3.x as f32;
    let mut curx2 = p3.x as f32;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = (p3.y - scanline_y) as f32;
        let t = dy / height;
        let color_left = interpolate_color(c3, c1, t);
        let color_right = interpolate_color(c3, c2, t);

        draw_horizontal_line_gouraud(
            curx1 as i32,
            curx2 as i32,
            scanline_y,
            color_left,
            color_right,
            fb,
        );
        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[cfg(feature = "lighting")]
fn draw_horizontal_line_gouraud<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    color1: Rgb565,
    color2: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    let (start_x, end_x, start_color, end_color) = if x1 <= x2 {
        (x1, x2, color1, color2)
    } else {
        (x2, x1, color2, color1)
    };

    let span = (end_x - start_x) as f32;
    for x in start_x..=end_x {
        let t = if span > 0.0 {
            (x - start_x) as f32 / span
        } else {
            0.0
        };
        let color = interpolate_color(start_color, end_color, t);
        fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), color)])
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::Pixel;
    use embedded_graphics_core::geometry::{OriginDimensions, Size};

    struct TestFb<const W: usize, const H: usize> {
        pixels: [Rgb565; 400],
    }

    impl<const W: usize, const H: usize> Default for TestFb<W, H> {
        fn default() -> Self {
            Self {
                pixels: [Rgb565::BLACK; 400],
            }
        }
    }

    impl<const W: usize, const H: usize> OriginDimensions for TestFb<W, H> {
        fn size(&self) -> Size {
            Size::new(W as u32, H as u32)
        }
    }

    impl<const W: usize, const H: usize> DrawTarget for TestFb<W, H> {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(point, color) in pixels {
                if point.x >= 0 && point.x < W as i32 && point.y >= 0 && point.y < H as i32 {
                    let idx = (point.y as usize) * W + (point.x as usize);
                    if idx < self.pixels.len() {
                        self.pixels[idx] = color;
                    }
                }
            }
            Ok(())
        }
    }

    #[test]
    fn test_fill_triangle_variations() {
        let mut fb = TestFb::<20, 20>::default();

        // Flat-bottom triangle
        fill_triangle(
            Point::new(10, 2),
            Point::new(2, 10),
            Point::new(18, 10),
            Rgb565::RED,
            &mut fb,
        );
        assert_eq!(fb.pixels[5 * 20 + 10], Rgb565::RED);

        // Flat-top triangle
        fill_triangle(
            Point::new(2, 11),
            Point::new(18, 11),
            Point::new(10, 19),
            Rgb565::GREEN,
            &mut fb,
        );
        assert_eq!(fb.pixels[15 * 20 + 10], Rgb565::GREEN);

        // General triangle
        let mut fb2 = TestFb::<20, 20>::default();
        fill_triangle(
            Point::new(10, 1),
            Point::new(2, 8),
            Point::new(18, 18),
            Rgb565::BLUE,
            &mut fb2,
        );
        assert_eq!(fb2.pixels[8 * 20 + 10], Rgb565::BLUE);
    }

    #[test]
    fn test_draw_primitives() {
        let mut fb = TestFb::<20, 20>::default();

        // Line
        draw(
            DrawPrimitive::Line(
                [nalgebra::Point2::new(1, 1), nalgebra::Point2::new(5, 1)],
                Rgb565::WHITE,
            ),
            &mut fb,
        );
        assert_eq!(fb.pixels[1 * 20 + 3], Rgb565::WHITE);

        // Point
        draw(
            DrawPrimitive::ColoredPoint(nalgebra::Point2::new(7, 7), Rgb565::RED),
            &mut fb,
        );
        assert_eq!(fb.pixels[7 * 20 + 7], Rgb565::RED);

        // ColoredTriangle
        draw(
            DrawPrimitive::ColoredTriangle(
                [
                    nalgebra::Point2::new(10, 2),
                    nalgebra::Point2::new(5, 15),
                    nalgebra::Point2::new(15, 15),
                ],
                Rgb565::GREEN,
            ),
            &mut fb,
        );
        assert_eq!(fb.pixels[10 * 20 + 10], Rgb565::GREEN);
    }
}
