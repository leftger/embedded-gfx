#[cfg(feature = "aa")]
use core::fmt::Debug;
#[cfg(feature = "aa")]
use embedded_graphics_core::draw_target::DrawTarget;
#[cfg(feature = "aa")]
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
#[cfg(feature = "aa")]
use embedded_graphics_core::prelude::Point;

#[cfg(feature = "aa")]
use crate::raster::aa::ReadPixel;

/// Component-wise blend in 8-bit fixed-point coverage.
/// `coverage_q8` ∈ [0, 256]; 256 = full line color, 0 = full background.
#[cfg(feature = "aa")]
#[inline(always)]
fn blend_q8(bg: Rgb565, fg: Rgb565, coverage_q8: u32) -> Rgb565 {
    let inv = 256 - coverage_q8;
    let r = (bg.r() as u32 * inv + fg.r() as u32 * coverage_q8) >> 8;
    let g = (bg.g() as u32 * inv + fg.g() as u32 * coverage_q8) >> 8;
    let b = (bg.b() as u32 * inv + fg.b() as u32 * coverage_q8) >> 8;
    Rgb565::new(r as u8, g as u8, b as u8)
}

#[cfg(feature = "aa")]
#[inline(always)]
fn plot_aa<D>(fb: &mut D, x: i32, y: i32, color: Rgb565, coverage_q8: u32)
where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    if coverage_q8 == 0 {
        return;
    }
    let final_color = if coverage_q8 >= 256 {
        color
    } else {
        let bg = fb.read_pixel(Point::new(x, y));
        blend_q8(bg, color, coverage_q8)
    };
    fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
        .unwrap();
}

/// Wu's anti-aliased line algorithm.
///
/// Walks the major axis one integer step at a time; at each step writes two
/// pixels straddling the line with complementary fractional coverage.
#[cfg(feature = "aa")]
pub fn draw_line_aa<D>(x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb565, fb: &mut D)
where
    D: DrawTarget<Color = Rgb565> + ReadPixel,
    <D as DrawTarget>::Error: Debug,
{
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steep = dy > dx;
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
    if dx == 0 {
        // Single pixel
        let (px, py) = if steep { (y0, x0) } else { (x0, y0) };
        plot_aa(fb, px, py, color, 256);
        return;
    }
    // 16.16 fixed-point gradient
    let gradient: i32 = ((dy as i64) << 16) as i32 / dx;
    // Start at exact (x0, y0); intery accumulates the y position in 16.16.
    let mut intery: i32 = y0 << 16;
    for x in x0..=x1 {
        let y_int = intery >> 16;
        let frac_q16 = (intery & 0xFFFF) as u32;
        let cov_top = 256 - (frac_q16 >> 8); // pixel at y_int
        let cov_bot = frac_q16 >> 8; //         pixel at y_int + 1
        if steep {
            plot_aa(fb, y_int, x, color, cov_top);
            plot_aa(fb, y_int + 1, x, color, cov_bot);
        } else {
            plot_aa(fb, x, y_int, color, cov_top);
            plot_aa(fb, x, y_int + 1, color, cov_bot);
        }
        intery += gradient;
    }
}

struct Octant {
    value: u8,
}

impl Octant {
    #[inline]
    fn new(start: (i32, i32), end: (i32, i32)) -> Self {
        let mut value = 0u8;
        let mut dx = end.0 - start.0;
        let mut dy = end.1 - start.1;

        if dy < 0 {
            dx = -dx;
            dy = -dy;
            value += 4;
        }
        if dx < 0 {
            let tmp = dx;
            dx = dy;
            dy = -tmp;
            value += 2;
        }
        if dx < dy {
            value += 1;
        }

        Self { value }
    }

    #[inline]
    fn to(&self, point: (i32, i32)) -> (i32, i32) {
        match self.value {
            0 => (point.0, point.1),
            1 => (point.1, point.0),
            2 => (point.1, -point.0),
            3 => (-point.0, point.1),
            4 => (-point.0, -point.1),
            5 => (-point.1, -point.0),
            6 => (-point.1, point.0),
            7 => (point.0, -point.1),
            _ => unreachable!(),
        }
    }

    #[inline]
    fn from(&self, point: (i32, i32)) -> (i32, i32) {
        match self.value {
            0 => (point.0, point.1),
            1 => (point.1, point.0),
            2 => (-point.1, point.0),
            3 => (-point.0, point.1),
            4 => (-point.0, -point.1),
            5 => (-point.1, -point.0),
            6 => (point.1, -point.0),
            7 => (point.0, -point.1),
            _ => unreachable!(),
        }
    }
}

/// Standard 2D Bresenham line iterator with octant transformation.
#[derive(Clone, Debug)]
pub struct Bresenham {
    point: (i32, i32),
    end_x: i32,
    delta_x: i32,
    delta_y: i32,
    error: i32,
    octant_value: u8,
}

impl Bresenham {
    pub fn new(start: (i32, i32), end: (i32, i32)) -> Self {
        let octant = Octant::new(start, end);
        let start_oct = octant.to(start);
        let end_oct = octant.to(end);

        let delta_x = end_oct.0 - start_oct.0;
        let delta_y = end_oct.1 - start_oct.1;

        Self {
            delta_x,
            delta_y,
            octant_value: octant.value,
            point: start_oct,
            end_x: end_oct.0,
            error: delta_y - delta_x,
        }
    }
}

impl Iterator for Bresenham {
    type Item = (i32, i32);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.point.0 <= self.end_x {
            let octant = Octant {
                value: self.octant_value,
            };
            let point = octant.from(self.point);

            if self.error >= 0 {
                self.point.1 += 1;
                self.error -= self.delta_x;
            }

            self.point.0 += 1;
            self.error += self.delta_y;

            Some(point)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::*;

    #[test]
    fn bresenham_covers_horizontal_vertical_and_diagonal() {
        let horizontal: Vec<_> = Bresenham::new((0, 0), (4, 0)).collect();
        assert_eq!(
            horizontal,
            std::vec![(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]
        );

        let vertical: Vec<_> = Bresenham::new((0, 0), (0, 3)).collect();
        assert_eq!(vertical, std::vec![(0, 0), (0, 1), (0, 2), (0, 3)]);

        let diagonal: Vec<_> = Bresenham::new((0, 0), (3, 3)).collect();
        assert_eq!(diagonal, std::vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
    }

    #[test]
    fn bresenham_handles_negative_and_steep_directions() {
        let backward: Vec<_> = Bresenham::new((4, 0), (0, 0)).collect();
        assert_eq!(backward, std::vec![(4, 0), (3, 0), (2, 0), (1, 0), (0, 0)]);

        let steep: Vec<_> = Bresenham::new((0, 0), (1, 4)).collect();
        assert_eq!(steep, std::vec![(0, 0), (0, 1), (0, 2), (0, 3), (1, 4)]);

        let negative_y: Vec<_> = Bresenham::new((0, 4), (4, 0)).collect();
        assert_eq!(negative_y.len(), 5);
        assert_eq!(negative_y[0], (0, 4));
        assert_eq!(negative_y[4], (4, 0));
    }
}
