use embedded_graphics_core::{Pixel, draw_target::DrawTarget, geometry::Point, pixelcolor::Rgb565};
use heapless::Vec;

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
                HudElement::Border { x, y, w, h, color } => {
                    let _ = target.draw_iter(
                        border_pixels(x, y, w, h).map(|(px, py)| Pixel(Point::new(px, py), color)),
                    );
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
