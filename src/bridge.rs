//! Interoperability bridges between embedded-3dgfx and embedded-graphics.
//!
//! # What's here
//!
//! - [`draw_to`] — render a [`DrawPrimitive`] to any `DrawTarget<Color = C>`
//!   (not just `Rgb565`) via on-the-fly color conversion.
//! - [`AsEgPoint`] / [`AsNalgebraPoint`] — zero-cost conversions between
//!   `embedded_graphics_core::geometry::Point` and `nalgebra::Point2<i32>`.
//! - [`nalgebra_to_eg`] / [`eg_to_nalgebra`] — free-function equivalents.
//! - `ReadPixel` impl for `FrameBuf<Rgb565, B>` (enabled with the `aa`
//!   feature) so anti-aliased rasterization works without boilerplate.
//! - [`render_drawable_to_buffer`] — rasterize any `Drawable<Color = Rgb565>`
//!   into a `&mut [Rgb565]` slice so it can be used as a 3D texture.

use core::fmt::Debug;
use core::marker::PhantomData;

use embedded_graphics_core::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point},
    pixelcolor::{PixelColor, Rgb565},
    primitives::Rectangle,
    Drawable, Pixel,
};
use embedded_graphics_framebuf::{FrameBuf, backends::FrameBufferBackend};

#[cfg(feature = "aa")]
use crate::draw::ReadPixel;
#[cfg(feature = "aa")]
use embedded_graphics_core::pixelcolor::RgbColor;
use crate::draw::draw;
use crate::DrawPrimitive;

// ── 1. Color adapter ─────────────────────────────────────────────────────────

/// Adapts any `DrawTarget<Color = C>` to accept `Rgb565` pixels by converting
/// each pixel's color on the fly. Constructed internally by [`draw_to`].
pub struct ColorAdapter<'a, C, D> {
    inner: &'a mut D,
    _phantom: PhantomData<C>,
}

impl<C, D> Dimensions for ColorAdapter<'_, C, D>
where
    C: PixelColor,
    D: DrawTarget<Color = C>,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}

impl<C, D> DrawTarget for ColorAdapter<'_, C, D>
where
    C: PixelColor + From<Rgb565>,
    D: DrawTarget<Color = C>,
{
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        self.inner.draw_iter(
            pixels
                .into_iter()
                .map(|Pixel(pos, color)| Pixel(pos, C::from(color))),
        )
    }
}

/// Draw a [`DrawPrimitive`] to any [`DrawTarget`], converting pixel colors from
/// `Rgb565` (the 3D engine's internal format) to the target's native color type.
///
/// Use the plain [`draw`](crate::draw::draw) function directly when your
/// framebuffer already uses `Rgb565` — it has zero conversion overhead.
///
/// # Example
///
/// ```ignore
/// use embedded_3dgfx::bridge::draw_to;
/// use embedded_graphics_core::pixelcolor::Rgb888;
///
/// // display: impl DrawTarget<Color = Rgb888>
/// draw_to(primitive, &mut display);
/// ```
pub fn draw_to<C, D>(primitive: DrawPrimitive, fb: &mut D)
where
    C: PixelColor + From<Rgb565>,
    D: DrawTarget<Color = C>,
    D::Error: Debug,
{
    let mut adapter = ColorAdapter {
        inner: fb,
        _phantom: PhantomData,
    };
    draw(primitive, &mut adapter);
}

// ── 2. Point conversion helpers ───────────────────────────────────────────────

/// Convert a `nalgebra::Point2<i32>` to an embedded-graphics `Point`.
#[inline]
pub fn nalgebra_to_eg(p: nalgebra::Point2<i32>) -> Point {
    Point::new(p.x, p.y)
}

/// Convert an embedded-graphics `Point` to a `nalgebra::Point2<i32>`.
#[inline]
pub fn eg_to_nalgebra(p: Point) -> nalgebra::Point2<i32> {
    nalgebra::Point2::new(p.x, p.y)
}

/// Extension trait: convert a `nalgebra::Point2<i32>` to an embedded-graphics `Point`.
pub trait AsEgPoint {
    /// Returns the equivalent embedded-graphics `Point`.
    fn as_eg_point(&self) -> Point;
}

impl AsEgPoint for nalgebra::Point2<i32> {
    #[inline]
    fn as_eg_point(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

/// Extension trait: convert an embedded-graphics `Point` to `nalgebra::Point2<i32>`.
pub trait AsNalgebraPoint {
    /// Returns the equivalent `nalgebra::Point2<i32>`.
    fn as_nalgebra(&self) -> nalgebra::Point2<i32>;
}

impl AsNalgebraPoint for Point {
    #[inline]
    fn as_nalgebra(&self) -> nalgebra::Point2<i32> {
        nalgebra::Point2::new(self.x, self.y)
    }
}

// ── 3. ReadPixel for FrameBuf ────────────────────────────────────────────────

/// Implements [`ReadPixel`] for any `FrameBuf<Rgb565, B>`.
///
/// Required for anti-aliased rasterization (`aa-heuristic` / `aa-coverage`
/// features).  Because `FrameBuf` already holds the pixel data in RAM, the
/// implementation is a direct indexed lookup — no extra memory needed.
#[cfg(feature = "aa")]
impl<B> ReadPixel for FrameBuf<Rgb565, B>
where
    B: FrameBufferBackend<Color = Rgb565>,
{
    fn read_pixel(&self, point: Point) -> Rgb565 {
        let w = self.width() as i32;
        let h = self.height() as i32;
        if point.x < 0 || point.x >= w || point.y < 0 || point.y >= h {
            return <Rgb565 as RgbColor>::BLACK;
        }
        self.get_color_at(point)
    }
}

// ── 4. Drawable → texture buffer helper ──────────────────────────────────────

struct SliceBackend<'a>(pub &'a mut [Rgb565]);

impl FrameBufferBackend for SliceBackend<'_> {
    type Color = Rgb565;
    fn set(&mut self, index: usize, color: Rgb565) {
        self.0[index] = color;
    }
    fn get(&self, index: usize) -> Rgb565 {
        self.0[index]
    }
    fn nr_elements(&self) -> usize {
        self.0.len()
    }
}

/// Rasterize a [`Drawable<Color = Rgb565>`](Drawable) into a caller-supplied
/// pixel buffer, ready to be used as a 3D texture.
///
/// `width` and `height` must be powers of two (required by
/// [`Texture::new`](crate::texture::Texture::new)), and `buffer.len()` must
/// equal `width * height`.
///
/// # Usage
///
/// ```ignore
/// use embedded_3dgfx::{bridge::render_drawable_to_buffer, texture::Texture};
/// use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
///
/// static mut BUF: [Rgb565; 32 * 32] = [Rgb565::BLACK; 32 * 32];
///
/// // SAFETY: single-threaded, called once before the render loop starts.
/// render_drawable_to_buffer(&my_icon, unsafe { &mut BUF }, 32, 32).unwrap();
/// let texture = Texture::new(unsafe { &BUF }, 32, 32);
/// ```
///
/// # Errors
///
/// Returns `Err(())` when `buffer.len() != width * height`.
pub fn render_drawable_to_buffer<D>(
    drawable: &D,
    buffer: &mut [Rgb565],
    width: usize,
    height: usize,
) -> Result<(), ()>
where
    D: Drawable<Color = Rgb565>,
{
    if buffer.len() != width * height {
        return Err(());
    }
    let mut fb = FrameBuf::new(SliceBackend(buffer), width, height);
    drawable.draw(&mut fb).map(|_| ()).map_err(|_| ())
}
