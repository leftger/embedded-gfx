#[cfg(feature = "fixed-raster")]
use core::fmt::Debug;
#[cfg(feature = "fixed-raster")]
use embedded_graphics_core::draw_target::DrawTarget;
#[cfg(feature = "fixed-raster")]
use embedded_graphics_core::pixelcolor::Rgb565;
#[cfg(feature = "fixed-raster")]
use embedded_graphics_core::prelude::Point;

#[cfg(feature = "fixed-raster")]
pub fn fill_triangle_fixed<D: DrawTarget<Color = Rgb565>>(
    mut p1: Point,
    mut p2: Point,
    mut p3: Point,
    color: Rgb565,
    fb: &mut D,
) where
    <D as DrawTarget>::Error: Debug,
{
    if p1.y > p2.y {
        core::mem::swap(&mut p1, &mut p2);
    }
    if p1.y > p3.y {
        core::mem::swap(&mut p1, &mut p3);
    }
    if p2.y > p3.y {
        core::mem::swap(&mut p2, &mut p3);
    }

    if p1.y == p3.y {
        return;
    }

    let bounds = fb.bounding_box();
    let min_x = bounds.top_left.x;
    let max_x = bounds.bottom_right().unwrap().x;
    let min_y = bounds.top_left.y;
    let max_y = bounds.bottom_right().unwrap().y;

    let dy12 = p2.y - p1.y;
    let dy13 = p3.y - p1.y;
    let dy23 = p3.y - p2.y;

    let dx12_step = if dy12 > 0 {
        ((p2.x - p1.x) << 16) / dy12
    } else {
        0
    };
    let dx13_step = if dy13 > 0 {
        ((p3.x - p1.x) << 16) / dy13
    } else {
        0
    };
    let dx23_step = if dy23 > 0 {
        ((p3.x - p2.x) << 16) / dy23
    } else {
        0
    };

    let mut x13_fp = (p1.x << 16) + 0x8000;
    let mut x12_fp = x13_fp;

    for y in p1.y..p2.y {
        if y >= min_y && y <= max_y {
            let xa = (x12_fp >> 16).clamp(min_x, max_x);
            let xb = (x13_fp >> 16).clamp(min_x, max_x);
            let (start_x, end_x) = if xa <= xb { (xa, xb) } else { (xb, xa) };
            for x in start_x..=end_x {
                let _ = fb.draw_iter(core::iter::once(embedded_graphics_core::Pixel(
                    Point::new(x, y),
                    color,
                )));
            }
        }
        x12_fp += dx12_step;
        x13_fp += dx13_step;
    }

    let mut x23_fp = (p2.x << 16) + 0x8000;
    for y in p2.y..=p3.y {
        if y >= min_y && y <= max_y {
            let xa = (x23_fp >> 16).clamp(min_x, max_x);
            let xb = (x13_fp >> 16).clamp(min_x, max_x);
            let (start_x, end_x) = if xa <= xb { (xa, xb) } else { (xb, xa) };
            for x in start_x..=end_x {
                let _ = fb.draw_iter(core::iter::once(embedded_graphics_core::Pixel(
                    Point::new(x, y),
                    color,
                )));
            }
        }
        x23_fp += dx23_step;
        x13_fp += dx13_step;
    }
}

