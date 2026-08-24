//! Standard Z-buffered triangle rasterization, Gouraud shading, and translucent triangles.

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::Point;

use super::blend::fast_blend_rgb565;
use super::effects::{DepthInterpolationMode, DitherConfig, FogConfig};
#[cfg(feature = "lighting")]
use super::fill::interpolate_color;
use crate::primitive::DrawPrimitive;

/// Render primitives with Z-buffering.
#[inline]
pub fn draw_zbuffered<D: DrawTarget<Color = Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    draw_zbuffered_with_options(
        primitive,
        fb,
        zbuffer,
        width,
        None,
        None,
        DepthInterpolationMode::Exact,
    );
}

/// Render primitives with Z-buffering and optional fog / dithering post-processing.
#[inline]
pub fn draw_zbuffered_with_effects<D: DrawTarget<Color = Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    draw_zbuffered_with_options(
        primitive,
        fb,
        zbuffer,
        width,
        fog_config,
        dither_config,
        DepthInterpolationMode::Exact,
    );
}

/// Render primitives with Z-buffering, effects, and configurable depth interpolation mode.
pub fn draw_zbuffered_with_options<D: DrawTarget<Color = Rgb565>>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    depth_mode: DepthInterpolationMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::ColoredTriangleWithDepth {
            mut points,
            depths: mut raw_depths,
            color,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                raw_depths.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                raw_depths.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                raw_depths.swap(1, 2);
            }

            let [p1, p2, p3] = points;
            let (z1, z2, z3) =
                depth_mode.process_depths(raw_depths[0], raw_depths[1], raw_depths[2]);

            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
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

            fill_triangle_zbuffered(
                p1,
                p2,
                p3,
                z1,
                z2,
                z3,
                color,
                fb,
                zbuffer,
                width,
                fog_config,
                dither_config,
            );
        }

        DrawPrimitive::TranslucentTriangleWithDepth {
            mut points,
            depths: mut raw_depths,
            color,
            alpha,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                raw_depths.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                raw_depths.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                raw_depths.swap(1, 2);
            }

            let [p1, p2, p3] = points;
            let (z1, z2, z3) =
                depth_mode.process_depths(raw_depths[0], raw_depths[1], raw_depths[2]);

            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
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

            fill_triangle_zbuffered_translucent(
                p1, p2, p3, z1, z2, z3, color, alpha, fb, zbuffer, width,
            );
        }

        #[cfg(feature = "lighting")]
        DrawPrimitive::GouraudTriangleWithDepth {
            mut points,
            depths: mut raw_depths,
            mut colors,
        } => {
            if points[0].y > points[1].y {
                points.swap(0, 1);
                raw_depths.swap(0, 1);
                colors.swap(0, 1);
            }
            if points[0].y > points[2].y {
                points.swap(0, 2);
                raw_depths.swap(0, 2);
                colors.swap(0, 2);
            }
            if points[1].y > points[2].y {
                points.swap(1, 2);
                raw_depths.swap(1, 2);
                colors.swap(1, 2);
            }

            let [p1, p2, p3] = points;
            let (z1, z2, z3) =
                depth_mode.process_depths(raw_depths[0], raw_depths[1], raw_depths[2]);
            let [c1, c2, c3] = colors;

            let scr_w = width as i32;
            let scr_h = (zbuffer.len() / width) as i32;
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

            fill_triangle_zbuffered_gouraud(
                p1,
                p2,
                p3,
                z1,
                z2,
                z3,
                c1,
                c2,
                c3,
                fb,
                zbuffer,
                width,
                fog_config,
                dither_config,
            );
        }

        _ => super::fill::draw(primitive, fb),
    }
}

#[inline]
pub(crate) fn fill_triangle_zbuffered<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;

        fill_bottom_flat_triangle_zbuffered(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
        fill_top_flat_triangle_zbuffered(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    }
}

#[cfg(feature = "lighting")]
#[inline]
pub(crate) fn fill_triangle_zbuffered_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            c1,
            c2,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            c1,
            c2,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        let c4 = interpolate_color(c1, c3, t);

        fill_bottom_flat_triangle_zbuffered_gouraud(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            c1,
            c2,
            c4,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
        fill_top_flat_triangle_zbuffered_gouraud(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            c2,
            c4,
            c3,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );
    }
}

#[inline]
pub(crate) fn fill_bottom_flat_triangle_zbuffered<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = (p1.x << 16) + (1 << 15);
    let mut curx2 = (p1.x << 16) + (1 << 15);

    let scr_h = (zbuffer.len() / width) as i32;
    let y_skip = (0_i32 - p1.y).max(0);
    curx1 = curx1.wrapping_add(invslope1.wrapping_mul(y_skip));
    curx2 = curx2.wrapping_add(invslope2.wrapping_mul(y_skip));
    let y_start = p1.y.max(0);
    let y_end = p2.y.min(scr_h - 1);

    for scanline_y in y_start..=y_end {
        let dy = scanline_y - p1.y;
        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        draw_scanline_zbuffered(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 = curx1.wrapping_add(invslope1);
        curx2 = curx2.wrapping_add(invslope2);
    }
}

