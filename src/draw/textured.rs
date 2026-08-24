//! Textured, lightmapped, and BSP coverage triangle rasterization.

use core::fmt::Debug;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics_core::prelude::Point;

use super::effects::{DitherConfig, FogConfig};
#[cfg(feature = "lighting")]
use super::fill::interpolate_color;
use super::zbuffered::draw_zbuffered_with_effects;
use crate::primitive::DrawPrimitive;
use crate::retro::{PaletteMode, ScreenTint, StippleMode, TextureMapping};

#[inline]
pub(crate) fn interpolate_uv(
    t: f32,
    w_left: f32,
    w_right: f32,
    uv_left: [f32; 2],
    uv_right: [f32; 2],
    mapping: TextureMapping,
) -> [f32; 2] {
    match mapping {
        TextureMapping::Affine => [
            uv_left[0] + t * (uv_right[0] - uv_left[0]),
            uv_left[1] + t * (uv_right[1] - uv_left[1]),
        ],
        TextureMapping::PerspectiveCorrect => {
            let inv_w_l = if w_left != 0.0 { 1.0 / w_left } else { 1.0 };
            let inv_w_r = if w_right != 0.0 { 1.0 / w_right } else { 1.0 };
            let inv_w = inv_w_l + t * (inv_w_r - inv_w_l);
            let w = if inv_w != 0.0 { 1.0 / inv_w } else { 1.0 };

            let u_over_w_l = uv_left[0] * inv_w_l;
            let u_over_w_r = uv_right[0] * inv_w_r;
            let u_over_w = u_over_w_l + t * (u_over_w_r - u_over_w_l);

            let v_over_w_l = uv_left[1] * inv_w_l;
            let v_over_w_r = uv_right[1] * inv_w_r;
            let v_over_w = v_over_w_l + t * (v_over_w_r - v_over_w_l);

            [u_over_w * w, v_over_w * w]
        }
    }
}

#[inline]
pub(crate) fn should_skip_stipple(x: i32, y: i32, stipple_mode: StippleMode) -> bool {
    match stipple_mode {
        StippleMode::Off => false,
        StippleMode::Checkerboard => ((x ^ y) & 1) != 0,
    }
}

#[inline]
pub fn draw_zbuffered_with_textures<D: DrawTarget<Color = Rgb565>, const N: usize>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_manager: &crate::texture::TextureManager<N>,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
) where
    <D as DrawTarget>::Error: Debug,
{
    draw_zbuffered_with_textures_mapped(
        primitive,
        fb,
        zbuffer,
        width,
        texture_manager,
        fog_config,
        dither_config,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );
}

#[inline]
pub fn draw_zbuffered_with_textures_mapped<D: DrawTarget<Color = Rgb565>, const N: usize>(
    primitive: DrawPrimitive,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_manager: &crate::texture::TextureManager<N>,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    match primitive {
        DrawPrimitive::TexturedTriangleWithDepth {
            mut points,
            mut depths,
            mut ws,
            mut uvs,
            texture_id,
        } => {
            if let Some(texture) = texture_manager.get(texture_id) {
                if points[0].y > points[1].y {
                    points.swap(0, 1);
                    depths.swap(0, 1);
                    ws.swap(0, 1);
                    uvs.swap(0, 1);
                }
                if points[0].y > points[2].y {
                    points.swap(0, 2);
                    depths.swap(0, 2);
                    ws.swap(0, 2);
                    uvs.swap(0, 2);
                }
                if points[1].y > points[2].y {
                    points.swap(1, 2);
                    depths.swap(1, 2);
                    ws.swap(1, 2);
                    uvs.swap(1, 2);
                }

                let [p1, p2, p3] = points;
                let [z1, z2, z3] = depths;
                let [w1, w2, w3] = ws;
                let [uv1, uv2, uv3] = uvs;

                let scr_w = width as i32;
                let scr_h = (zbuffer.len() / width) as i32;
                if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                    return;
                }
                if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                    return;
                }
                if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                    return;
                }
                if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                    return;
                }

                fill_triangle_zbuffered_textured(
                    p1,
                    p2,
                    p3,
                    z1,
                    z2,
                    z3,
                    w1,
                    w2,
                    w3,
                    uv1,
                    uv2,
                    uv3,
                    texture,
                    fb,
                    zbuffer,
                    width,
                    fog_config,
                    dither_config,
                    texture_mapping,
                    stipple_mode,
                    screen_tint,
                    palette_mode,
                );
            }
        }
        #[cfg(feature = "textured")]
        DrawPrimitive::TexturedGouraudTriangleWithDepth {
            mut points,
            mut depths,
            mut ws,
            mut uvs,
            mut colors,
            texture_id,
        } => {
            if let Some(texture) = texture_manager.get(texture_id) {
                if points[0].y > points[1].y {
                    points.swap(0, 1);
                    depths.swap(0, 1);
                    ws.swap(0, 1);
                    uvs.swap(0, 1);
                    colors.swap(0, 1);
                }
                if points[0].y > points[2].y {
                    points.swap(0, 2);
                    depths.swap(0, 2);
                    ws.swap(0, 2);
                    uvs.swap(0, 2);
                    colors.swap(0, 2);
                }
                if points[1].y > points[2].y {
                    points.swap(1, 2);
                    depths.swap(1, 2);
                    ws.swap(1, 2);
                    uvs.swap(1, 2);
                    colors.swap(1, 2);
                }

                let [p1, p2, p3] = points;
                let [z1, z2, z3] = depths;
                let [w1, w2, w3] = ws;
                let [uv1, uv2, uv3] = uvs;
                let [c1, c2, c3] = colors;

                let scr_w = width as i32;
                let scr_h = (zbuffer.len() / width) as i32;
                if p1.x < 0 && p2.x < 0 && p3.x < 0 {
                    return;
                }
                if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
                    return;
                }
                if p1.y < 0 && p2.y < 0 && p3.y < 0 {
                    return;
                }
                if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
                    return;
                }

                fill_triangle_zbuffered_textured_gouraud(
                    p1,
                    p2,
                    p3,
                    z1,
                    z2,
                    z3,
                    w1,
                    w2,
                    w3,
                    uv1,
                    uv2,
                    uv3,
                    c1,
                    c2,
                    c3,
                    texture,
                    fb,
                    zbuffer,
                    width,
                    fog_config,
                    dither_config,
                    texture_mapping,
                    stipple_mode,
                    screen_tint,
                    palette_mode,
                );
            }
        }
        _ => draw_zbuffered_with_effects(primitive, fb, zbuffer, width, fog_config, dither_config),
    }
}

