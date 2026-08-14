pub use embedded_draw_target::PixelRead;
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::prelude::Point;

/// Framebuffer readback interface.
#[cfg(feature = "aa")]
pub trait ReadPixel {
    /// Returns the color currently stored at `point`.
    fn read_pixel(&self, point: Point) -> Rgb565;
}

#[cfg(feature = "aa")]
impl<T: PixelRead<Color = Rgb565>> ReadPixel for T {
    #[inline]
    fn read_pixel(&self, point: Point) -> Rgb565 {
        self.get_pixel(point)
    }
}
