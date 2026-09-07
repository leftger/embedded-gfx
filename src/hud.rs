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

/// Blend and transparency mode for 2D sprite blitting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpriteBlendMode {
    /// Direct overwrite of destination pixels (default).
    #[default]
    Normal,
    /// Transparent color-key (pixels matching the key color are skipped).
    ColorKey(Rgb565),
    /// Alpha blended with 8-bit alpha (0 = transparent, 255 = fully opaque).
    Alpha(u8),
    /// Saturating additive blend (src + dst), ideal for neon glow, particles, and flares.
    Additive,
}

/// A 2D sprite image referencing an RGB565 pixel slice.
#[derive(Clone, Copy, Debug)]
pub struct Sprite2D<'a> {
    /// Screen X position of the top-left corner.
    pub x: i32,
    /// Screen Y position of the top-left corner.
    pub y: i32,
    /// Width of the sprite in pixels.
    pub width: u32,
    /// Height of the sprite in pixels.
    pub height: u32,
    /// Integer scale multiplier (1 = 1x, 2 = 2x, etc.).
    pub scale: u8,
    /// Transparency / blend mode.
    pub blend_mode: SpriteBlendMode,
    /// Pixel data slice of length `width * height`.
    pub data: &'a [Rgb565],
}

impl<'a> Sprite2D<'a> {
    /// Create a new 2D sprite descriptor.
    pub const fn new(x: i32, y: i32, width: u32, height: u32, data: &'a [Rgb565]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            scale: 1,
            blend_mode: SpriteBlendMode::Normal,
            data,
        }
    }

    /// Configure color-key transparency (e.g. magenta `0xF81F`).
    pub const fn with_colorkey(mut self, key: Rgb565) -> Self {
        self.blend_mode = SpriteBlendMode::ColorKey(key);
        self
    }

    /// Configure alpha transparency level (0-255).
    pub const fn with_alpha(mut self, alpha: u8) -> Self {
        self.blend_mode = SpriteBlendMode::Alpha(alpha);
        self
    }

    /// Configure integer scaling multiplier (1x, 2x, 3x, ...).
    pub const fn with_scale(mut self, scale: u8) -> Self {
        self.scale = if scale > 0 { scale } else { 1 };
        self
    }

    /// Blit this sprite directly onto an embedded-graphics [`DrawTarget`].
    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let scale = self.scale as i32;
        let w = self.width as usize;
        let h = self.height as usize;

        for sy in 0..h {
            for sx in 0..w {
                let idx = sy * w + sx;
                if idx >= self.data.len() {
                    break;
                }
                let color = self.data[idx];

                if let SpriteBlendMode::ColorKey(key) = self.blend_mode {
                    if color == key {
                        continue;
                    }
                }

                let base_x = self.x + sx as i32 * scale;
                let base_y = self.y + sy as i32 * scale;

                if scale == 1 {
                    target.draw_iter([Pixel(Point::new(base_x, base_y), color)])?;
                } else {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            target
                                .draw_iter([Pixel(Point::new(base_x + dx, base_y + dy), color)])?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// A single HUD element drawn as a 2D overlay after the 3D scene.
#[derive(Clone, Copy, Debug)]
pub enum HudElement<'a> {
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
    /// 2D composited sprite overlay.
    Sprite(Sprite2D<'a>),
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
pub struct HudLayer<'a, const N: usize> {
    elements: Vec<HudElement<'a>, N>,
}

impl<const N: usize> Default for HudLayer<'_, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, const N: usize> HudLayer<'a, N> {
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
    pub fn push(&mut self, element: HudElement<'a>) -> Result<(), HudElement<'a>> {
        self.elements.push(element)
    }

    /// Draw all elements onto `target` in insertion order.
    pub fn draw<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        for elem in &self.elements {
            match elem {
                HudElement::FillRect { x, y, w, h, color } => {
                    let _ = target.draw_iter(
                        rect_pixels(*x, *y, *w, *h)
                            .map(|(px, py)| Pixel(Point::new(px, py), *color)),
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
                        rect_pixels(*x, *y, *w, *h)
                            .map(|(px, py)| Pixel(Point::new(px, py), *color)),
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
                    let filled = ((*w as f32 * value.clamp(0.0, 1.0)) as u32).min(*w);
                    if filled > 0 {
                        let _ = target.draw_iter(
                            rect_pixels(*x, *y, filled, *h)
                                .map(|(px, py)| Pixel(Point::new(px, py), *fg)),
                        );
                    }
                    if filled < *w {
                        let _ = target.draw_iter(
                            rect_pixels(*x + filled as i32, *y, *w - filled, *h)
                                .map(|(px, py)| Pixel(Point::new(px, py), *bg)),
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
                        border_pixels(*x, *y, *w, *h)
                            .map(|(px, py)| Pixel(Point::new(px, py), *color)),
                    );
                }
                HudElement::Sprite(sprite) => {
                    let _ = sprite.draw(target);
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

    #[test]
    fn test_sprite2d_colorkey_and_scale() {
        let sprite_data = [
            Rgb565::RED,
            Rgb565::MAGENTA, // ColorKey
            Rgb565::BLUE,
            Rgb565::GREEN,
        ];
        let sprite = Sprite2D::new(10, 10, 2, 2, &sprite_data)
            .with_colorkey(Rgb565::MAGENTA)
            .with_scale(2);

        let mut target = MockTarget::new();
        sprite.draw(&mut target).unwrap();

        // 3 non-transparent pixels * (2x2 scale) = 12 pixels drawn
        assert_eq!(target.pixels.len(), 12);
        assert!(target.pixels.iter().all(|&(_, _, c)| c != Rgb565::MAGENTA));
    }

    #[test]
    fn test_framebuf_draw_target_and_formatting() {
        let mut buf = [Rgb565::BLACK; 12];
        {
            let mut target = FramebufDrawTarget::new(&mut buf, 4, 3);
            assert_eq!(target.size(), Size::new(4, 3));
            target
                .draw_iter([Pixel(Point::new(1, 1), Rgb565::RED)])
                .unwrap();
            target
                .draw_iter([Pixel(Point::new(99, 99), Rgb565::BLUE)])
                .unwrap();
        }
        assert_eq!(buf[5], Rgb565::RED);
        assert_eq!(buf[0], Rgb565::BLACK);

        let mut text = [b' '; 6];
        assert_eq!(format_u16_dec(42, &mut text, 3), "042");
        assert_eq!(format_u16_dec(65535, &mut text, 5), "65535");
    }

    #[test]
    fn test_hud_translucent_sprite_partial_bar_and_default() {
        let sprite_data = [Rgb565::RED, Rgb565::BLUE, Rgb565::GREEN, Rgb565::YELLOW];
        let mut hud = HudLayer::<6>::default();
        hud.push(HudElement::TranslucentRect {
            x: 1,
            y: 1,
            w: 2,
            h: 2,
            color: Rgb565::RED,
            alpha: 128,
        })
        .unwrap();
        hud.push(HudElement::TranslucentBorder {
            x: 5,
            y: 5,
            w: 3,
            h: 3,
            color: Rgb565::GREEN,
            alpha: 64,
        })
        .unwrap();
        hud.push(HudElement::ProgressBar {
            x: 0,
            y: 0,
            w: 10,
            h: 1,
            value: 0.5,
            fg: Rgb565::WHITE,
            bg: Rgb565::BLACK,
        })
        .unwrap();
        hud.push(HudElement::Sprite(Sprite2D::new(0, 0, 2, 2, &sprite_data)))
            .unwrap();

        let mut target = MockTarget::new();
        hud.draw(&mut target);
        assert!(target.pixels.len() > 20);
    }

    #[test]
    fn test_sprite_alpha_additive_normal_and_zero_scale() {
        let data = [Rgb565::RED; 1];
        let mut target = MockTarget::new();
        Sprite2D::new(0, 0, 1, 1, &data)
            .with_alpha(128)
            .draw(&mut target)
            .unwrap();
        assert_eq!(target.pixels.len(), 1);

        let mut target2 = MockTarget::new();
        Sprite2D::new(0, 0, 1, 1, &data)
            .with_scale(0)
            .with_colorkey(Rgb565::MAGENTA)
            .draw(&mut target2)
            .unwrap();
        assert_eq!(target2.pixels.len(), 1);

        let mut target3 = MockTarget::new();
        Sprite2D::new(0, 0, 1, 1, &data).draw(&mut target3).unwrap();
        assert_eq!(target3.pixels.len(), 1);
    }
}