pub fn draw_zbuffered_lightmapped<D: DrawTarget<Color = Rgb565>, const N: usize>(
    points: [nalgebra::Point2<i32>; 3],
    depths: [f32; 3],
    ws: [f32; 3],
    surface_uvs: [[f32; 2]; 3],
    lm_uvs: [[f32; 2]; 3],
    texture_id: u32,
    lightmap_id: u32,
    brightness: u8,
    dynamic_tint: Rgb565,
    fog_config: Option<&FogConfig>,
    texture_manager: &crate::texture::TextureManager<N>,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
) where
    <D as DrawTarget>::Error: Debug,
{
    draw_zbuffered_lightmapped_mapped(
        points,
        depths,
        ws,
        surface_uvs,
        lm_uvs,
        texture_id,
        lightmap_id,
        brightness,
        dynamic_tint,
        fog_config,
        texture_manager,
        fb,
        zbuffer,
        width,
        TextureMapping::PerspectiveCorrect,
        StippleMode::Off,
        None,
        PaletteMode::Off,
    );
}

pub fn draw_zbuffered_lightmapped_mapped<D: DrawTarget<Color = Rgb565>, const N: usize>(
    mut points: [nalgebra::Point2<i32>; 3],
    mut depths: [f32; 3],
    mut ws: [f32; 3],
    mut surface_uvs: [[f32; 2]; 3],
    mut lm_uvs: [[f32; 2]; 3],
    texture_id: u32,
    lightmap_id: u32,
    brightness: u8,
    dynamic_tint: Rgb565,
    fog_config: Option<&FogConfig>,
    texture_manager: &crate::texture::TextureManager<N>,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let surf = match texture_manager.get(texture_id) {
        Some(t) => t,
        None => return,
    };
    let lm = if lightmap_id == u32::MAX {
        None
    } else {
        texture_manager.get(lightmap_id)
    };

    macro_rules! swap_all {
        ($i:expr, $j:expr) => {
            points.swap($i, $j);
            depths.swap($i, $j);
            ws.swap($i, $j);
            surface_uvs.swap($i, $j);
            lm_uvs.swap($i, $j);
        };
    }
    if points[0].y > points[1].y {
        swap_all!(0, 1);
    }
    if points[0].y > points[2].y {
        swap_all!(0, 2);
    }
    if points[1].y > points[2].y {
        swap_all!(1, 2);
    }

    let [p1, p2, p3] = points;
    let [z1, z2, z3] = depths;
    let [w1, w2, w3] = ws;
    let [uv1, uv2, uv3] = surface_uvs;
    let [luv1, luv2, luv3] = lm_uvs;

    let scr_w = width as i32;
    let scr_h = (zbuffer.len() / width) as i32;
    if p1.x < 0 && p2.x < 0 && p3.x < 0 {
        return;
    }
    if p1.x >= scr_w && p2.x >= scr_w && p3.x >= scr_w {
        return;
    }
    if p1.y < 0 && p2.y < 0 && p3.y < 0 {
        return;
    }
    if p1.y >= scr_h && p2.y >= scr_h && p3.y >= scr_h {
        return;
    }

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2.y == p3.y {
        fill_lm_bottom_flat(
            p1,
            p2,
            p3,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            luv1,
            luv2,
            luv3,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
    } else if p1.y == p2.y {
        fill_lm_top_flat(
            p1,
            p2,
            p3,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            luv1,
            luv2,
            luv3,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
    } else {
        let dy31 = (p3.y - p1.y) as f32;
        let dy21 = (p2.y - p1.y) as f32;
        let t = dy21 / dy31;
        let p4x = p1.x + ((p3.x - p1.x) as f32 * t) as i32;
        let p4 = Point::new(p4x, p2.y);
        let z4_int = (z1_int as f32 + (z3_int as f32 - z1_int as f32) * t) as u32;
        let w4 = w1 + (w3 - w1) * t;
        let uv4 = [
            uv1[0] + (uv3[0] - uv1[0]) * t,
            uv1[1] + (uv3[1] - uv1[1]) * t,
        ];
        let luv4 = [
            luv1[0] + (luv3[0] - luv1[0]) * t,
            luv1[1] + (luv3[1] - luv1[1]) * t,
        ];
        let p4_2 = nalgebra::Point2::new(p4.x, p4.y);
        fill_lm_bottom_flat(
            p1,
            p2,
            p4_2,
            z1_int,
            z2_int,
            z4_int,
            w1,
            w2,
            w4,
            uv1,
            uv2,
            uv4,
            luv1,
            luv2,
            luv4,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
        fill_lm_top_flat(
            p2,
            p4_2,
            p3,
            z2_int,
            z4_int,
            z3_int,
            w2,
            w4,
            w3,
            uv2,
            uv4,
            uv3,
            luv2,
            luv4,
            luv3,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn fill_lm_bottom_flat<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    luv1: [f32; 2],
    luv2: [f32; 2],
    luv3: [f32; 2],
    dynamic_tint: Rgb565,
    fog_config: Option<&FogConfig>,
    surf: &crate::texture::Texture,
    lm: Option<&crate::texture::Texture>,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
    brightness: u8,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;
    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;
    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;
        let z_l = (z1 as i64 + (z2 as i64 - z1 as i64) * dy as i64 / height as i64) as u32;
        let z_r = (z1 as i64 + (z3 as i64 - z1 as i64) * dy as i64 / height as i64) as u32;
        let wl = w1 + t * (w2 - w1);
        let wr = w1 + t * (w3 - w1);
        let uvl = [
            uv1[0] + t * (uv2[0] - uv1[0]),
            uv1[1] + t * (uv2[1] - uv1[1]),
        ];
        let uvr = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];
        let luvl = [
            luv1[0] + t * (luv2[0] - luv1[0]),
            luv1[1] + t * (luv2[1] - luv1[1]),
        ];
        let luvr = [
            luv1[0] + t * (luv3[0] - luv1[0]),
            luv1[1] + t * (luv3[1] - luv1[1]),
        ];
        draw_scanline_lm(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_l,
            z_r,
            wl,
            wr,
            uvl,
            uvr,
            luvl,
            luvr,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn fill_lm_top_flat<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    luv1: [f32; 2],
    luv2: [f32; 2],
    luv3: [f32; 2],
    dynamic_tint: Rgb565,
    fog_config: Option<&FogConfig>,
    surf: &crate::texture::Texture,
    lm: Option<&crate::texture::Texture>,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
    brightness: u8,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }
    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;
    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;
    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;
        let z_l = (z1 as i64 + (z3 as i64 - z1 as i64) * dy as i64 / height as i64) as u32;
        let z_r = (z2 as i64 + (z3 as i64 - z2 as i64) * dy as i64 / height as i64) as u32;
        let wl = w1 + t * (w3 - w1);
        let wr = w2 + t * (w3 - w2);
        let uvl = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];
        let uvr = [
            uv2[0] + t * (uv3[0] - uv2[0]),
            uv2[1] + t * (uv3[1] - uv2[1]),
        ];
        let luvl = [
            luv1[0] + t * (luv3[0] - luv1[0]),
            luv1[1] + t * (luv3[1] - luv1[1]),
        ];
        let luvr = [
            luv2[0] + t * (luv3[0] - luv2[0]),
            luv2[1] + t * (luv3[0] - luv2[1]),
        ];
        draw_scanline_lm(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_l,
            z_r,
            wl,
            wr,
            uvl,
            uvr,
            luvl,
            luvr,
            dynamic_tint,
            fog_config,
            surf,
            lm,
            fb,
            zbuffer,
            width,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
            brightness,
        );
        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn draw_scanline_lm<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    w1: f32,
    w2: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    luv1: [f32; 2],
    luv2: [f32; 2],
    dynamic_tint: Rgb565,
    fog_config: Option<&FogConfig>,
    surf: &crate::texture::Texture,
    lm: Option<&crate::texture::Texture>,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
    brightness: u8,
) where
    <D as DrawTarget>::Error: Debug,
{
    if y < 0 {
        return;
    }
    let height = zbuffer.len() / width;
    if y as usize >= height {
        return;
    }

    let (left_x, right_x, z_left, z_right, w_left, w_right, uv_left, uv_right, luv_left, luv_right) =
        if x1 <= x2 {
            (x1, x2, z1, z2, w1, w2, uv1, uv2, luv1, luv2)
        } else {
            (x2, x1, z2, z1, w2, w1, uv2, uv1, luv2, luv1)
        };

    let start_x = left_x.max(0);
    let end_x = right_x.min(width as i32 - 1);
    if start_x > end_x {
        return;
    }

    let span = right_x - left_x;
    let inv_span = if span > 0 { 1.0 / span as f32 } else { 0.0 };
    let z_step = if span > 0 {
        (((z_right as i64 - z_left as i64) << 16) / span as i64) as i32
    } else {
        0
    };

    let left_clip = start_x - left_x;
    let mut z_curr = ((z_left as i64) << 16) + (left_clip as i64 * z_step as i64);
    let mut zbuf_idx = y as usize * width + start_x as usize;

    for x in start_x..=end_x {
        if should_skip_stipple(x, y, stipple_mode) {
            z_curr += z_step as i64;
            zbuf_idx += 1;
            continue;
        }

        let z = (z_curr >> 16) as u32;
        z_curr += z_step as i64;
        let z_depth = crate::to_zdepth(z);

        if z_depth >= zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
            zbuf_idx += 1;
            continue;
        }
        zbuffer[zbuf_idx] = z_depth;

        let t = (x - left_x) as f32 * inv_span;
        let [su, sv] = interpolate_uv(t, w_left, w_right, uv_left, uv_right, texture_mapping);
        let surf_c = surf.sample(su, sv);

        let lit_c = if let Some(lm_tex) = lm {
            let [lu, lv] = interpolate_uv(t, w_left, w_right, luv_left, luv_right, texture_mapping);
            let lm_c = lm_tex.sample(lu, lv);
            let r = ((surf_c.r() as u32 * lm_c.r() as u32) / 31).min(31) as u8;
            let g = ((surf_c.g() as u32 * lm_c.g() as u32) / 63).min(63) as u8;
            let b = ((surf_c.b() as u32 * lm_c.b() as u32) / 31).min(31) as u8;
            Rgb565::new(r, g, b)
        } else {
            surf_c
        };

        let lit_c = if brightness < 255 {
            let scale = brightness as u32;
            let r = ((lit_c.r() as u32 * scale) / 255) as u8;
            let g = ((lit_c.g() as u32 * scale) / 255) as u8;
            let b = ((lit_c.b() as u32 * scale) / 255) as u8;
            Rgb565::new(r, g, b)
        } else {
            lit_c
        };

        let tinted_c = Rgb565::new(
            (lit_c.r() as u16 + dynamic_tint.r() as u16).min(31) as u8,
            (lit_c.g() as u16 + dynamic_tint.g() as u16).min(63) as u8,
            (lit_c.b() as u16 + dynamic_tint.b() as u16).min(31) as u8,
        );

        let mut final_c = if let Some(fog) = fog_config {
            fog.apply(tinted_c, z)
        } else {
            tinted_c
        };

        if let Some(tint) = screen_tint {
            final_c = tint.apply(final_c);
        }
        final_c = palette_mode.apply(final_c);

        fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_c)])
            .unwrap();
    }
}

