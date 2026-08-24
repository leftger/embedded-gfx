//! Anti-aliasing algorithms (2xSSAA, Heuristic edge-AA, and Coverage-AA).

extern crate alloc;

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::Point;
#[allow(unused_imports)]
use micromath::F32Ext;

use super::blend::{ReadPixel, blend_q8};

#[cfg(feature = "aa")]
#[inline]
pub(crate) fn aa_pixel<T: ReadPixel>(fb: &T, point: Point, draw_color: Rgb565, coverage: u8) {
    if coverage == 255 {
        return;
    }
    let bg_color = fb.read_pixel(point);
    let _blended = blend_q8(bg_color, draw_color, coverage as u32);
}

/// Render primitives with Z-buffering and basic anti-aliasing.
#[cfg(feature = "aa")]
#[inline]
pub fn draw_zbuffered_aa<T: ReadPixel + DrawTarget<Color = Rgb565>>(
    primitive: crate::primitive::DrawPrimitive,
    fb: &mut T,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <T as DrawTarget>::Error: Debug,
{
    match primitive {
        crate::primitive::DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => {
            super::zbuffered::fill_triangle_zbuffered(
                points[0], points[1], points[2], depths[0], depths[1], depths[2], color, fb,
                zbuffer, width, None, None,
            );
            draw_line_aa(points[0], points[1], color, fb);
            draw_line_aa(points[1], points[2], color, fb);
            draw_line_aa(points[2], points[0], color, fb);
        }
        _ => super::zbuffered::draw_zbuffered(primitive, fb, zbuffer, width),
    }
}

struct SuperFramebuffer {
    width: u32,
    height: u32,
    pixels: alloc::vec::Vec<(Point, Rgb565)>,
}

impl DrawTarget for SuperFramebuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.pixels.push((pixel.0, pixel.1));
        }
        Ok(())
    }
}

impl embedded_graphics_core::geometry::OriginDimensions for SuperFramebuffer {
    fn size(&self) -> embedded_graphics_core::geometry::Size {
        embedded_graphics_core::geometry::Size::new(self.width, self.height)
    }
}

/// Render primitives with 2x Super-Sample Anti-Aliasing (2xSSAA).
#[cfg(feature = "aa")]
pub fn draw_zbuffered_2xssaa<D: DrawTarget<Color = Rgb565>>(
    primitive: crate::primitive::DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        crate::primitive::DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => {
            let [p1, p2, p3] = points;
            let [z1, z2, z3] = depths;

            let super_p1 = nalgebra::Point2::new(p1.x * 2, p1.y * 2);
            let super_p2 = nalgebra::Point2::new(p2.x * 2, p2.y * 2);
            let super_p3 = nalgebra::Point2::new(p3.x * 2, p3.y * 2);

            let super_width = width * 2;
            let super_height = (zbuffer.len() / width) * 2;
            let mut super_zbuffer = alloc::vec![crate::Z_MAX_VALUE; super_width * super_height];

            let mut super_fb = SuperFramebuffer {
                width: super_width as u32,
                height: super_height as u32,
                pixels: alloc::vec::Vec::new(),
            };

            super::zbuffered::fill_triangle_zbuffered(
                super_p1,
                super_p2,
                super_p3,
                z1,
                z2,
                z3,
                color,
                &mut super_fb,
                &mut super_zbuffer,
                super_width,
                None,
                None,
            );

            let mut grid = alloc::vec![None; width * (zbuffer.len() / width)];
            for (pt, c) in super_fb.pixels {
                let gx = (pt.x / 2) as usize;
                let gy = (pt.y / 2) as usize;
                let subx = (pt.x % 2) as usize;
                let suby = (pt.y % 2) as usize;
                let idx = gy * width + gx;
                if idx < grid.len() {
                    let entry = grid[idx].get_or_insert([(Rgb565::BLACK, false); 4]);
                    entry[suby * 2 + subx] = (c, true);
                }
            }

            for y in 0..(zbuffer.len() / width) {
                for x in 0..width {
                    let idx = y * width + x;
                    if let Some(samples) = grid[idx] {
                        let mut r_sum = 0u32;
                        let mut g_sum = 0u32;
                        let mut b_sum = 0u32;
                        let mut count = 0u32;
                        for (sc, active) in samples {
                            if active {
                                r_sum += sc.r() as u32;
                                g_sum += sc.g() as u32;
                                b_sum += sc.b() as u32;
                                count += 1;
                            }
                        }
                        if count > 0 {
                            let avg_c = Rgb565::new(
                                (r_sum / count) as u8,
                                (g_sum / count) as u8,
                                (b_sum / count) as u8,
                            );
                            fb.draw_iter([embedded_graphics_core::Pixel(
                                Point::new(x as i32, y as i32),
                                avg_c,
                            )])
                            .unwrap();
                        }
                    }
                }
            }
        }
        _ => super::zbuffered::draw_zbuffered(primitive, fb, zbuffer, width),
    }
}