#[inline]
pub(crate) fn fill_top_flat_triangle_zbuffered<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = (p3.x << 16) + (1 << 15);
    let mut curx2 = (p3.x << 16) + (1 << 15);

    let scr_h = (zbuffer.len() / width) as i32;
    let y_skip_bot = (p3.y - (scr_h - 1)).max(0);
    curx1 = curx1.wrapping_sub(invslope1.wrapping_mul(y_skip_bot));
    curx2 = curx2.wrapping_sub(invslope2.wrapping_mul(y_skip_bot));
    let y_start = p1.y.max(0);
    let y_end = p3.y.min(scr_h - 1);

    for scanline_y in (y_start..=y_end).rev() {
        let dy = scanline_y - p1.y;
        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        draw_scanline_zbuffered(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 = curx1.wrapping_sub(invslope1);
        curx2 = curx2.wrapping_sub(invslope2);
    }
}

#[cfg(feature = "lighting")]
#[inline(always)]
fn fill_bottom_flat_triangle_zbuffered_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = (p1.x << 16) + (1 << 15);
    let mut curx2 = (p1.x << 16) + (1 << 15);

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        let color_left = interpolate_color(c1, c2, t);
        let color_right = interpolate_color(c1, c3, t);

        draw_scanline_zbuffered_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color_left,
            color_right,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[cfg(feature = "lighting")]
#[inline(always)]
fn fill_top_flat_triangle_zbuffered_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = (p3.x << 16) + (1 << 15);
    let mut curx2 = (p3.x << 16) + (1 << 15);

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        let color_left = interpolate_color(c1, c3, t);
        let color_right = interpolate_color(c2, c3, t);

        draw_scanline_zbuffered_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            color_left,
            color_right,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[cfg(feature = "lighting")]
#[inline(always)]
fn draw_scanline_zbuffered_gouraud<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    color1: Rgb565,
    color2: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    if y < 0 {
        return;
    }
    let height = zbuffer.len() / width;
    if y as usize >= height {
        return;
    }

    let (left_x, right_x, z_left, z_right, c_left, c_right) = if x1 <= x2 {
        (x1, x2, z1, z2, color1, color2)
    } else {
        (x2, x1, z2, z1, color2, color1)
    };

    let start_x = left_x.max(0);
    let end_x = right_x.min(width as i32 - 1);
    if start_x > end_x {
        return;
    }

    let span = right_x - left_x;
    let z_step = if span > 0 {
        (((z_right as i64 - z_left as i64) << 16) / span as i64) as i32
    } else {
        0
    };

    let left_clip = start_x - left_x;
    let r_step = if span > 0 {
        ((c_right.r() as i32 - c_left.r() as i32) << 16) / span
    } else {
        0
    };
    let g_step = if span > 0 {
        ((c_right.g() as i32 - c_left.g() as i32) << 16) / span
    } else {
        0
    };
    let b_step = if span > 0 {
        ((c_right.b() as i32 - c_left.b() as i32) << 16) / span
    } else {
        0
    };

    let mut z_curr = ((z_left as i64) << 16) + (left_clip as i64 * z_step as i64);
    let mut r_curr = ((c_left.r() as i32) << 16) + left_clip * r_step;
    let mut g_curr = ((c_left.g() as i32) << 16) + left_clip * g_step;
    let mut b_curr = ((c_left.b() as i32) << 16) + left_clip * b_step;
    let mut zbuf_idx = y as usize * width + start_x as usize;

    for x in start_x..=end_x {
        let z = (z_curr >> 16) as u32;
        let r = (r_curr >> 16).clamp(0, 31) as u8;
        let g = (g_curr >> 16).clamp(0, 63) as u8;
        let b = (b_curr >> 16).clamp(0, 31) as u8;
        z_curr += z_step as i64;
        r_curr += r_step;
        g_curr += g_step;
        b_curr += b_step;
        let z_depth = crate::to_zdepth(z);

        if z_depth < zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
            zbuffer[zbuf_idx] = z_depth;

            let mut final_color = Rgb565::new(r, g, b);

            if let Some(fog) = fog_config {
                final_color = fog.apply(final_color, z);
            }

            if let Some(dither) = dither_config {
                final_color = dither.apply(final_color, x, y);
            }

            fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                .unwrap();
        }
        zbuf_idx += 1;
    }
}

