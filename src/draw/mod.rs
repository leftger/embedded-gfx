//! Primitive rasterization, Z-buffering, texture mapping, and blending engine.

#[cfg(feature = "aa")]
pub mod aa;
pub mod blend;
pub mod effects;
pub mod fill;
#[cfg(feature = "fixed-raster")]
pub mod fixed;
#[cfg(feature = "textured")]
pub mod textured;
pub mod zbuffered;

#[cfg(feature = "aa")]
pub use aa::*;
pub use blend::*;
pub use effects::*;
pub use fill::*;
#[cfg(feature = "fixed-raster")]
pub use fixed::*;
#[cfg(feature = "textured")]
pub use textured::*;
pub use zbuffered::*;

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_graphics_core::pixelcolor::Rgb565;
    use embedded_graphics_core::prelude::*;
    use nalgebra::Point2;

    struct MockFramebuffer {
        pixels: std::vec::Vec<(i32, i32, Rgb565)>,
    }

    impl MockFramebuffer {
        fn new() -> Self {
            Self {
                pixels: std::vec::Vec::new(),
            }
        }

        fn contains_pixel(&self, x: i32, y: i32) -> bool {
            self.pixels.iter().any(|(px, py, _)| *px == x && *py == y)
        }

        fn pixel_count(&self) -> usize {
            self.pixels.len()
        }
    }

    impl DrawTarget for MockFramebuffer {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>>,
        {
            for pixel in pixels {
                self.pixels.push((pixel.0.x, pixel.0.y, pixel.1));
            }
            Ok(())
        }
    }

    impl OriginDimensions for MockFramebuffer {
        fn size(&self) -> Size {
            Size::new(640, 480)
        }
    }

    #[test]
    fn test_draw_point() {
        let mut fb = MockFramebuffer::new();
        let point = Point2::new(10, 20);
        let color = Rgb565::CSS_RED;

        draw(crate::DrawPrimitive::ColoredPoint(point, color), &mut fb);

        assert_eq!(fb.pixel_count(), 1);
        assert!(fb.contains_pixel(10, 20));
    }

    #[test]
    fn test_draw_line_horizontal() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(10, 20);
        let p2 = Point2::new(20, 20);
        let color = Rgb565::CSS_GREEN;

        draw(crate::DrawPrimitive::Line([p1, p2], color), &mut fb);

        assert!(fb.pixel_count() >= 10);
        assert!(fb.contains_pixel(10, 20));
        assert!(fb.contains_pixel(20, 20));
    }

    #[test]
    fn test_draw_line_vertical() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(10, 10);
        let p2 = Point2::new(10, 20);
        let color = Rgb565::CSS_BLUE;

        draw(crate::DrawPrimitive::Line([p1, p2], color), &mut fb);

        assert!(fb.pixel_count() >= 10);
        assert!(fb.contains_pixel(10, 10));
        assert!(fb.contains_pixel(10, 20));
    }

    #[test]
    fn test_draw_line_diagonal() {
        let mut fb = MockFramebuffer::new();
        let p1 = Point2::new(0, 0);
        let p2 = Point2::new(10, 10);
        let color = Rgb565::CSS_WHITE;

        draw(crate::DrawPrimitive::Line([p1, p2], color), &mut fb);

        assert!(fb.pixel_count() >= 10);
        assert!(fb.contains_pixel(0, 0));
        assert!(fb.contains_pixel(10, 10));
    }

    #[test]
    fn test_draw_triangle_flat_bottom() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(50, 10),
            Point2::new(30, 30),
            Point2::new(70, 30),
        ];
        let color = Rgb565::CSS_YELLOW;

        draw(
            crate::DrawPrimitive::ColoredTriangle(vertices, color),
            &mut fb,
        );

        let count = fb.pixel_count();
        assert!(count > 0, "Expected pixels to be drawn, got {}", count);
        assert!(fb.contains_pixel(50, 10));
    }

    #[test]
    fn test_draw_triangle_flat_top() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(30, 10),
            Point2::new(70, 10),
            Point2::new(50, 30),
        ];
        let color = Rgb565::CSS_CYAN;

        draw(
            crate::DrawPrimitive::ColoredTriangle(vertices, color),
            &mut fb,
        );

        assert!(fb.pixel_count() > 20);
        assert!(fb.contains_pixel(50, 30));
    }

    #[test]
    fn test_draw_triangle_general() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(50, 10),
            Point2::new(30, 30),
            Point2::new(80, 40),
        ];
        let color = Rgb565::CSS_MAGENTA;

        draw(
            crate::DrawPrimitive::ColoredTriangle(vertices, color),
            &mut fb,
        );

        assert!(fb.pixel_count() > 30);
    }

    #[test]
    fn test_triangle_vertex_sorting() {
        let mut fb = MockFramebuffer::new();
        let vertices = [
            Point2::new(50, 30),
            Point2::new(30, 10),
            Point2::new(70, 20),
        ];
        let color = Rgb565::CSS_WHITE;

        draw(
            crate::DrawPrimitive::ColoredTriangle(vertices, color),
            &mut fb,
        );

        assert!(fb.pixel_count() > 10);
    }

    #[test]
    fn test_draw_multiple_primitives() {
        let mut fb = MockFramebuffer::new();

        draw(
            crate::DrawPrimitive::ColoredPoint(Point2::new(5, 5), Rgb565::CSS_RED),
            &mut fb,
        );
        draw(
            crate::DrawPrimitive::Line(
                [Point2::new(10, 10), Point2::new(20, 20)],
                Rgb565::CSS_GREEN,
            ),
            &mut fb,
        );

        assert!(fb.pixel_count() > 11);
        assert!(fb.contains_pixel(5, 5));
    }

    #[test]
    fn test_scanline_z_linear_interpolation_correctness() {
        let width = 100;
        let mut zbuffer = std::vec![crate::Z_MAX_VALUE; width * 10];
        let mut fb = MockFramebuffer::new();

        let x1 = 10;
        let x2 = 90;
        let y = 5;
        let z1 = 10000u32;
        let z2 = 90000u32;

        zbuffered::draw_scanline_zbuffered(
            x1,
            x2,
            y,
            z1,
            z2,
            Rgb565::CSS_BLUE,
            &mut fb,
            &mut zbuffer,
            width,
            None,
            None,
        );

        let span = (x2 - x1) as f64;
        for x in x1..=x2 {
            let idx = y as usize * width + x as usize;
            let actual_z = zbuffer[idx];
            let expected_z = (z1 as f64 + (x - x1) as f64 * (z2 - z1) as f64 / span) as u32;
            let expected_z_depth = crate::to_zdepth(expected_z);

            let diff = (actual_z as i64 - expected_z_depth as i64).abs();
            assert!(
                diff <= 2,
                "Z interpolation error at x={}: actual={}, expected={}, diff={}",
                x,
                actual_z,
                expected_z_depth,
                diff
            );
        }
    }

    #[test]
    fn test_zbuffer_depth_occlusion_correctness() {
        let width = 50;
        let mut zbuffer = std::vec![crate::Z_MAX_VALUE; width * 5];
        let mut fb = MockFramebuffer::new();

        zbuffered::draw_scanline_zbuffered(
            10,
            20,
            2,
            40000 << 16,
            40000 << 16,
            Rgb565::CSS_RED,
            &mut fb,
            &mut zbuffer,
            width,
            None,
            None,
        );

        zbuffered::draw_scanline_zbuffered(
            10,
            20,
            2,
            20000 << 16,
            20000 << 16,
            Rgb565::CSS_GREEN,
            &mut fb,
            &mut zbuffer,
            width,
            None,
            None,
        );

        for x in 10..=20 {
            let idx = 2 * width + x;
            assert_eq!(zbuffer[idx], crate::to_zdepth(20000 << 16));
        }

        zbuffered::draw_scanline_zbuffered(
            10,
            20,
            2,
            60000 << 16,
            60000 << 16,
            Rgb565::CSS_BLUE,
            &mut fb,
            &mut zbuffer,
            width,
            None,
            None,
        );

        for x in 10..=20 {
            let idx = 2 * width + x;
            assert_eq!(zbuffer[idx], crate::to_zdepth(20000 << 16));
        }
    }

    #[test]
    #[cfg(feature = "textured")]
    fn test_sub_span_textured_scanline_correctness() {
        use crate::retro::{PaletteMode, ScreenTint, StippleMode, TextureMapping};

        let width = 100;
        let mut zbuffer = std::vec![crate::Z_MAX_VALUE; width * 10];
        let mut fb = MockFramebuffer::new();

        static TEX_DATA: [Rgb565; 4] = [
            Rgb565::CSS_RED,
            Rgb565::CSS_GREEN,
            Rgb565::CSS_BLUE,
            Rgb565::CSS_YELLOW,
        ];
        let texture = crate::texture::Texture::new(&TEX_DATA, 2, 2);

        textured::draw_scanline_zbuffered_textured(
            10,
            73,
            4,
            1000 << 16,
            1000 << 16,
            1.0,
            1.0,
            [0.0, 0.0],
            [1.0, 1.0],
            &texture,
            &mut fb,
            &mut zbuffer,
            width,
            None,
            None,
            TextureMapping::Affine,
            StippleMode::Off,
            None,
            PaletteMode::Off,
        );

        for x in 10..=73 {
            let idx = 4 * width + x as usize;
            assert_eq!(zbuffer[idx], crate::to_zdepth(1000 << 16));
        }
        assert!(fb.pixel_count() >= 64);
    }

    #[test]
    fn test_fast_blend_rgb565() {
        let bg = Rgb565::BLACK;
        let fg = Rgb565::WHITE;
        assert_eq!(fast_blend_rgb565(bg, fg, 0), bg);
        assert_eq!(fast_blend_rgb565(bg, fg, 255), fg);

        let blended = fast_blend_rgb565(bg, fg, 128);
        assert!(blended.r() > 0 && blended.r() < 31);
    }

    #[test]
    fn test_fast_blend_rgba8888() {
        let bg = [0, 0, 0, 255];
        let fg = [255, 255, 255, 128];
        let out = fast_blend_rgba8888(bg, fg);
        assert!(out[0] > 0 && out[0] < 255);
    }

    #[test]
    fn test_fast_blend_rgba8888_to_rgb565() {
        let bg = Rgb565::BLACK;
        let fg = [255, 0, 0, 255];
        let out = fast_blend_rgba8888_to_rgb565(bg, fg);
        assert_eq!(out, Rgb565::CSS_RED);
    }
}