/// Render primitives with Z-buffering and Coverage-Based Anti-Aliasing (AA-Coverage).
#[cfg(feature = "aa")]
pub fn draw_zbuffered_aa_coverage<D: DrawTarget<Color = Rgb565>>(
    primitive: crate::primitive::DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        crate::primitive::DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => {
            let height = zbuffer.len() / width;
            let mut coverage_buffer = alloc::vec![0u8; width * height];

            super::zbuffered::fill_triangle_zbuffered(
                points[0], points[1], points[2], depths[0], depths[1], depths[2], color, fb,
                zbuffer, width, None, None,
            );

            draw_line_aa_coverage(points[0], points[1], color, &mut coverage_buffer, width);
            draw_line_aa_coverage(points[1], points[2], color, &mut coverage_buffer, width);
            draw_line_aa_coverage(points[2], points[0], color, &mut coverage_buffer, width);

            composite_aa_background(fb, &coverage_buffer, color, width, height);
        }
        _ => super::zbuffered::draw_zbuffered(primitive, fb, zbuffer, width),
    }
}

#[cfg(feature = "aa")]
pub(crate) fn draw_line_aa_coverage(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    _color: Rgb565,
    coverage_buffer: &mut [u8],
    width: usize,
) {
    let mut x0 = p1.x;
    let mut y0 = p1.y;
    let x1 = p2.x;
    let y1 = p2.y;

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };

    let mut err = dx - dy;

    loop {
        if x0 >= 0 && y0 >= 0 && (x0 as usize) < width {
            let idx = (y0 as usize) * width + (x0 as usize);
            if idx < coverage_buffer.len() {
                coverage_buffer[idx] = coverage_buffer[idx].saturating_add(64);
            }
        }

        if x0 == x1 && y0 == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[cfg(feature = "aa")]
pub(crate) fn composite_aa_background<D: DrawTarget<Color = Rgb565>>(
    fb: &mut D,
    coverage_buffer: &[u8],
    color: Rgb565,
    width: usize,
    height: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let cov = coverage_buffer[idx];
            if cov > 0 {
                let blended = blend_q8(Rgb565::BLACK, color, cov as u32);
                fb.draw_iter([embedded_graphics_core::Pixel(
                    Point::new(x as i32, y as i32),
                    blended,
                )])
                .unwrap();
            }
        }
    }
}

#[cfg(feature = "aa")]
pub fn draw_line_aa<T: ReadPixel + DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    color: Rgb565,
    fb: &mut T,
) where
    <T as DrawTarget>::Error: Debug,
{
    let x0 = p1.x as f32;
    let y0 = p1.y as f32;
    let x1 = p2.x as f32;
    let y1 = p2.y as f32;

    let steep = (y1 - y0).abs() > (x1 - x0).abs();

    let (x0, y0, x1, y1) = if steep {
        (y0, x0, y1, x1)
    } else {
        (x0, y0, x1, y1)
    };

    let (x0, y0, x1, y1) = if x0 > x1 {
        (x1, y1, x0, y0)
    } else {
        (x0, y0, x1, y1)
    };

    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx == 0.0 { 1.0 } else { dy / dx };

    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xpxl1 = xend as i32;
    let ypxl1 = yend.floor() as i32;

    if steep {
        aa_pixel(fb, Point::new(ypxl1, xpxl1), color, 255);
        aa_pixel(fb, Point::new(ypxl1 + 1, xpxl1), color, 255);
    } else {
        aa_pixel(fb, Point::new(xpxl1, ypxl1), color, 255);
        aa_pixel(fb, Point::new(xpxl1, ypxl1 + 1), color, 255);
    }

    let mut intery = yend + gradient;

    let xend = x1.round();
    let yend = y1 + gradient * (xend - x1);
    let xpxl2 = xend as i32;
    let ypxl2 = yend.floor() as i32;

    if steep {
        aa_pixel(fb, Point::new(ypxl2, xpxl2), color, 255);
        aa_pixel(fb, Point::new(ypxl2 + 1, xpxl2), color, 255);
    } else {
        aa_pixel(fb, Point::new(xpxl2, ypxl2), color, 255);
        aa_pixel(fb, Point::new(xpxl2, ypxl2 + 1), color, 255);
    }

    if steep {
        for x in (xpxl1 + 1)..xpxl2 {
            let y = intery.floor() as i32;
            let frac = intery - intery.floor();
            let cov1 = ((1.0 - frac) * 255.0) as u8;
            let cov2 = (frac * 255.0) as u8;

            aa_pixel(fb, Point::new(y, x), color, cov1);
            aa_pixel(fb, Point::new(y + 1, x), color, cov2);
            intery += gradient;
        }
    } else {
        for x in (xpxl1 + 1)..xpxl2 {
            let y = intery.floor() as i32;
            let frac = intery - intery.floor();
            let cov1 = ((1.0 - frac) * 255.0) as u8;
            let cov2 = (frac * 255.0) as u8;

            aa_pixel(fb, Point::new(x, y), color, cov1);
            aa_pixel(fb, Point::new(x, y + 1), color, cov2);
            intery += gradient;
        }
    }
}
