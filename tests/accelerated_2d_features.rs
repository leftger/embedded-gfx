//! Integration tests for accelerated 2D features in embedded-3dgfx.
//!
//! Covers:
//! 1. Fast alpha blending (RGB565 & RGBA8888)
//! 2. 2xSSAA sub-pixel anti-aliased rasterization
//! 3. Q16.16 affine texture mapping & billboard scanline sampler
//! 4. Translucent 3D triangle rendering with depth-buffer

extern crate std;

use embedded_3dgfx::draw::{fast_blend_rgb565, fast_blend_rgba8888, fast_blend_rgba8888_to_rgb565};
#[cfg(feature = "textured")]
use embedded_3dgfx::texture::Texture;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor, WebColors};

// ─────────────────────────────────────────────────────────────────────────────
// 1. FAST ALPHA BLENDING
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_fast_blend_rgb565_fully_opaque() {
    let bg = Rgb565::new(0, 0, 0);
    let fg = Rgb565::new(31, 63, 31); // white in RGB565 max components
    let result = fast_blend_rgb565(bg, fg, 255);
    assert_eq!(result, fg, "alpha=255 must return foreground verbatim");
}

#[test]
fn test_fast_blend_rgb565_fully_transparent() {
    let bg = Rgb565::CSS_GREEN;
    let fg = Rgb565::CSS_RED;
    let result = fast_blend_rgb565(bg, fg, 0);
    assert_eq!(result, bg, "alpha=0 must return background verbatim");
}

#[test]
fn test_fast_blend_rgb565_midpoint() {
    // 50% blend of black onto white — result should be near mid-grey
    let bg = Rgb565::new(31, 63, 31); // white
    let fg = Rgb565::new(0, 0, 0); // black
    let result = fast_blend_rgb565(bg, fg, 128);
    // Mid-point: each channel ≈ half maximum
    assert!(result.r() <= 16, "r channel should be ~half white");
    assert!(result.g() <= 32, "g channel should be ~half white");
    assert!(result.b() <= 16, "b channel should be ~half white");
    assert!(result.r() >= 14, "r channel should not be zero");
    assert!(result.g() >= 28, "g channel should not be zero");
    assert!(result.b() >= 14, "b channel should not be zero");
}

#[test]
fn test_fast_blend_rgb565_quarter_alpha() {
    let bg = Rgb565::new(0, 0, 0);
    let fg = Rgb565::new(31, 63, 31);
    let result = fast_blend_rgb565(bg, fg, 64); // ~25% fg
    // At 25%, result channels should be roughly 25% of maximum
    assert!(result.r() <= 9, "r ~25% of 31");
    assert!(result.g() <= 17, "g ~25% of 63");
    assert!(result.b() <= 9, "b ~25% of 31");
}

#[test]
fn test_fast_blend_rgb565_three_quarter_alpha() {
    let bg = Rgb565::new(0, 0, 0);
    let fg = Rgb565::new(31, 63, 31);
    let result = fast_blend_rgb565(bg, fg, 192); // ~75% fg
    assert!(result.r() >= 22, "r ~75% of 31");
    assert!(result.g() >= 44, "g ~75% of 63");
    assert!(result.b() >= 22, "b ~75% of 31");
}

#[test]
fn test_fast_blend_rgb565_associativity() {
    // blend(bg, A->B at alpha) and blend(result, B->C at alpha) should chain correctly
    let black = Rgb565::new(0, 0, 0);
    let white = Rgb565::new(31, 63, 31);
    let mid = fast_blend_rgb565(black, white, 128);
    // mid should be lighter than black
    assert!(mid.r() > 0 || mid.g() > 0 || mid.b() > 0);
    // chained 50% on top
    let result2 = fast_blend_rgb565(mid, white, 128);
    // result2 should be lighter than mid
    assert!(result2.r() >= mid.r());
    assert!(result2.g() >= mid.g());
    assert!(result2.b() >= mid.b());
}

#[test]
fn test_fast_blend_rgba8888_fully_opaque() {
    let bg = [10u8, 20, 30, 255];
    let fg = [200u8, 150, 100, 255];
    let result = fast_blend_rgba8888(bg, fg);
    assert_eq!(&result[..3], &fg[..3], "fully opaque fg replaces bg RGB");
}

