use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::Point;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::*;
use std::time::Instant;

struct DummyTarget {
    width: u32,
    height: u32,
    count: usize,
}

impl DummyTarget {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            count: 0,
        }
    }
}

impl Dimensions for DummyTarget {
    fn bounding_box(&self) -> embedded_graphics_core::primitives::Rectangle {
        embedded_graphics_core::primitives::Rectangle::new(
            Point::new(0, 0),
            Size::new(self.width, self.height),
        )
    }
}

impl DrawTarget for DummyTarget {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
    {
        for _ in pixels {
            self.count += 1;
        }
        Ok(())
    }
}

fn main() {
    let mut fb = DummyTarget::new(320, 240);
    let p1 = Point::new(20, 20);
    let p2 = Point::new(300, 40);
    let p3 = Point::new(150, 220);
    let color = Rgb565::CSS_RED;

    let iterations = 10_000;

    #[cfg(feature = "fixed-raster")]
    {
        let start = Instant::now();
        for _ in 0..iterations {
            embedded_3dgfx::draw::fill_triangle_fixed(p1, p2, p3, color, &mut fb);
        }
        let elapsed = start.elapsed();
        println!(
            "Fixed-Point Rasterizer: {} triangles in {:?} ({:.2} kTri/sec)",
            iterations,
            elapsed,
            (iterations as f64 / elapsed.as_secs_f64()) / 1000.0
        );
    }

    #[cfg(not(feature = "fixed-raster"))]
    {
        println!("Run with --features fixed-raster to benchmark fixed-point rasterizer.");
    }
}