#[cfg(feature = "raycast")]
pub fn draw_bsp_coverage<D: DrawTarget<Color = Rgb565>, const N: usize>(
    mut points: [nalgebra::Point2<i32>; 3],
    mut ws: [f32; 3],
    mut uvs: [[f32; 2]; 3],
    texture_id: u32,
    texture_manager: &crate::texture::TextureManager<N>,
    fb: &mut D,
    coverage: &mut crate::bsp::coverage::CoverageBuffer<'_>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let tex = match texture_manager.get(texture_id) {
        Some(t) => t,
        None => return,
    };

    if points[0].y > points[1].y {
        points.swap(0, 1);
        ws.swap(0, 1);
        uvs.swap(0, 1);
    }
    if points[0].y > points[2].y {
        points.swap(0, 2);
        ws.swap(0, 2);
        uvs.swap(0, 2);
    }
    if points[1].y > points[2].y {
        points.swap(1, 2);
        ws.swap(1, 2);
        uvs.swap(1, 2);
    }

    let [p1, p2, p3] = points;
    let [w1, w2, w3] = ws;
    let [uv1, uv2, uv3] = uvs;

    let w = coverage.width as i32;
    let h = coverage.height as i32;
    if p1.x < 0 && p2.x < 0 && p3.x < 0 {
        return;
    }
    if p1.x >= w && p2.x >= w && p3.x >= w {
        return;
    }
    if p1.y < 0 && p2.y < 0 && p3.y < 0 {
        return;
    }
    if p1.y >= h && p2.y >= h && p3.y >= h {
        return;
    }

    let rasterize_span =
        |x1: i32,
         x2: i32,
         y: i32,
         wl: f32,
         wr: f32,
         uvl: [f32; 2],
         uvr: [f32; 2],
         fb: &mut D,
         coverage: &mut crate::bsp::coverage::CoverageBuffer<'_>| {
            let start = x1.min(x2);
            let end = x1.max(x2);
            let span = end - start;
            for x in start..=end {
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                if coverage.is_covered(x as usize, y as usize) {
                    continue;
                }
                if should_skip_stipple(x, y, stipple_mode) {
                    continue;
                }
                let t = if span > 0 {
                    (x - start) as f32 / span as f32
                } else {
                    0.0
                };
                let [su, sv] = interpolate_uv(t, wl, wr, uvl, uvr, texture_mapping);
                let mut color = tex.sample(su, sv);
                if let Some(tint) = screen_tint {
                    color = tint.apply(color);
                }
                color = palette_mode.apply(color);
                coverage.mark_covered(x as usize, y as usize);
                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), color)])
                    .unwrap();
            }
        };

    let draw_flat_bottom =
        |p1: nalgebra::Point2<i32>,
         p2: nalgebra::Point2<i32>,
         p3: nalgebra::Point2<i32>,
         w1: f32,
         w2: f32,
         w3: f32,
         uv1: [f32; 2],
         uv2: [f32; 2],
         uv3: [f32; 2],
         fb: &mut D,
         coverage: &mut crate::bsp::coverage::CoverageBuffer<'_>| {
            let height = p2.y - p1.y;
            if height == 0 {
                return;
            }
            let invslope1 = ((p2.x - p1.x) << 16) / height;
            let invslope2 = ((p3.x - p1.x) << 16) / height;
            let mut cx1 = p1.x << 16;
            let mut cx2 = p1.x << 16;
            for sy in p1.y..=p2.y {
                let dy = sy - p1.y;
                let t = dy as f32 / height as f32;
                let wl = w1 + t * (w2 - w1);
                let wr = w1 + t * (w3 - w1);
                let uvl = [
                    uv1[0] + t * (uv2[0] - uv1[0]),
                    uv1[1] + t * (uv2[1] - uv1[1]),
                ];
                let uvr = [
                    uv1[0] + t * (uv3[0] - uv1[0]),
                    uv1[1] + t * (uv3[1] - uv1[1]),
                ];
                rasterize_span(cx1 >> 16, cx2 >> 16, sy, wl, wr, uvl, uvr, fb, coverage);
                cx1 += invslope1;
                cx2 += invslope2;
            }
        };

    let draw_flat_top =
        |p1: nalgebra::Point2<i32>,
         p2: nalgebra::Point2<i32>,
         p3: nalgebra::Point2<i32>,
         w1: f32,
         w2: f32,
         w3: f32,
         uv1: [f32; 2],
         uv2: [f32; 2],
         uv3: [f32; 2],
         fb: &mut D,
         coverage: &mut crate::bsp::coverage::CoverageBuffer<'_>| {
            let height = p3.y - p1.y;
            if height == 0 {
                return;
            }
            let invslope1 = ((p3.x - p1.x) << 16) / height;
            let invslope2 = ((p3.x - p2.x) << 16) / height;
            let mut cx1 = p3.x << 16;
            let mut cx2 = p3.x << 16;
            for sy in (p1.y..=p3.y).rev() {
                let dy = sy - p1.y;
                let t = dy as f32 / height as f32;
                let wl = w1 + t * (w3 - w1);
                let wr = w2 + t * (w3 - w2);
                let uvl = [
                    uv1[0] + t * (uv3[0] - uv1[0]),
                    uv1[1] + t * (uv3[1] - uv1[1]),
                ];
                let uvr = [
                    uv2[0] + t * (uv3[0] - uv2[0]),
                    uv2[1] + t * (uv3[1] - uv2[1]),
                ];
                rasterize_span(cx1 >> 16, cx2 >> 16, sy, wl, wr, uvl, uvr, fb, coverage);
                cx1 -= invslope1;
                cx2 -= invslope2;
            }
        };

    if p2.y == p3.y {
        draw_flat_bottom(p1, p2, p3, w1, w2, w3, uv1, uv2, uv3, fb, coverage);
    } else if p1.y == p2.y {
        draw_flat_top(p1, p2, p3, w1, w2, w3, uv1, uv2, uv3, fb, coverage);
    } else {
        let dy31 = (p3.y - p1.y) as f32;
        let dy21 = (p2.y - p1.y) as f32;
        let t = dy21 / dy31;
        let p4x = p1.x + ((p3.x - p1.x) as f32 * t) as i32;
        let p4 = nalgebra::Point2::new(p4x, p2.y);
        let w4 = w1 + (w3 - w1) * t;
        let uv4 = [
            uv1[0] + (uv3[0] - uv1[0]) * t,
            uv1[1] + (uv3[1] - uv1[1]) * t,
        ];
        draw_flat_bottom(p1, p2, p4, w1, w2, w4, uv1, uv2, uv4, fb, coverage);
        draw_flat_top(p2, p4, p3, w2, w4, w3, uv2, uv4, uv3, fb, coverage);
    }
}

