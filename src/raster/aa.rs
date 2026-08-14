pub use embedded_draw_target::PixelRead;

/// Framebuffer readback interface.
#[cfg(feature = "aa")]
pub trait ReadPixel {
    /// Returns the color currently stored at `point`.
    fn read_pixel(
        &self,
        point: embedded_graphics_core::prelude::Point,
    ) -> embedded_graphics_core::pixelcolor::Rgb565;
}

#[cfg(feature = "aa")]
impl<T: PixelRead<Color = embedded_graphics_core::pixelcolor::Rgb565>> ReadPixel for T {
    #[inline]
    fn read_pixel(
        &self,
        point: embedded_graphics_core::prelude::Point,
    ) -> embedded_graphics_core::pixelcolor::Rgb565 {
        self.get_pixel(point)
    }
}