#[inline(always)]
pub(crate) fn draw_scanline_zbuffered<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    if y < 0 {
        return;
    }
    let height = zbuffer.len() / width;
    if y as usize >= height {
        return;
    }

    let (left_x, right_x, z_left, z_right) = if x1 <= x2 {
        (x1, x2, z1, z2)
    } else {
        (x2, x1, z2, z1)
    };

    let start_x = left_x.max(0);
    let end_x = right_x.min(width as i32 - 1);
    if start_x > end_x {
        return;
    }

    let span = right_x - left_x;
    let z_step = if span > 0 {
        (((z_right as i64 - z_left as i64) << 16) / span as i64) as i32
    } else {
        0
    };

    let mut zbuf_idx = y as usize * width + start_x as usize;

    if z_step == 0 {
        let z = z_left;
        let z_depth = crate::to_zdepth(z);
        let mut base_color = color;
        if let Some(fog) = fog_config {
            base_color = fog.apply(base_color, z);
        }
        for x in start_x..=end_x {
            if z_depth < zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
                zbuffer[zbuf_idx] = z_depth;
                let final_color = if let Some(dither) = dither_config {
                    dither.apply(base_color, x, y)
                } else {
                    base_color
                };
                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                    .unwrap();
            }
            zbuf_idx += 1;
        }
        return;
    }

    let left_clip = start_x - left_x;
    let mut z_curr = ((z_left as i64) << 16) + (left_clip as i64 * z_step as i64);

    for x in start_x..=end_x {
        let z = (z_curr >> 16) as u32;
        z_curr += z_step as i64;
        let z_depth = crate::to_zdepth(z);

        if z_depth < zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
            zbuffer[zbuf_idx] = z_depth;

            let mut final_color = color;

            if let Some(fog) = fog_config {
                final_color = fog.apply(final_color, z);
            }

            if let Some(dither) = dither_config {
                final_color = dither.apply(final_color, x, y);
            }

            fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                .unwrap();
        }
        zbuf_idx += 1;
    }
}

#[inline(always)]
fn fill_triangle_zbuffered_translucent<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    color: Rgb565,
    alpha: u8,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_translucent(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, alpha, fb, zbuffer, width,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_translucent(
            p1_eg, p2_eg, p3_eg, z1_int, z2_int, z3_int, color, alpha, fb, zbuffer, width,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        fill_bottom_flat_translucent(
            p1_eg, p2_eg, p4, z1_int, z2_int, z4_int, color, alpha, fb, zbuffer, width,
        );
        fill_top_flat_translucent(
            p2_eg, p4, p3_eg, z2_int, z4_int, z3_int, color, alpha, fb, zbuffer, width,
        );
    }
}

#[inline(always)]
fn fill_bottom_flat_translucent<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    alpha: u8,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = (p1.x << 16) + (1 << 15);
    let mut curx2 = (p1.x << 16) + (1 << 15);

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;

        let (left_x, right_x, z_l, z_r) = if curx1 <= curx2 {
            (curx1 >> 16, curx2 >> 16, z_left, z_right)
        } else {
            (curx2 >> 16, curx1 >> 16, z_right, z_left)
        };

        let span = right_x - left_x;
        for x in left_x..=right_x {
            if x < 0 || scanline_y < 0 || x >= width as i32 {
                continue;
            }
            let idx = (scanline_y as usize) * width + (x as usize);
            if idx >= zbuffer.len() {
                continue;
            }

            let z = if span > 0 {
                let t = (x - left_x) as f32 / span as f32;
                (z_l as f32 + t * (z_r as f32 - z_l as f32)) as u32
            } else {
                z_l
            };
            let z_depth = crate::to_zdepth(z);

            if z_depth < zbuffer[idx] {
                zbuffer[idx] = z_depth;
                let draw_color = fast_blend_rgb565(Rgb565::BLACK, color, alpha);
                let _ = fb.draw_iter([embedded_graphics_core::Pixel(
                    Point::new(x, scanline_y),
                    draw_color,
                )]);
            }
        }

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[inline(always)]
fn fill_top_flat_translucent<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    color: Rgb565,
    alpha: u8,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = (p3.x << 16) + (1 << 15);
    let mut curx2 = (p3.x << 16) + (1 << 15);

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let z_left = (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32;
        let z_right = (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32;

        let (left_x, right_x, z_l, z_r) = if curx1 <= curx2 {
            (curx1 >> 16, curx2 >> 16, z_left, z_right)
        } else {
            (curx2 >> 16, curx1 >> 16, z_right, z_left)
        };

        let span = right_x - left_x;
        for x in left_x..=right_x {
            if x < 0 || scanline_y < 0 || x >= width as i32 {
                continue;
            }
            let idx = (scanline_y as usize) * width + (x as usize);
            if idx >= zbuffer.len() {
                continue;
            }

            let z = if span > 0 {
                let t = (x - left_x) as f32 / span as f32;
                (z_l as f32 + t * (z_r as f32 - z_l as f32)) as u32
            } else {
                z_l
            };
            let z_depth = crate::to_zdepth(z);

            if z_depth < zbuffer[idx] {
                zbuffer[idx] = z_depth;
                let draw_color = fast_blend_rgb565(Rgb565::BLACK, color, alpha);
                let _ = fb.draw_iter([embedded_graphics_core::Pixel(
                    Point::new(x, scanline_y),
                    draw_color,
                )]);
            }
        }

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}
