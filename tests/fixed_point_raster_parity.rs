#![cfg(feature = "fixed-raster")]

use embedded_3dgfx::Z_MAX_VALUE;
use embedded_3dgfx::draw::{fill_triangle_fixed, fill_triangle_zbuffered_fixed};
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::Point;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;

struct BufferTarget {
    pixels: Vec<(i32, i32, Rgb565)>,
    width: u32,
    height: u32,
}

impl BufferTarget {
    fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: Vec::new(),
            width,
            height,
        }
    }

    fn pixel_count(&self) -> usize {
        self.pixels.len()
    }
}

impl Dimensions for BufferTarget {
    fn bounding_box(&self) -> embedded_graphics_core::primitives::Rectangle {
        embedded_graphics_core::primitives::Rectangle::new(
            Point::new(0, 0),
            Size::new(self.width, self.height),
        )
    }
}

impl DrawTarget for BufferTarget {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
    {
        for p in pixels {
            self.pixels.push((p.0.x, p.0.y, p.1));
        }
        Ok(())
    }
}

#[test]
fn test_fixed_point_rasterizer_pixel_coverage() {
    let mut fb_fixed = BufferTarget::new(100, 100);

    let p1 = Point::new(10, 10);
    let p2 = Point::new(80, 20);
    let p3 = Point::new(30, 70);
    let color = Rgb565::CSS_BLUE;

    fill_triangle_fixed(p1, p2, p3, color, &mut fb_fixed);

    assert!(
        fb_fixed.pixel_count() > 1000,
        "Fixed-point triangle rasterizer should fill pixels inside triangle bounds"
    );
}

#[test]
fn test_fixed_point_zbuffered_rasterizer_depth_test() {
    let width = 100;
    let height = 100;
    let mut fb = BufferTarget::new(width, height);
    let mut zbuffer = vec![Z_MAX_VALUE; (width * height) as usize];

    let p1 = Point::new(10, 10);
    let p2 = Point::new(80, 20);
    let p3 = Point::new(30, 70);

    let z1 = 1000 << 16;
    let z2 = 2000 << 16;
    let z3 = 3000 << 16;

    let color = Rgb565::CSS_GREEN;

    fill_triangle_zbuffered_fixed(
        p1,
        p2,
        p3,
        z1,
        z2,
        z3,
        color,
        &mut fb,
        &mut zbuffer,
        width as usize,
    );

    assert!(fb.pixel_count() > 1000);

    // Verify z-buffer was written for rendered pixels
    let mut z_written_count = 0;
    for &z in &zbuffer {
        if z != Z_MAX_VALUE {
            z_written_count += 1;
        }
    }
    assert!(z_written_count > 1000);
}