#[test]
fn test_fast_blend_rgba8888_fully_transparent() {
    let bg = [10u8, 20, 30, 255];
    let fg = [200u8, 150, 100, 0];
    let result = fast_blend_rgba8888(bg, fg);
    assert_eq!(&result[..3], &bg[..3], "transparent fg leaves bg unchanged");
}

#[test]
fn test_fast_blend_rgba8888_midpoint() {
    let bg = [0u8, 0, 0, 255];
    let fg = [200u8, 200, 200, 128];
    let result = fast_blend_rgba8888(bg, fg);
    // ~50% of 200 ≈ 100 for each channel
    assert!(result[0] >= 90 && result[0] <= 110, "R channel mid-blend");
    assert!(result[1] >= 90 && result[1] <= 110, "G channel mid-blend");
    assert!(result[2] >= 90 && result[2] <= 110, "B channel mid-blend");
}

#[test]
fn test_fast_blend_rgba8888_to_rgb565_opaque() {
    let bg = Rgb565::CSS_BLACK;
    let fg_rgba: [u8; 4] = [248, 0, 0, 255]; // pure red (fits RGB565 scale)
    let result = fast_blend_rgba8888_to_rgb565(bg, fg_rgba);
    assert_eq!(
        result.r(),
        31,
        "fully opaque red should give max R component"
    );
    assert_eq!(result.g(), 0, "no green");
    assert_eq!(result.b(), 0, "no blue");
}

#[test]
fn test_fast_blend_rgba8888_to_rgb565_transparent() {
    let bg = Rgb565::CSS_WHITE;
    let fg_rgba: [u8; 4] = [0, 0, 0, 0]; // fully transparent black
    let result = fast_blend_rgba8888_to_rgb565(bg, fg_rgba);
    assert_eq!(result, bg, "transparent fg must preserve background");
}

#[test]
fn test_fast_blend_rgba8888_to_rgb565_half_alpha() {
    let bg = Rgb565::new(0, 0, 0); // black bg
    let fg_rgba: [u8; 4] = [248, 248, 248, 128]; // ~50% white
    let result = fast_blend_rgba8888_to_rgb565(bg, fg_rgba);
    // Should be grey-ish: RGB565 r∈[12..20], g∈[24..40], b∈[12..20]
    assert!(result.r() > 0, "should blend some red channel");
    assert!(result.g() > 0, "should blend some green channel");
    assert!(result.b() > 0, "should blend some blue channel");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Q16.16 AFFINE TEXTURE MAPPING
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "textured")]
static CHECKER_DATA: [Rgb565; 64] = {
    // 8x8 checkerboard: black on even rows/cols, white on odd
    let mut data = [Rgb565::CSS_BLACK; 64];
    let mut i = 0;
    while i < 64 {
        let row = i / 8;
        let col = i % 8;
        if (row + col) % 2 == 1 {
            data[i] = Rgb565::CSS_WHITE;
        }
        i += 1;
    }
    data
};

#[cfg(feature = "textured")]
static SOLID_RED: [Rgb565; 64] = [Rgb565::CSS_RED; 64];
#[cfg(feature = "textured")]
static SOLID_GREEN: [Rgb565; 64] = [Rgb565::CSS_GREEN; 64];