#[inline(always)]
fn fill_triangle_zbuffered_textured<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let p4 = Point::new(
            (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32,
            p2_eg.y,
        );
        let z4_int = (z1_int as i64 + (t * (z3_int as i64 - z1_int as i64) as f32) as i64) as u32;
        let w4 = w1 + t * (w3 - w1);
        let uv4 = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];

        fill_bottom_flat_triangle_zbuffered_textured(
            p1_eg,
            p2_eg,
            p4,
            z1_int,
            z2_int,
            z4_int,
            w1,
            w2,
            w4,
            uv1,
            uv2,
            uv4,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
        fill_top_flat_triangle_zbuffered_textured(
            p2_eg,
            p4,
            p3_eg,
            z2_int,
            z4_int,
            z3_int,
            w2,
            w4,
            w3,
            uv2,
            uv4,
            uv3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    }
}

#[inline(always)]
fn fill_bottom_flat_triangle_zbuffered_textured<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        let w_left = w1 + t * (w2 - w1);
        let w_right = w1 + t * (w3 - w1);

        let uv_left = [
            uv1[0] + t * (uv2[0] - uv1[0]),
            uv1[1] + t * (uv2[1] - uv1[1]),
        ];
        let uv_right = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];

        draw_scanline_zbuffered_textured(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[inline(always)]
fn fill_top_flat_triangle_zbuffered_textured<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z2 as i64 + ((z3 as i64 - z2 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z2
        };

        let w_left = w1 + t * (w3 - w1);
        let w_right = w2 + t * (w3 - w2);

        let uv_left = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];
        let uv_right = [
            uv2[0] + t * (uv3[0] - uv2[0]),
            uv2[1] + t * (uv3[1] - uv2[1]),
        ];

        draw_scanline_zbuffered_textured(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[inline(always)]
pub(crate) fn draw_scanline_zbuffered_textured<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    w1: f32,
    w2: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    if y < 0 {
        return;
    }
    let height = zbuffer.len() / width;
    if y as usize >= height {
        return;
    }

    let (left_x, right_x, z_left, z_right, w_left, w_right, uv_left, uv_right) = if x1 <= x2 {
        (x1, x2, z1, z2, w1, w2, uv1, uv2)
    } else {
        (x2, x1, z2, z1, w2, w1, uv2, uv1)
    };

    let start_x = left_x.max(0);
    let end_x = right_x.min(width as i32 - 1);
    if start_x > end_x {
        return;
    }

    let span = right_x - left_x;
    let inv_span = if span > 0 { 1.0 / span as f32 } else { 0.0 };
    let z_step = if span > 0 {
        (((z_right as i64 - z_left as i64) << 16) / span as i64) as i32
    } else {
        0
    };

    let left_clip = start_x - left_x;
    let mut z_curr = ((z_left as i64) << 16) + (left_clip as i64 * z_step as i64);
    let mut zbuf_idx = y as usize * width + start_x as usize;

    const SUB_SPAN_SIZE: i32 = 16;
    let mut span_x = start_x;
    while span_x <= end_x {
        let next_span_x = (span_x + SUB_SPAN_SIZE).min(end_x + 1);
        let span_len = next_span_x - span_x;

        let t_start = (span_x - left_x) as f32 * inv_span;
        let t_end = (next_span_x - 1 - left_x) as f32 * inv_span;

        let [u_start, v_start] =
            interpolate_uv(t_start, w_left, w_right, uv_left, uv_right, texture_mapping);
        let [u_end, v_end] =
            interpolate_uv(t_end, w_left, w_right, uv_left, uv_right, texture_mapping);

        let inv_sub = if span_len > 1 {
            1.0 / (span_len - 1) as f32
        } else {
            0.0
        };
        let du = (u_end - u_start) * inv_sub;
        let dv = (v_end - v_start) * inv_sub;

        let mut curr_u = u_start;
        let mut curr_v = v_start;

        for x in span_x..next_span_x {
            if should_skip_stipple(x, y, stipple_mode) {
                z_curr += z_step as i64;
                zbuf_idx += 1;
                curr_u += du;
                curr_v += dv;
                continue;
            }

            let z = (z_curr >> 16) as u32;
            z_curr += z_step as i64;
            let z_depth = crate::to_zdepth(z);

            if z_depth < zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
                zbuffer[zbuf_idx] = z_depth;

                let mut final_color = texture.sample(curr_u, curr_v);

                if let Some(fog) = fog_config {
                    final_color = fog.apply(final_color, z);
                }

                if let Some(dither) = dither_config {
                    final_color = dither.apply(final_color, x, y);
                }
                if let Some(tint) = screen_tint {
                    final_color = tint.apply(final_color);
                }
                final_color = palette_mode.apply(final_color);

                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                    .unwrap();
            }
            zbuf_idx += 1;
            curr_u += du;
            curr_v += dv;
        }

        span_x = next_span_x;
    }
}

