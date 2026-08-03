use embedded_3dgfx::config::default_profile_caps;
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

/// Reports throughput as `perf_baseline profile=<name> path=<name> ktri_per_sec=<value>`
/// so CI can capture per-profile numbers without gating on wall-clock timing
/// (shared runners are too noisy for hard perf thresholds; see #24).
fn report(profile: &str, path: &str, iterations: u32, elapsed: std::time::Duration) {
    let ktri_per_sec = (iterations as f64 / elapsed.as_secs_f64()) / 1000.0;
    println!(
        "perf_baseline profile={profile} path={path} iterations={iterations} elapsed_ms={:.3} ktri_per_sec={ktri_per_sec:.2}",
        elapsed.as_secs_f64() * 1000.0
    );
}

fn main() {
    let profile_name =
        std::env::var("EMBEDDED_3DGFX_CAPS").unwrap_or_else(|_| "default".to_string());

    let (width, height) = match default_profile_caps() {
        Some(caps) => (caps.max_width as u32, caps.max_height as u32),
        None => (320, 240),
    };

    let p1 = Point::new((width as i32) / 16, (height as i32) / 12);
    let p2 = Point::new((width as i32 * 15) / 16, (height as i32) / 6);
    let p3 = Point::new((width as i32) / 2, (height as i32 * 11) / 12);
    let color = Rgb565::CSS_RED;
    let iterations = 10_000;

    let mut fb = DummyTarget::new(width, height);
    let start = Instant::now();
    for _ in 0..iterations {
        embedded_3dgfx::draw::fill_triangle(
            std::hint::black_box(p1),
            std::hint::black_box(p2),
            std::hint::black_box(p3),
            std::hint::black_box(color),
            &mut fb,
        );
    }
    std::hint::black_box(fb.count);
    report(&profile_name, "float", iterations, start.elapsed());

    #[cfg(feature = "fixed-raster")]
    {
        let mut fb = DummyTarget::new(width, height);
        let start = Instant::now();
        for _ in 0..iterations {
            embedded_3dgfx::draw::fill_triangle_fixed(
                std::hint::black_box(p1),
                std::hint::black_box(p2),
                std::hint::black_box(p3),
                std::hint::black_box(color),
                &mut fb,
            );
        }
        std::hint::black_box(fb.count);
        report(&profile_name, "fixed", iterations, start.elapsed());
    }

    #[cfg(not(feature = "fixed-raster"))]
    {
        println!("Run with --features fixed-raster to also benchmark the fixed-point rasterizer.");
    }
}
