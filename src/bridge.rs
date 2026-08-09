//! Interoperability bridges between embedded-3dgfx and embedded-graphics.
//!
//! # What's here
//!
//! - [`draw_to`] — render a [`DrawPrimitive`] to any `DrawTarget<Color = C>`
//!   (not just `Rgb565`) via on-the-fly color conversion.
//! - [`AsEgPoint`] / [`AsNalgebraPoint`] — zero-cost conversions between
//!   `embedded_graphics_core::geometry::Point` and `nalgebra::Point2<i32>`.
//! - [`nalgebra_to_eg`] / [`eg_to_nalgebra`] — free-function equivalents.
//! - [`render_drawable_to_buffer`] — rasterize any `Drawable<Color = Rgb565>`
//!   into a `&mut [Rgb565]` slice so it can be used as a 3D texture.
//!
//! Readback for `FrameBuf` is no longer implemented here: `embedded-draw-target`
//! provides `PixelRead` for it, which satisfies `ReadPixel` through this
//! crate's blanket impl, so anti-aliased rasterization still works on a plain
//! `FrameBuf` without boilerplate.

use core::fmt::Debug;
use core::marker::PhantomData;

use embedded_graphics_core::{
    Drawable, Pixel,
    draw_target::DrawTarget,
    geometry::{Dimensions, Point},
    pixelcolor::{PixelColor, Rgb565},
    primitives::Rectangle,
};
use embedded_graphics_framebuf::{FrameBuf, backends::FrameBufferBackend};

use crate::DrawPrimitive;
use crate::draw::draw;

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
/// Use the plain [`draw`] function directly when your
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

// ── 3. Drawable → texture buffer helper ──────────────────────────────────────

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

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use embedded_graphics_core::pixelcolor::RgbColor;
    use embedded_graphics_framebuf::backends::{EndianCorrectedBuffer, EndianCorrection};
    use nalgebra::Point2;

    struct SingleRedPixel;

    impl Drawable for SingleRedPixel {
        type Color = Rgb565;
        type Output = ();

        fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
        where
            D: DrawTarget<Color = Self::Color>,
        {
            target.draw_iter([Pixel(Point::new(0, 0), Rgb565::RED)])
        }
    }

    #[test]
    fn point_conversion_helpers_roundtrip() {
        let n = Point2::new(12, -9);
        let eg = nalgebra_to_eg(n);
        assert_eq!(eg, Point::new(12, -9));
        let n2 = eg_to_nalgebra(eg);
        assert_eq!(n2, n);
        assert_eq!(n.as_eg_point(), eg);
        assert_eq!(eg.as_nalgebra(), n);
    }

    #[test]
    fn render_drawable_to_buffer_validates_buffer_size() {
        let mut too_short = [Rgb565::BLACK; 3];
        let err = render_drawable_to_buffer(&SingleRedPixel, &mut too_short, 2, 2);
        assert!(err.is_err());
    }

    #[test]
    fn render_drawable_to_buffer_draws_into_slice() {
        let mut data = [Rgb565::BLACK; 4];
        render_drawable_to_buffer(&SingleRedPixel, &mut data, 2, 2).unwrap();
        assert_eq!(data[0], Rgb565::RED);
    }

    #[test]
    fn draw_to_writes_primitive_to_target() {
        let backing = std::vec![Rgb565::BLACK; 4].leak();
        let mut fb = FrameBuf::new(
            EndianCorrectedBuffer::new(backing, EndianCorrection::ToLittleEndian),
            2,
            2,
        );
        draw_to::<Rgb565, _>(
            DrawPrimitive::ColoredPoint(Point2::new(1, 1), Rgb565::new(31, 0, 0)),
            &mut fb,
        );
        assert_eq!(fb.get_color_at(Point::new(1, 1)), Rgb565::new(31, 0, 0));
    }
}