#[cfg(feature = "textured")]
pub fn fill_triangle_zbuffered_textured_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: nalgebra::Point2<i32>,
    p2: nalgebra::Point2<i32>,
    p3: nalgebra::Point2<i32>,
    z1: f32,
    z2: f32,
    z3: f32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let p1_eg = Point::new(p1.x, p1.y);
    let p2_eg = Point::new(p2.x, p2.y);
    let p3_eg = Point::new(p3.x, p3.y);

    let z1_int = (z1 * 65536.0) as u32;
    let z2_int = (z2 * 65536.0) as u32;
    let z3_int = (z3 * 65536.0) as u32;

    if p2_eg.y == p3_eg.y {
        fill_bottom_flat_triangle_zbuffered_textured_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            c1,
            c2,
            c3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    } else if p1_eg.y == p2_eg.y {
        fill_top_flat_triangle_zbuffered_textured_gouraud(
            p1_eg,
            p2_eg,
            p3_eg,
            z1_int,
            z2_int,
            z3_int,
            w1,
            w2,
            w3,
            uv1,
            uv2,
            uv3,
            c1,
            c2,
            c3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    } else {
        let t = (p2_eg.y - p1_eg.y) as f32 / (p3_eg.y - p1_eg.y) as f32;
        let split_x = (p1_eg.x as f32 + t * (p3_eg.x - p1_eg.x) as f32) as i32;
        let p_split = Point::new(split_x, p2_eg.y);

        let z_split = (z1_int as f64 + (z3_int as f64 - z1_int as f64) * t as f64) as u32;
        let w_split = w1 + t * (w3 - w1);
        let uv_split = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];
        let color_split = interpolate_color(c1, c3, t);

        fill_bottom_flat_triangle_zbuffered_textured_gouraud(
            p1_eg,
            p2_eg,
            p_split,
            z1_int,
            z2_int,
            z_split,
            w1,
            w2,
            w_split,
            uv1,
            uv2,
            uv_split,
            c1,
            c2,
            color_split,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );

        fill_top_flat_triangle_zbuffered_textured_gouraud(
            p2_eg,
            p_split,
            p3_eg,
            z2_int,
            z_split,
            z3_int,
            w2,
            w_split,
            w3,
            uv2,
            uv_split,
            uv3,
            c2,
            color_split,
            c3,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );
    }
}