#[cfg(feature = "fixed-raster")]
pub fn fill_triangle_zbuffered_fixed<D: DrawTarget<Color = Rgb565>>(
    mut p1: Point,
    mut p2: Point,
    mut p3: Point,
    mut z1: u32,
    mut z2: u32,
    mut z3: u32,
    color: Rgb565,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    if p1.y > p2.y {
        core::mem::swap(&mut p1, &mut p2);
        core::mem::swap(&mut z1, &mut z2);
    }
    if p1.y > p3.y {
        core::mem::swap(&mut p1, &mut p3);
        core::mem::swap(&mut z1, &mut z3);
    }
    if p2.y > p3.y {
        core::mem::swap(&mut p2, &mut p3);
        core::mem::swap(&mut z2, &mut z3);
    }

    if p1.y == p3.y {
        return;
    }

    let bounds = fb.bounding_box();
    let min_x = bounds.top_left.x;
    let max_x = bounds.bottom_right().unwrap().x;
    let min_y = bounds.top_left.y;
    let max_y = bounds.bottom_right().unwrap().y;

    let dy12 = p2.y - p1.y;
    let dy13 = p3.y - p1.y;
    let dy23 = p3.y - p2.y;

    let dx12_step = if dy12 > 0 {
        ((p2.x - p1.x) << 16) / dy12
    } else {
        0
    };
    let dx13_step = if dy13 > 0 {
        ((p3.x - p1.x) << 16) / dy13
    } else {
        0
    };
    let dx23_step = if dy23 > 0 {
        ((p3.x - p2.x) << 16) / dy23
    } else {
        0
    };

    let dz12_step = if dy12 > 0 {
        ((z2 as i64 - z1 as i64) << 16) / dy12 as i64
    } else {
        0
    };
    let dz13_step = if dy13 > 0 {
        ((z3 as i64 - z1 as i64) << 16) / dy13 as i64
    } else {
        0
    };
    let dz23_step = if dy23 > 0 {
        ((z3 as i64 - z2 as i64) << 16) / dy23 as i64
    } else {
        0
    };

    let mut x13_fp = (p1.x << 16) + 0x8000;
    let mut x12_fp = x13_fp;
    let mut z13_fp = (z1 as i64) << 16;
    let mut z12_fp = z13_fp;

    for y in p1.y..p2.y {
        if y >= min_y && y <= max_y {
            let xa = x12_fp >> 16;
            let xb = x13_fp >> 16;
            let (start_x, end_x, za_fp, zb_fp) = if xa <= xb {
                (xa, xb, z12_fp, z13_fp)
            } else {
                (xb, xa, z13_fp, z12_fp)
            };
            let span_dx = end_x - start_x;
            let dz_span_step = if span_dx > 0 {
                (zb_fp - za_fp) / span_dx as i64
            } else {
                0
            };
            let mut z_curr_fp = za_fp;

            for x in start_x..=end_x {
                if x >= min_x && x <= max_x {
                    let z_val = (z_curr_fp >> 16) as u32;
                    let zdepth = crate::to_zdepth(z_val);
                    let idx = (y as usize) * width + (x as usize);
                    if idx < zbuffer.len() && zdepth < zbuffer[idx] {
                        zbuffer[idx] = zdepth;
                        let _ = fb.draw_iter(core::iter::once(embedded_graphics_core::Pixel(
                            Point::new(x, y),
                            color,
                        )));
                    }
                }
                z_curr_fp += dz_span_step;
            }
        }
        x12_fp += dx12_step;
        x13_fp += dx13_step;
        z12_fp += dz12_step;
        z13_fp += dz13_step;
    }

    let mut x23_fp = ((p1.x << 16) + 0x8000) + dx12_step * dy12;
    let mut z23_fp = ((z1 as i64) << 16) + dz12_step * dy12 as i64;
    for y in p2.y..=p3.y {
        if y >= min_y && y <= max_y {
            let xa = x23_fp >> 16;
            let xb = x13_fp >> 16;
            let (start_x, end_x, za_fp, zb_fp) = if xa <= xb {
                (xa, xb, z23_fp, z13_fp)
            } else {
                (xb, xa, z13_fp, z23_fp)
            };
            let span_dx = end_x - start_x;
            let dz_span_step = if span_dx > 0 {
                (zb_fp - za_fp) / span_dx as i64
            } else {
                0
            };
            let mut z_curr_fp = za_fp;

            for x in start_x..=end_x {
                if x >= min_x && x <= max_x {
                    let z_val = (z_curr_fp >> 16) as u32;
                    let zdepth = crate::to_zdepth(z_val);
                    let idx = (y as usize) * width + (x as usize);
                    if idx < zbuffer.len() && zdepth < zbuffer[idx] {
                        zbuffer[idx] = zdepth;
                        let _ = fb.draw_iter(core::iter::once(embedded_graphics_core::Pixel(
                            Point::new(x, y),
                            color,
                        )));
                    }
                }
                z_curr_fp += dz_span_step;
            }
        }
        x23_fp += dx23_step;
        x13_fp += dx13_step;
        z23_fp += dz23_step;
        z13_fp += dz13_step;
    }
}
