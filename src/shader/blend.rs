use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};

/// Fast RGB565 alpha blending.
///
/// Blends `fg` color over `bg` color using an 8-bit alpha value `alpha` ∈ [0, 255].
#[inline(always)]
pub fn fast_blend_rgb565(bg: Rgb565, fg: Rgb565, alpha: u8) -> Rgb565 {
    if alpha == 255 {
        return fg;
    }
    if alpha == 0 {
        return bg;
    }
    let a = alpha as u32;
    let inv = 255 - a;
    let r = (bg.r() as u32 * inv + fg.r() as u32 * a) / 255;
    let g = (bg.g() as u32 * inv + fg.g() as u32 * a) / 255;
    let b = (bg.b() as u32 * inv + fg.b() as u32 * a) / 255;
    Rgb565::new(r as u8, g as u8, b as u8)
}

/// Fast RGBA8888 alpha blending.
///
/// Blends `fg` RGBA8888 channel tuple over `bg` RGBA8888 channel tuple.
#[inline(always)]
pub fn fast_blend_rgba8888(bg: [u8; 4], fg: [u8; 4]) -> [u8; 4] {
    let a = fg[3] as u32;
    if a == 255 {
        return fg;
    }
    if a == 0 {
        return bg;
    }
    let inv = 255 - a;
    let r = (bg[0] as u32 * inv + fg[0] as u32 * a) / 255;
    let g = (bg[1] as u32 * inv + fg[1] as u32 * a) / 255;
    let b = (bg[2] as u32 * inv + fg[2] as u32 * a) / 255;
    let out_a = fg[3] as u32 + (bg[3] as u32 * inv) / 255;
    [r as u8, g as u8, b as u8, out_a as u8]
}

/// Fast RGBA8888 to RGB565 alpha blending.
///
/// Blends an RGBA8888 foreground pixel directly onto an RGB565 background pixel.
#[inline(always)]
pub fn fast_blend_rgba8888_to_rgb565(bg: Rgb565, fg_rgba: [u8; 4]) -> Rgb565 {
    let alpha = fg_rgba[3];
    if alpha == 0 {
        return bg;
    }
    let fg_r = fg_rgba[0] >> 3;
    let fg_g = fg_rgba[1] >> 2;
    let fg_b = fg_rgba[2] >> 3;

    if alpha == 255 {
        return Rgb565::new(fg_r, fg_g, fg_b);
    }

    let fg_color = Rgb565::new(fg_r, fg_g, fg_b);
    fast_blend_rgb565(bg, fg_color, alpha)
}

/// Fast color inversion (reverse color) filter for RGB565.
///
/// Inverts RGB color channels.
#[inline(always)]
pub fn reverse_color_rgb565(c: Rgb565) -> Rgb565 {
    Rgb565::new(31 - c.r(), 63 - c.g(), 31 - c.b())
}

/// Fast color inversion (reverse color) filter for RGBA8888.
///
/// Inverts R, G, B channels while preserving alpha.
#[inline(always)]
pub fn reverse_color_rgba8888(rgba: [u8; 4]) -> [u8; 4] {
    [255 - rgba[0], 255 - rgba[1], 255 - rgba[2], rgba[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::pixelcolor::RgbColor;

    #[test]
    fn test_fast_blend_rgb565_bounds() {
        let red = Rgb565::RED;
        let green = Rgb565::GREEN;

        assert_eq!(fast_blend_rgb565(red, green, 0), red);
        assert_eq!(fast_blend_rgb565(red, green, 255), green);

        let mid = fast_blend_rgb565(Rgb565::BLACK, Rgb565::WHITE, 128);
        assert!(mid.r() > 10 && mid.r() < 25);
        assert!(mid.g() > 20 && mid.g() < 40);
        assert!(mid.b() > 10 && mid.b() < 25);
    }

    #[test]
    fn test_fast_blend_rgba8888_bounds() {
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];

        assert_eq!(fast_blend_rgba8888(black, [255, 255, 255, 0]), black);
        assert_eq!(fast_blend_rgba8888(black, white), white);

        let blended = fast_blend_rgba8888(black, [255, 255, 255, 128]);
        assert_eq!(blended[0], 128);
        assert_eq!(blended[1], 128);
        assert_eq!(blended[2], 128);
        assert_eq!(blended[3], 255);
    }

    #[test]
    fn test_fast_blend_rgba8888_to_rgb565() {
        let bg = Rgb565::BLACK;
        assert_eq!(fast_blend_rgba8888_to_rgb565(bg, [255, 0, 0, 0]), bg);
        assert_eq!(
            fast_blend_rgba8888_to_rgb565(bg, [255, 0, 0, 255]),
            Rgb565::RED
        );

        let mid = fast_blend_rgba8888_to_rgb565(bg, [255, 255, 255, 128]);
        assert!(mid.r() > 0 && mid.r() < 31);
    }

    #[test]
    fn test_reverse_color() {
        assert_eq!(reverse_color_rgb565(Rgb565::BLACK), Rgb565::WHITE);
        assert_eq!(reverse_color_rgb565(Rgb565::WHITE), Rgb565::BLACK);

        let rgba = [10, 20, 30, 200];
        let rev = reverse_color_rgba8888(rgba);
        assert_eq!(rev, [245, 235, 225, 200]);
    }
}
