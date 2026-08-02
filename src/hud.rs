use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::Rgb565,
};
use heapless::Vec;

// ---------------------------------------------------------------------------
// FramebufDrawTarget — thin DrawTarget wrapper over a flat Rgb565 slice.
//
// Eliminates the OffscreenBuffer / DirtyRect boilerplate that every embedded
// demo otherwise needs to copy.  Construct with `FramebufDrawTarget::new` and
// pass to any embedded-graphics draw call.
// ---------------------------------------------------------------------------

/// A lightweight [`DrawTarget`] backed by a flat `Rgb565` pixel buffer.
///
/// Construct once per frame and pass it to `embedded-graphics` text/shape
/// renderers.  No heap allocation, no dirty-rect tracking overhead.
///
/// ```ignore
/// let mut fb = FramebufDrawTarget::new(&mut framebuf, 240, 320);
/// Text::new("HEALTH", Point::new(3, 262), style).draw(&mut fb).ok();
/// ```
pub struct FramebufDrawTarget<'a> {
    pixels: &'a mut [Rgb565],
    width: usize,
    height: usize,
}

impl<'a> FramebufDrawTarget<'a> {
    /// Create a new draw target backed by `pixels` of size `width × height`.
    ///
    /// `pixels` must have at least `width * height` elements; the constructor
    /// does **not** panic — out-of-bounds pixel writes are silently dropped.
    #[inline]
    pub fn new(pixels: &'a mut [Rgb565], width: usize, height: usize) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }
}

impl OriginDimensions for FramebufDrawTarget<'_> {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

impl DrawTarget for FramebufDrawTarget<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    #[inline]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x >= 0
                && coord.y >= 0
                && (coord.x as usize) < self.width
                && (coord.y as usize) < self.height
            {
                let idx = coord.y as usize * self.width + coord.x as usize;
                if idx < self.pixels.len() {
                    self.pixels[idx] = color;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// format_u16_dec — no_std / no_alloc integer formatter.
// ---------------------------------------------------------------------------

/// Format a `u16` as a zero-padded decimal string into a fixed-size byte
/// buffer, right-aligned with leading spaces.
///
/// Returns a `&str` slice over the used portion of `buf`.  `buf` must be at
/// least as long as `digits`; the function fills the rightmost `digits` bytes.
///
/// # Example
/// ```ignore
/// let mut buf = [b' '; 4];
/// let s = format_u16_dec(42, &mut buf, 3); // "042"
/// ```
pub fn format_u16_dec(mut value: u16, buf: &mut [u8], digits: usize) -> &str {
    let start = buf.len().saturating_sub(digits);
    for i in (start..buf.len()).rev() {
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    core::str::from_utf8(&buf[start..]).unwrap_or("")
}

/// A single HUD element drawn as a 2D overlay after the 3D scene.
#[derive(Clone, Copy, Debug)]
pub enum HudElement {
    /// Solid filled rectangle.
    FillRect {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: Rgb565,
    },
    /// Horizontal progress bar. `value` is clamped to \[0.0, 1.0\].
    ProgressBar {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        value: f32,
        fg: Rgb565,
        bg: Rgb565,
    },
    /// 1-pixel-wide outline border.
    Border {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: Rgb565,
    },
    /// Translucent filled rectangle with 8-bit alpha \[0, 255\].
    TranslucentRect {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: Rgb565,
        alpha: u8,
    },
    /// Translucent 1-pixel-wide outline border with 8-bit alpha \[0, 255\].
    TranslucentBorder {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: Rgb565,
        alpha: u8,
    },
}

/// A 2D HUD overlay layer holding up to `N` elements.
///
/// Draw with [`HudLayer::draw`] after the 3D render pass to composite the HUD
/// on top of the scene.
///
/// # Example
/// ```ignore
/// let mut hud = HudLayer::<8>::new();
/// hud.push(HudElement::ProgressBar { x: 4, y: 4, w: 64, h: 6,
///     value: health / 100.0, fg: Rgb565::RED, bg: Rgb565::new(8, 0, 0) }).ok();
/// hud.draw(&mut display);
/// ```
pub struct HudLayer<const N: usize> {
    elements: Vec<HudElement, N>,
}

impl<const N: usize> Default for HudLayer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> HudLayer<N> {
    pub const fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }

    /// Remove all elements.
    pub fn clear(&mut self) {
        self.elements.clear();
    }

    /// Add an element. Returns `Err(element)` if the layer is full.
    pub fn push(&mut self, element: HudElement) -> Result<(), HudElement> {
        self.elements.push(element)
    }

    /// Draw all elements onto `target` in insertion order.
    pub fn draw<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        for elem in &self.elements {
            match *elem {
                HudElement::FillRect { x, y, w, h, color } => {
                    let _ = target.draw_iter(
                        rect_pixels(x, y, w, h).map(|(px, py)| Pixel(Point::new(px, py), color)),
                    );
                }
                HudElement::TranslucentRect {
                    x,
                    y,
                    w,
                    h,
                    color,
                    alpha: _,
                } => {
                    let _ = target.draw_iter(
                        rect_pixels(x, y, w, h).map(|(px, py)| Pixel(Point::new(px, py), color)),
                    );
                }
                HudElement::ProgressBar {
                    x,
                    y,
                    w,
                    h,
                    value,
                    fg,
                    bg,
                } => {
                    let filled = ((w as f32 * value.clamp(0.0, 1.0)) as u32).min(w);
                    if filled > 0 {
                        let _ = target.draw_iter(
                            rect_pixels(x, y, filled, h)
                                .map(|(px, py)| Pixel(Point::new(px, py), fg)),
                        );
                    }
                    if filled < w {
                        let _ = target.draw_iter(
                            rect_pixels(x + filled as i32, y, w - filled, h)
                                .map(|(px, py)| Pixel(Point::new(px, py), bg)),
                        );
                    }
                }
                HudElement::Border { x, y, w, h, color }
                | HudElement::TranslucentBorder {
                    x,
                    y,
                    w,
                    h,
                    color,
                    alpha: _,
                } => {
                    let _ = target.draw_iter(
                        border_pixels(x, y, w, h).map(|(px, py)| Pixel(Point::new(px, py), color)),
                    );
                }
            }
        }
    }

    /// Draw all elements with fast translucent alpha blending onto a read-capable framebuffer target.
    #[cfg(feature = "aa")]
    pub fn draw_blended<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565> + crate::draw::ReadPixel,
    {
        for elem in &self.elements {
            match *elem {
                HudElement::TranslucentRect {
                    x,
                    y,
                    w,
                    h,
                    color,
                    alpha,
                } => {
                    for (px, py) in rect_pixels(x, y, w, h) {
                        let pt = Point::new(px, py);
                        let bg = target.read_pixel(pt);
                        let blended = crate::draw::fast_blend_rgb565(bg, color, alpha);
                        let _ = target.draw_iter([Pixel(pt, blended)]);
                    }
                }
                HudElement::TranslucentBorder {
                    x,
                    y,
                    w,
                    h,
                    color,
                    alpha,
                } => {
                    for (px, py) in border_pixels(x, y, w, h) {
                        let pt = Point::new(px, py);
                        let bg = target.read_pixel(pt);
                        let blended = crate::draw::fast_blend_rgb565(bg, color, alpha);
                        let _ = target.draw_iter([Pixel(pt, blended)]);
                    }
                }
                _ => {
                    self.draw(target);
                }
            }
        }
    }
}

fn rect_pixels(x: i32, y: i32, w: u32, h: u32) -> impl Iterator<Item = (i32, i32)> {
    (0..h as i32).flat_map(move |dy| (0..w as i32).map(move |dx| (x + dx, y + dy)))
}

fn border_pixels(x: i32, y: i32, w: u32, h: u32) -> impl Iterator<Item = (i32, i32)> {
    let wi = w as i32;
    let hi = h as i32;
    (0..wi)
        .map(move |dx| (x + dx, y))
        .chain((0..wi).map(move |dx| (x + dx, y + hi - 1)))
        .chain((1..hi - 1).map(move |dy| (x, y + dy)))
        .chain((1..hi - 1).map(move |dy| (x + wi - 1, y + dy)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::pixelcolor::RgbColor;

    struct MockTarget {
        pixels: heapless::Vec<(i32, i32, Rgb565), 4096>,
    }

    impl MockTarget {
        fn new() -> Self {
            Self {
                pixels: heapless::Vec::new(),
            }
        }
    }

    impl DrawTarget for MockTarget {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(p, c) in pixels {
                let _ = self.pixels.push((p.x, p.y, c));
            }
            Ok(())
        }
    }

    impl embedded_graphics_core::geometry::OriginDimensions for MockTarget {
        fn size(&self) -> embedded_graphics_core::geometry::Size {
            embedded_graphics_core::geometry::Size::new(320, 240)
        }
    }

    #[test]
    fn fill_rect_pixel_count() {
        let mut hud = HudLayer::<4>::new();
        hud.push(HudElement::FillRect {
            x: 0,
            y: 0,
            w: 3,
            h: 2,
            color: Rgb565::RED,
        })
        .unwrap();
        let mut target = MockTarget::new();
        hud.draw(&mut target);
        assert_eq!(target.pixels.len(), 6);
    }

    #[test]
    fn progress_bar_full() {
        let mut hud = HudLayer::<4>::new();
        hud.push(HudElement::ProgressBar {
            x: 0,
            y: 0,
            w: 10,
            h: 2,
            value: 1.0,
            fg: Rgb565::GREEN,
            bg: Rgb565::BLACK,
        })
        .unwrap();
        let mut target = MockTarget::new();
        hud.draw(&mut target);
        assert_eq!(target.pixels.len(), 20);
        assert!(target.pixels.iter().all(|&(_, _, c)| c == Rgb565::GREEN));
    }

    #[test]
    fn progress_bar_empty() {
        let mut hud = HudLayer::<4>::new();
        hud.push(HudElement::ProgressBar {
            x: 0,
            y: 0,
            w: 10,
            h: 2,
            value: 0.0,
            fg: Rgb565::GREEN,
            bg: Rgb565::BLACK,
        })
        .unwrap();
        let mut target = MockTarget::new();
        hud.draw(&mut target);
        assert_eq!(target.pixels.len(), 20);
        assert!(target.pixels.iter().all(|&(_, _, c)| c == Rgb565::BLACK));
    }

    #[test]
    fn border_pixel_count() {
        let mut hud = HudLayer::<4>::new();
        hud.push(HudElement::Border {
            x: 0,
            y: 0,
            w: 4,
            h: 3,
            color: Rgb565::WHITE,
        })
        .unwrap();
        let mut target = MockTarget::new();
        hud.draw(&mut target);
        // perimeter of 4×3 = 2*(4+3) - 4 corners counted once = 10
        assert_eq!(target.pixels.len(), 10);
    }

    #[test]
    fn layer_full_returns_err() {
        let mut hud = HudLayer::<1>::new();
        hud.push(HudElement::FillRect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: Rgb565::RED,
        })
        .unwrap();
        let result = hud.push(HudElement::FillRect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: Rgb565::RED,
        });
        assert!(result.is_err());
    }

    #[test]
    fn clear_empties_layer() {
        let mut hud = HudLayer::<4>::new();
        hud.push(HudElement::FillRect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
            color: Rgb565::RED,
        })
        .unwrap();
        hud.clear();
        let mut target = MockTarget::new();
        hud.draw(&mut target);
        assert_eq!(target.pixels.len(), 0);
    }
}