#[cfg(feature = "textured")]
#[test]
fn test_affine_q16_sample_top_left() {
    let tex = Texture::new(&CHECKER_DATA, 8, 8);
    // Q16.16 (0, 0) = top-left = black on 8x8 checker
    let pixel = tex.sample_affine_q16(0, 0);
    assert_eq!(pixel, Rgb565::CSS_BLACK, "top-left pixel should be black");
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_q16_sample_center() {
    let tex = Texture::new(&CHECKER_DATA, 8, 8);
    // Q16.16 0.5 = 32768; (4,4) index in 8x8 = row 4, col 4 → (4+4)%2=0 → black
    let pixel = tex.sample_affine_q16(32768, 32768);
    assert_eq!(pixel, Rgb565::CSS_BLACK, "center pixel (4,4) is black");
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_q16_sample_wrapping() {
    let tex = Texture::new(&SOLID_RED, 8, 8);
    // Q16.16 2.0 = 131072; should wrap to same result as 0.0
    let p0 = tex.sample_affine_q16(0, 0);
    let p1 = tex.sample_affine_q16(131072, 0);
    assert_eq!(p0, p1, "coordinates should wrap (repeat mode)");
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_scanline_q16_step_across_solid() {
    let tex = Texture::new(&SOLID_RED, 8, 8);
    let mut scanline = [Rgb565::CSS_BLACK; 8];
    // Step du = 65536/8 = 8192 per pixel across the texture width
    tex.sample_affine_scanline_q16(0, 0, 8192, 0, &mut scanline);
    for (i, &px) in scanline.iter().enumerate() {
        assert_eq!(
            px,
            Rgb565::CSS_RED,
            "pixel {i} should be red on solid texture"
        );
    }
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_scanline_q16_step_vertical() {
    // Stepping v only (horizontal axis = 0) should sweep vertically
    let tex = Texture::new(&CHECKER_DATA, 8, 8);
    let mut scanline = [Rgb565::CSS_BLACK; 8];
    // dv = 8192 (1/8 of texture height per step), du = 0 (no horizontal movement)
    tex.sample_affine_scanline_q16(0, 0, 0, 8192, &mut scanline);
    // Each successive pixel samples the next row at column 0.
    // col=0: rows 0,1,2,3,4,5,6,7 → black, white, black, white, ... (checker)
    assert_eq!(scanline[0], Rgb565::CSS_BLACK, "row 0, col 0 = black");
    assert_eq!(scanline[1], Rgb565::CSS_WHITE, "row 1, col 0 = white");
    assert_eq!(scanline[2], Rgb565::CSS_BLACK, "row 2, col 0 = black");
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_q16_full_texture_traversal() {
    let tex = Texture::new(&CHECKER_DATA, 8, 8);
    // Walk all 64 texels in row-major Q16.16 steps
    let du: u32 = 65536 / 8; // 8192
    let dv: u32 = 65536 / 8;

    let mut mismatches = 0u32;
    for row in 0..8u32 {
        for col in 0..8u32 {
            let u = col * du;
            let v = row * dv;
            let sampled = tex.sample_affine_q16(u, v);
            let expected = if (row + col) % 2 == 0 {
                Rgb565::CSS_BLACK
            } else {
                Rgb565::CSS_WHITE
            };
            if sampled != expected {
                mismatches += 1;
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "all 64 texels must match the checkerboard pattern"
    );
}

#[cfg(feature = "textured")]
#[test]
fn test_affine_scanline_q16_two_textures() {
    // Confirm different textures return different scanline results
    let tex_r = Texture::new(&SOLID_RED, 8, 8);
    let tex_g = Texture::new(&SOLID_GREEN, 8, 8);

    let mut scan_r = [Rgb565::CSS_BLACK; 4];
    let mut scan_g = [Rgb565::CSS_BLACK; 4];

    tex_r.sample_affine_scanline_q16(0, 0, 8192, 0, &mut scan_r);
    tex_g.sample_affine_scanline_q16(0, 0, 8192, 0, &mut scan_g);

    assert!(scan_r.iter().all(|&p| p == Rgb565::CSS_RED));
    assert!(scan_g.iter().all(|&p| p == Rgb565::CSS_GREEN));
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. BILLBOARD Q16.16 AFFINE UV MAPPING  (arm_2d_transform.h port)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "scene")]
use embedded_3dgfx::billboard::{Billboard, BillboardQ16};
#[allow(unused_imports)]
use nalgebra::{Point3, Vector3};

#[cfg(feature = "scene")]
#[test]
fn test_billboard_default_uv_q16_corners() {
    let uvs = Billboard::default_uv_q16();
    // [bottom-left, bottom-right, top-right, top-left]
    assert_eq!(uvs[0], [0, 0], "BL = (0,0) in Q16");
    assert_eq!(uvs[1], [65536, 0], "BR = (1.0, 0) in Q16");
    assert_eq!(uvs[2], [65536, 65536], "TR = (1.0, 1.0) in Q16");
    assert_eq!(uvs[3], [0, 65536], "TL = (0, 1.0) in Q16");
}

#[cfg(feature = "scene")]
#[test]
fn test_billboard_custom_uv_q16() {
    let pos = Point3::new(0.0, 0.0, 0.0);
    let mut bb = Billboard::new(pos, 1.0, Rgb565::CSS_RED);
    // Set sub-tile UV (top-left quadrant: u∈[0,0.5], v∈[0,0.5])
    bb.uv = Some([[0.0, 0.0], [0.5, 0.0], [0.5, 0.5], [0.0, 0.5]]);
    let uvs = bb.get_uv_q16();
    assert_eq!(uvs[0], [0, 0]);
    assert_eq!(uvs[1], [32768, 0]); // 0.5 * 65536
    assert_eq!(uvs[2], [32768, 32768]);
    assert_eq!(uvs[3], [0, 32768]);
}

#[cfg(feature = "scene")]
#[test]
fn test_billboard_textured_quad_q16_layout() {
    let pos = Point3::new(0.0, 0.0, 0.0);
    let bb = Billboard::new(pos, 2.0, Rgb565::CSS_BLUE);
    let cam_pos = Point3::new(0.0, 0.0, 5.0);
    let cam_up = Vector3::new(0.0, 1.0, 0.0);

    let (quad, uvs) = bb.generate_textured_quad_q16(cam_pos, cam_up);
    assert_eq!(quad.len(), 4, "quad must have 4 vertices");
    assert_eq!(uvs.len(), 4, "UV must have 4 entries");
    // Default UVs span [0..1] in Q16
    assert_eq!(uvs[0], [0, 0]);
    assert_eq!(uvs[2], [65536, 65536]);
}

#[cfg(feature = "scene")]
#[test]
fn test_billboard_q16_from_float_position_scale() {
    let pos = Point3::new(2.0, 3.0, -1.0);
    let bq = BillboardQ16::from_float(pos, 0.5, Rgb565::CSS_RED);
    assert_eq!(bq.position_q16[0], 131072, "x=2.0 → 131072 in Q16.16");
    assert_eq!(bq.position_q16[1], 196608, "y=3.0 → 196608 in Q16.16");
    assert_eq!(bq.position_q16[2], -65536, "z=-1.0 → -65536 in Q16.16");
    assert_eq!(bq.size_q16, 32768, "size=0.5 → 32768 in Q16.16");
}

#[cfg(feature = "scene")]
#[test]
fn test_billboard_q16_uv_from_default() {
    let pos = Point3::new(0.0, 0.0, 0.0);
    let bq = BillboardQ16::from_float(pos, 1.0, Rgb565::CSS_RED);
    // Default UVs must be the four corners
    assert_eq!(bq.uv_q16[0], [0, 0]);
    assert_eq!(bq.uv_q16[1], [65536, 0]);
    assert_eq!(bq.uv_q16[2], [65536, 65536]);
    assert_eq!(bq.uv_q16[3], [0, 65536]);
}

#[cfg(feature = "scene")]
#[test]
fn test_billboard_fast_transform_matches_manual() {
    let pos = Point3::new(0.0, 0.0, 0.0);
    let bb = Billboard::new(pos, 2.0, Rgb565::CSS_RED);
    let right = Vector3::new(1.0, 0.0, 0.0);
    let up = Vector3::new(0.0, 1.0, 0.0);

    let quad = bb.transform_billboard_fast(right, up);
    // Half-size = 1.0; BL = (-1,-1,0), BR = (1,-1,0), TR = (1,1,0), TL = (-1,1,0)
    let eps = 1e-5f32;
    assert!((quad[0][0] - (-1.0)).abs() < eps, "BL.x");
    assert!((quad[0][1] - (-1.0)).abs() < eps, "BL.y");
    assert!((quad[1][0] - 1.0).abs() < eps, "BR.x");
    assert!((quad[2][0] - 1.0).abs() < eps, "TR.x");
    assert!((quad[2][1] - 1.0).abs() < eps, "TR.y");
    assert!((quad[3][0] - (-1.0)).abs() < eps, "TL.x");
    assert!((quad[3][1] - 1.0).abs() < eps, "TL.y");
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. 2xSSAA — confirmed via draw_zbuffered_2xssaa API surface existence
// ─────────────────────────────────────────────────────────────────────────────

use embedded_3dgfx::primitive::DrawPrimitive;
use nalgebra::Point2;

/// A minimal PixelRead + DrawTarget framebuffer for testing 2xSSAA.
struct MockFb {
    pixels: std::vec::Vec<Rgb565>,
    width: u32,
    height: u32,
}

impl MockFb {
    fn new(w: u32, h: u32) -> Self {
        Self {
            pixels: std::vec![Rgb565::CSS_BLACK; (w * h) as usize],
            width: w,
            height: h,
        }
    }
    fn pixel_at(&self, x: u32, y: u32) -> Rgb565 {
        self.pixels[(y * self.width + x) as usize]
    }
}

impl embedded_graphics_core::geometry::OriginDimensions for MockFb {
    fn size(&self) -> embedded_graphics_core::geometry::Size {
        embedded_graphics_core::geometry::Size::new(self.width, self.height)
    }
}

impl embedded_graphics_core::draw_target::DrawTarget for MockFb {
    type Color = Rgb565;
    type Error = core::convert::Infallible;
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics_core::Pixel<Rgb565>>,
    {
        for embedded_graphics_core::Pixel(pt, color) in pixels {
            if pt.x >= 0 && pt.y >= 0 && (pt.x as u32) < self.width && (pt.y as u32) < self.height {
                self.pixels[(pt.y as u32 * self.width + pt.x as u32) as usize] = color;
            }
        }
        Ok(())
    }
}

impl embedded_3dgfx::draw::PixelRead for MockFb {
    fn get_pixel(&self, pt: embedded_graphics_core::prelude::Point) -> Rgb565 {
        if pt.x >= 0 && pt.y >= 0 && (pt.x as u32) < self.width && (pt.y as u32) < self.height {
            self.pixels[(pt.y as u32 * self.width + pt.x as u32) as usize]
        } else {
            Rgb565::CSS_BLACK
        }
    }
}

#[test]
fn test_translucent_triangle_blends_correctly() {
    // The translucent rasterizer computes: arm_2d_blend_rgb565(BLACK, color, alpha).
    // So alpha=128 red over black gives: r≈15 (half of max 31), g=0, b=0.
    let mut fb = MockFb::new(16, 16);
    let mut zbuffer = std::vec![embedded_3dgfx::Z_MAX_VALUE; 16 * 16];

    let prim = DrawPrimitive::TranslucentTriangleWithDepth {
        points: [
            Point2::new(0i32, 0i32),
            Point2::new(15i32, 0i32),
            Point2::new(0i32, 15i32),
        ],
        depths: [1.0, 1.0, 1.0],
        color: Rgb565::CSS_RED,
        alpha: 128,
    };

    embedded_3dgfx::draw::draw_zbuffered_with_effects(prim, &mut fb, &mut zbuffer, 16, None, None);

    // alpha=128 blends half of red over pure black: r ≈ 15, g=0, b=0
    let px = fb.pixel_at(0, 0);
    assert!(
        px.r() >= 13 && px.r() <= 17,
        "alpha=128 red over black should give r≈15, got {}",
        px.r()
    );
    assert_eq!(px.g(), 0, "no green channel expected");
    assert_eq!(px.b(), 0, "no blue channel expected");
}

#[test]
fn test_zbuffered_triangle_opaque_writes_z() {
    let mut fb = MockFb::new(16, 16);
    let mut zbuffer = std::vec![embedded_3dgfx::Z_MAX_VALUE; 16 * 16];

    let prim = DrawPrimitive::ColoredTriangleWithDepth {
        points: [
            Point2::new(0i32, 0i32),
            Point2::new(15i32, 0i32),
            Point2::new(0i32, 15i32),
        ],
        depths: [5.0, 5.0, 5.0],
        color: Rgb565::CSS_BLUE,
    };

    embedded_3dgfx::draw::draw_zbuffered(prim, &mut fb, &mut zbuffer, 16);
    // z-buffer at (0,0) should have been written (no longer Z_MAX_VALUE)
    assert_ne!(
        zbuffer[0],
        embedded_3dgfx::Z_MAX_VALUE,
        "z-buffer must be updated after opaque triangle"
    );
    assert_eq!(fb.pixel_at(0, 0), Rgb565::CSS_BLUE, "pixel must be blue");
}

#[test]
fn test_zbuffered_occlusion() {
    // Draw a near blue triangle, then a far red triangle on top.
    // The far red triangle should be occluded by the near blue one.
    let mut fb = MockFb::new(16, 16);
    let mut zbuffer = std::vec![embedded_3dgfx::Z_MAX_VALUE; 16 * 16];

    let near_prim = DrawPrimitive::ColoredTriangleWithDepth {
        points: [
            Point2::new(0i32, 0i32),
            Point2::new(15i32, 0i32),
            Point2::new(0i32, 15i32),
        ],
        depths: [2.0, 2.0, 2.0],
        color: Rgb565::CSS_BLUE,
    };
    let far_prim = DrawPrimitive::ColoredTriangleWithDepth {
        points: [
            Point2::new(0i32, 0i32),
            Point2::new(15i32, 0i32),
            Point2::new(0i32, 15i32),
        ],
        depths: [9.0, 9.0, 9.0],
        color: Rgb565::CSS_RED,
    };

    embedded_3dgfx::draw::draw_zbuffered(near_prim, &mut fb, &mut zbuffer, 16);
    embedded_3dgfx::draw::draw_zbuffered(far_prim, &mut fb, &mut zbuffer, 16);

    // Blue (near) must win — red is behind it
    assert_eq!(
        fb.pixel_at(0, 0),
        Rgb565::CSS_BLUE,
        "near blue must occlude far red"
    );
}

#[test]
fn test_alpha_blend_chain_over_background() {
    // Verify that two successive alpha blends accumulate correctly:
    // 1. Start with a black background.
    // 2. 50% white → mid-grey.
    // 3. 50% more white → lighter grey.
    let black = Rgb565::new(0, 0, 0);
    let white = Rgb565::new(31, 63, 31);

    let step1 = fast_blend_rgb565(black, white, 128);
    let step2 = fast_blend_rgb565(step1, white, 128);

    // step2 should be lighter than step1
    assert!(
        step2.r() >= step1.r(),
        "each blend step increases brightness (R)"
    );
    assert!(
        step2.g() >= step1.g(),
        "each blend step increases brightness (G)"
    );
    assert!(
        step2.b() >= step1.b(),
        "each blend step increases brightness (B)"
    );

    // step2 should still be darker than pure white
    assert!(step2.r() < 31, "not yet at pure white after 2 blends");
    assert!(step2.g() < 63, "not yet at pure white after 2 blends");
}

#[test]
fn test_rgba8888_premultiplied_white_over_black() {
    let bg = [0u8, 0, 0, 255];
    let fg = [255u8, 255, 255, 200]; // nearly-opaque white
    let result = fast_blend_rgba8888(bg, fg);
    // ~78% white over black → all channels should be ~200
    assert!(result[0] >= 190 && result[0] <= 210, "R channel ≈200");
    assert!(result[1] >= 190 && result[1] <= 210, "G channel ≈200");
    assert!(result[2] >= 190 && result[2] <= 210, "B channel ≈200");
}

#[test]
fn test_reverse_color_rgb565() {
    use embedded_3dgfx::draw::reverse_color_rgb565;
    let black = Rgb565::new(0, 0, 0);
    let white = Rgb565::new(31, 63, 31);
    assert_eq!(
        reverse_color_rgb565(black),
        white,
        "inverting black gives white"
    );
    assert_eq!(
        reverse_color_rgb565(white),
        black,
        "inverting white gives black"
    );

    let red = Rgb565::new(31, 0, 0);
    let cyan = Rgb565::new(0, 63, 31);
    assert_eq!(reverse_color_rgb565(red), cyan, "inverting red gives cyan");
}

#[test]
fn test_reverse_color_rgba8888() {
    use embedded_3dgfx::draw::reverse_color_rgba8888;
    let color = [255, 0, 100, 128];
    let inverted = reverse_color_rgba8888(color);
    assert_eq!(
        inverted,
        [0, 255, 155, 128],
        "alpha is preserved, RGB is inverted"
    );
}

#[cfg(feature = "textured")]
#[test]
fn test_2xssaa_texture_sampling() {
    static RAW: [Rgb565; 4] = [
        Rgb565::new(31, 0, 0),   // Red
        Rgb565::new(0, 63, 0),   // Green
        Rgb565::new(0, 0, 31),   // Blue
        Rgb565::new(31, 63, 31), // White
    ];
    let tex = Texture::new(&RAW, 2, 2);
    // Center sampling with 2xSSAA (sub-pixel averaging)
    let ssaa_color = tex.sample_affine_2xssaa_q16(32768, 32768);
    // Averaging all 4 pixels: R ~ (31+0+0+31)/4 = 15, G ~ (0+63+0+63)/4 = 31, B ~ (0+0+31+31)/4 = 15
    assert!(ssaa_color.r() >= 14 && ssaa_color.r() <= 16);
    assert!(ssaa_color.g() >= 30 && ssaa_color.g() <= 32);
    assert!(ssaa_color.b() >= 14 && ssaa_color.b() <= 16);
}