#[cfg(feature = "textured")]
fn fill_bottom_flat_triangle_zbuffered_textured_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p2.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p2.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p1.x) << 16) / height;

    let mut curx1 = p1.x << 16;
    let mut curx2 = p1.x << 16;

    for scanline_y in p1.y..=p2.y {
        let dy = scanline_y - p1.y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z1 as i64 + ((z2 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };
        let z_right = if height > 0 {
            (z1 as i64 + ((z3 as i64 - z1 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z1
        };

        let w_left = w1 + t * (w2 - w1);
        let w_right = w1 + t * (w3 - w1);

        let uv_left = [
            uv1[0] + t * (uv2[0] - uv1[0]),
            uv1[1] + t * (uv2[1] - uv1[1]),
        ];
        let uv_right = [
            uv1[0] + t * (uv3[0] - uv1[0]),
            uv1[1] + t * (uv3[1] - uv1[1]),
        ];

        let color_left = interpolate_color(c1, c2, t);
        let color_right = interpolate_color(c1, c3, t);

        draw_scanline_zbuffered_textured_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            color_left,
            color_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );

        curx1 += invslope1;
        curx2 += invslope2;
    }
}

#[cfg(feature = "textured")]
fn fill_top_flat_triangle_zbuffered_textured_gouraud<D: DrawTarget<Color = Rgb565>>(
    p1: Point,
    p2: Point,
    p3: Point,
    z1: u32,
    z2: u32,
    z3: u32,
    w1: f32,
    w2: f32,
    w3: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
    c1: Rgb565,
    c2: Rgb565,
    c3: Rgb565,
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    let height = p3.y - p1.y;
    if height == 0 {
        return;
    }

    let invslope1 = ((p3.x - p1.x) << 16) / height;
    let invslope2 = ((p3.x - p2.x) << 16) / height;

    let mut curx1 = p3.x << 16;
    let mut curx2 = p3.x << 16;

    for scanline_y in (p1.y..=p3.y).rev() {
        let dy = p3.y - scanline_y;
        let t = dy as f32 / height as f32;

        let z_left = if height > 0 {
            (z3 as i64 + ((z1 as i64 - z3 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z3
        };
        let z_right = if height > 0 {
            (z3 as i64 + ((z2 as i64 - z3 as i64) * dy as i64 / height as i64)) as u32
        } else {
            z3
        };

        let w_left = w3 + t * (w1 - w3);
        let w_right = w3 + t * (w2 - w3);

        let uv_left = [
            uv3[0] + t * (uv1[0] - uv3[0]),
            uv3[1] + t * (uv1[1] - uv3[1]),
        ];
        let uv_right = [
            uv3[0] + t * (uv2[0] - uv3[0]),
            uv3[1] + t * (uv2[1] - uv3[1]),
        ];

        let color_left = interpolate_color(c3, c1, t);
        let color_right = interpolate_color(c3, c2, t);

        draw_scanline_zbuffered_textured_gouraud(
            curx1 >> 16,
            curx2 >> 16,
            scanline_y,
            z_left,
            z_right,
            w_left,
            w_right,
            uv_left,
            uv_right,
            color_left,
            color_right,
            texture,
            fb,
            zbuffer,
            width,
            fog_config,
            dither_config,
            texture_mapping,
            stipple_mode,
            screen_tint,
            palette_mode,
        );

        curx1 -= invslope1;
        curx2 -= invslope2;
    }
}

#[cfg(feature = "textured")]
fn draw_scanline_zbuffered_textured_gouraud<D: DrawTarget<Color = Rgb565>>(
    x1: i32,
    x2: i32,
    y: i32,
    z1: u32,
    z2: u32,
    w1: f32,
    w2: f32,
    uv1: [f32; 2],
    uv2: [f32; 2],
    color1: Rgb565,
    color2: Rgb565,
    texture: &crate::texture::Texture,
    fb: &mut D,
    zbuffer: &mut [crate::ZDepth],
    width: usize,
    fog_config: Option<&FogConfig>,
    dither_config: Option<&DitherConfig>,
    texture_mapping: TextureMapping,
    stipple_mode: StippleMode,
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) where
    <D as DrawTarget>::Error: Debug,
{
    if y < 0 {
        return;
    }
    let height = zbuffer.len() / width;
    if y as usize >= height {
        return;
    }

    let (
        left_x,
        right_x,
        z_left,
        z_right,
        w_left,
        w_right,
        uv_left,
        uv_right,
        color_left,
        color_right,
    ) = if x1 <= x2 {
        (x1, x2, z1, z2, w1, w2, uv1, uv2, color1, color2)
    } else {
        (x2, x1, z2, z1, w2, w1, uv2, uv1, color2, color1)
    };

    let start_x = left_x.max(0);
    let end_x = right_x.min(width as i32 - 1);
    if start_x > end_x {
        return;
    }

    let span = right_x - left_x;
    let inv_span = if span > 0 { 1.0 / span as f32 } else { 0.0 };
    let z_step = if span > 0 {
        (((z_right as i64 - z_left as i64) << 16) / span as i64) as i32
    } else {
        0
    };

    let left_clip = start_x - left_x;
    let mut z_curr = ((z_left as i64) << 16) + (left_clip as i64 * z_step as i64);
    let mut zbuf_idx = y as usize * width + start_x as usize;

    const SUB_SPAN_SIZE: i32 = 16;
    let mut span_x = start_x;

    while span_x <= end_x {
        let next_span_x = (span_x + SUB_SPAN_SIZE).min(end_x + 1);
        let span_len = next_span_x - span_x;

        let t_start = (span_x - left_x) as f32 * inv_span;
        let t_end = (next_span_x - 1 - left_x) as f32 * inv_span;

        let [u_start, v_start] =
            interpolate_uv(t_start, w_left, w_right, uv_left, uv_right, texture_mapping);
        let [u_end, v_end] =
            interpolate_uv(t_end, w_left, w_right, uv_left, uv_right, texture_mapping);

        let inv_sub = if span_len > 1 {
            1.0 / (span_len - 1) as f32
        } else {
            0.0
        };

        let du = (u_end - u_start) * inv_sub;
        let dv = (v_end - v_start) * inv_sub;

        let mut curr_u = u_start;
        let mut curr_v = v_start;

        for x in span_x..next_span_x {
            if should_skip_stipple(x, y, stipple_mode) {
                z_curr += z_step as i64;
                zbuf_idx += 1;
                curr_u += du;
                curr_v += dv;
                continue;
            }

            let z = (z_curr >> 16) as u32;
            z_curr += z_step as i64;
            let z_depth = crate::to_zdepth(z);

            if z_depth < zbuffer[zbuf_idx].saturating_add(crate::DEPTH_EPSILON) {
                zbuffer[zbuf_idx] = z_depth;

                let tx = (x - left_x) as f32 * inv_span;
                let c_interp = interpolate_color(color_left, color_right, tx);
                let tex_color = texture.sample(curr_u, curr_v);

                let r = ((tex_color.r() as u16 * c_interp.r() as u16) / 31) as u8;
                let g = ((tex_color.g() as u16 * c_interp.g() as u16) / 63) as u8;
                let b_val = ((tex_color.b() as u16 * c_interp.b() as u16) / 31) as u8;
                let mut final_color = Rgb565::new(r, g, b_val);

                if let Some(fog) = fog_config {
                    final_color = fog.apply(final_color, z);
                }
                if let Some(dither) = dither_config {
                    final_color = dither.apply(final_color, x, y);
                }
                if let Some(tint) = screen_tint {
                    final_color = tint.apply(final_color);
                }
                final_color = palette_mode.apply(final_color);

                fb.draw_iter([embedded_graphics_core::Pixel(Point::new(x, y), final_color)])
                    .unwrap();
            }
            zbuf_idx += 1;
            curr_u += du;
            curr_v += dv;
        }

        span_x = next_span_x;
    }
}
