use core::fmt::Debug;

use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    pixelcolor::{Rgb565, RgbColor},
    prelude::{OriginDimensions, Point},
};

use crate::{
    DrawPrimitive,
    command_buffer::{CommandBuffer, RenderCommand},
    draw::{DitherConfig, FogConfig, draw_zbuffered_with_effects},
    error::{BudgetKind, RenderError},
    retro::{PaletteMode, ScreenTint, SkyConfig, StippleMode},
};

pub struct FrameCtx<'a> {
    pub zbuffer: &'a mut [u32],
    pub width: usize,
    pub height: usize,
}

impl<'a> FrameCtx<'a> {
    pub fn validate(&self) -> Result<(), RenderError> {
        let expected = self.width * self.height;
        if self.zbuffer.len() != expected {
            return Err(RenderError::OutOfBudget(BudgetKind::ZBufferLength {
                expected,
                got: self.zbuffer.len(),
            }));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl DirtyRegion {
    fn from_bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Option<Self> {
        if max_x < min_x || max_y < min_y {
            return None;
        }
        Some(Self {
            x: min_x as usize,
            y: min_y as usize,
            width: (max_x - min_x + 1) as usize,
            height: (max_y - min_y + 1) as usize,
        })
    }
}

fn primitive_bounds(primitive: &DrawPrimitive) -> (i32, i32, i32, i32) {
    match primitive {
        DrawPrimitive::ColoredPoint(p, _) => (p.x, p.y, p.x, p.y),
        DrawPrimitive::Line([a, b], _) => (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)),
        DrawPrimitive::ColoredTriangle(points, _)
        | DrawPrimitive::ColoredTriangleWithDepth { points, .. }
        | DrawPrimitive::GouraudTriangle { points, .. }
        | DrawPrimitive::GouraudTriangleWithDepth { points, .. }
        | DrawPrimitive::TexturedTriangle { points, .. }
        | DrawPrimitive::TexturedTriangleWithDepth { points, .. }
        | DrawPrimitive::LightmappedTriangle { points, .. } => {
            let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
            let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
            let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
            let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
            (min_x, min_y, max_x, max_y)
        }
    }
}

fn clamp_bounds_to_frame(
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    width: usize,
    height: usize,
) -> Option<(i32, i32, i32, i32)> {
    let w = width as i32;
    let h = height as i32;
    let clamped_min_x = min_x.clamp(0, w.saturating_sub(1));
    let clamped_min_y = min_y.clamp(0, h.saturating_sub(1));
    let clamped_max_x = max_x.clamp(0, w.saturating_sub(1));
    let clamped_max_y = max_y.clamp(0, h.saturating_sub(1));
    if clamped_max_x < clamped_min_x || clamped_max_y < clamped_min_y {
        return None;
    }
    Some((clamped_min_x, clamped_min_y, clamped_max_x, clamped_max_y))
}

#[inline]
fn apply_post(color: Rgb565, tint: Option<ScreenTint>, palette_mode: PaletteMode) -> Rgb565 {
    let tinted = if let Some(t) = tint {
        t.apply(color)
    } else {
        color
    };
    palette_mode.apply(tinted)
}

fn tint_primitive(
    primitive: &DrawPrimitive,
    tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) -> DrawPrimitive {
    match primitive.clone() {
        DrawPrimitive::ColoredPoint(p, color) => {
            DrawPrimitive::ColoredPoint(p, apply_post(color, tint, palette_mode))
        }
        DrawPrimitive::Line(points, color) => {
            DrawPrimitive::Line(points, apply_post(color, tint, palette_mode))
        }
        DrawPrimitive::ColoredTriangle(points, color) => {
            DrawPrimitive::ColoredTriangle(points, apply_post(color, tint, palette_mode))
        }
        DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color,
        } => DrawPrimitive::ColoredTriangleWithDepth {
            points,
            depths,
            color: apply_post(color, tint, palette_mode),
        },
        DrawPrimitive::GouraudTriangle { points, colors } => DrawPrimitive::GouraudTriangle {
            points,
            colors: [
                apply_post(colors[0], tint, palette_mode),
                apply_post(colors[1], tint, palette_mode),
                apply_post(colors[2], tint, palette_mode),
            ],
        },
        DrawPrimitive::GouraudTriangleWithDepth {
            points,
            depths,
            colors,
        } => DrawPrimitive::GouraudTriangleWithDepth {
            points,
            depths,
            colors: [
                apply_post(colors[0], tint, palette_mode),
                apply_post(colors[1], tint, palette_mode),
                apply_post(colors[2], tint, palette_mode),
            ],
        },
        DrawPrimitive::LightmappedTriangle {
            points,
            depths,
            ws,
            surface_uvs,
            lm_uvs,
            texture_id,
            lightmap_id,
            brightness,
            dynamic_tint,
        } => DrawPrimitive::LightmappedTriangle {
            points,
            depths,
            ws,
            surface_uvs,
            lm_uvs,
            texture_id,
            lightmap_id,
            brightness,
            dynamic_tint: apply_post(dynamic_tint, tint, palette_mode),
        },
        other => other,
    }
}

#[inline]
fn blend_rgb565(a: Rgb565, b: Rgb565, t_q8: u16) -> Rgb565 {
    let inv = 255u16.saturating_sub(t_q8);
    let r = ((a.r() as u16 * inv + b.r() as u16 * t_q8) / 255) as u8;
    let g = ((a.g() as u16 * inv + b.g() as u16 * t_q8) / 255) as u8;
    let bch = ((a.b() as u16 * inv + b.b() as u16 * t_q8) / 255) as u8;
    Rgb565::new(r, g, bch)
}

#[inline]
fn stripe_on_at(x: i32, scroll: i32, stripe_w: i32) -> bool {
    (((x + scroll).div_euclid(stripe_w)) & 1) == 0
}

fn draw_sky_background<D>(
    fb: &mut D,
    width: usize,
    height: usize,
    sky: SkyConfig,
    camera_dir: [f32; 3],
    screen_tint: Option<ScreenTint>,
    palette_mode: PaletteMode,
) -> Result<(), RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    let w = width as i32;
    let h = height as i32;
    if w <= 0 || h <= 0 {
        return Ok(());
    }

    let horizon = (h as f32 * (0.5 + camera_dir[1].clamp(-1.0, 1.0) * 0.25)) as i32;
    let stripe_w = sky.stripe_width.max(1) as i32;
    let scroll = (camera_dir[0] * 128.0) as i32;
    let stripe_fade_span = (h / 6).max(1);

    for y in 0..h {
        let dy = (y - horizon + h / 2).clamp(0, h);
        let t_q8 = ((dy as i64 * 255) / h.max(1) as i64) as u16;
        let base = blend_rgb565(sky.top_color, sky.bottom_color, t_q8);
        let below_horizon = (y - horizon).max(0);
        let stripe_strength = if below_horizon == 0 {
            sky.stripe_strength as u16
        } else if below_horizon >= stripe_fade_span {
            0
        } else {
            let rem = stripe_fade_span - below_horizon;
            ((sky.stripe_strength as i32 * rem) / stripe_fade_span) as u16
        };
        for x in 0..w {
            let stripe_on = stripe_on_at(x, scroll, stripe_w);
            let mut color = if stripe_on && stripe_strength > 0 {
                blend_rgb565(base, sky.stripe_color, stripe_strength)
            } else {
                base
            };
            color = apply_post(color, screen_tint, palette_mode);
            fb.draw_iter([Pixel(Point::new(x, y), color)])
                .map_err(|_| RenderError::InvalidInput("draw target rejected sky write"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::stripe_on_at;

    #[test]
    fn stripe_phase_is_periodic_with_negative_scroll() {
        let stripe_w = 10;
        let scroll = -3;
        for x in -40..40 {
            assert_eq!(
                stripe_on_at(x, scroll, stripe_w),
                stripe_on_at(x + stripe_w * 2, scroll, stripe_w)
            );
        }
    }

    #[test]
    fn stripe_runs_do_not_exceed_width() {
        let stripe_w = 8;
        let scroll = -5;
        let mut max_run = 0usize;
        let mut run = 0usize;
        let mut prev = stripe_on_at(-64, scroll, stripe_w);
        for x in -63..=64 {
            let cur = stripe_on_at(x, scroll, stripe_w);
            if cur == prev {
                run += 1;
            } else {
                max_run = max_run.max(run);
                run = 1;
                prev = cur;
            }
        }
        max_run = max_run.max(run);
        assert!(max_run <= stripe_w as usize);
    }
}

pub fn execute_commands<D, const MAX: usize>(
    fb: &mut D,
    frame: &mut FrameCtx<'_>,
    cmd: &CommandBuffer<MAX>,
    fog: Option<&FogConfig>,
) -> Result<(), RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    let _ = execute_commands_with_dirty_region_effects(
        fb,
        frame,
        cmd,
        fog,
        None,
        None,
        StippleMode::Off,
        PaletteMode::Off,
        None,
        [0.0, 0.0, -1.0],
    )?;
    Ok(())
}

pub fn execute_commands_with_dirty_region<D, const MAX: usize>(
    fb: &mut D,
    frame: &mut FrameCtx<'_>,
    cmd: &CommandBuffer<MAX>,
    fog: Option<&FogConfig>,
) -> Result<Option<DirtyRegion>, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    execute_commands_with_dirty_region_effects(
        fb,
        frame,
        cmd,
        fog,
        None,
        None,
        StippleMode::Off,
        PaletteMode::Off,
        None,
        [0.0, 0.0, -1.0],
    )
}

pub fn execute_commands_with_dirty_region_effects<D, const MAX: usize>(
    fb: &mut D,
    frame: &mut FrameCtx<'_>,
    cmd: &CommandBuffer<MAX>,
    fog: Option<&FogConfig>,
    dither: Option<&DitherConfig>,
    screen_tint: Option<ScreenTint>,
    _stipple_mode: StippleMode,
    palette_mode: PaletteMode,
    sky: Option<SkyConfig>,
    camera_dir: [f32; 3],
) -> Result<Option<DirtyRegion>, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    frame.validate()?;
    let mut dirty_bounds: Option<(i32, i32, i32, i32)> = None;
    if let Some(sky_cfg) = sky {
        draw_sky_background(
            fb,
            frame.width,
            frame.height,
            sky_cfg,
            camera_dir,
            screen_tint,
            palette_mode,
        )?;
        dirty_bounds = Some((
            0,
            0,
            frame.width.saturating_sub(1) as i32,
            frame.height.saturating_sub(1) as i32,
        ));
    }

    for c in cmd.iter() {
        match c {
            RenderCommand::ClearColor(color) => {
                let w = frame.width as i32;
                let h = frame.height as i32;
                let clear_color = apply_post(*color, screen_tint, palette_mode);
                for y in 0..h {
                    for x in 0..w {
                        fb.draw_iter([Pixel(Point::new(x, y), clear_color)])
                            .map_err(|_| {
                                RenderError::InvalidInput("draw target rejected clear write")
                            })?;
                    }
                }
            }
            RenderCommand::ClearDepth(value) => {
                frame.zbuffer.fill(*value);
            }
            RenderCommand::Draw(primitive) => {
                let prim = tint_primitive(primitive, screen_tint, palette_mode);
                draw_zbuffered_with_effects(prim, fb, frame.zbuffer, frame.width, fog, dither);
                let (min_x, min_y, max_x, max_y) = primitive_bounds(primitive);
                if let Some((min_x, min_y, max_x, max_y)) =
                    clamp_bounds_to_frame(min_x, min_y, max_x, max_y, frame.width, frame.height)
                {
                    dirty_bounds = Some(match dirty_bounds {
                        Some((cx0, cy0, cx1, cy1)) => (
                            cx0.min(min_x),
                            cy0.min(min_y),
                            cx1.max(max_x),
                            cy1.max(max_y),
                        ),
                        None => (min_x, min_y, max_x, max_y),
                    });
                }
            }
        }
    }

    let region = dirty_bounds.and_then(|(x0, y0, x1, y1)| DirtyRegion::from_bounds(x0, y0, x1, y1));
    Ok(region)
}

pub fn execute_commands_tiled<D, const MAX: usize, const BIN_CAP: usize>(
    fb: &mut D,
    frame: &mut FrameCtx<'_>,
    cmd: &CommandBuffer<MAX>,
    tile: crate::tilebin::TileConfig,
    fog: Option<&FogConfig>,
) -> Result<crate::tilebin::TileBinStats, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    execute_commands_tiled_effects::<D, MAX, BIN_CAP>(
        fb,
        frame,
        cmd,
        tile,
        fog,
        None,
        None,
        StippleMode::Off,
        PaletteMode::Off,
        None,
        [0.0, 0.0, -1.0],
    )
}

pub fn execute_commands_tiled_effects<D, const MAX: usize, const BIN_CAP: usize>(
    fb: &mut D,
    frame: &mut FrameCtx<'_>,
    cmd: &CommandBuffer<MAX>,
    tile: crate::tilebin::TileConfig,
    fog: Option<&FogConfig>,
    dither: Option<&DitherConfig>,
    screen_tint: Option<ScreenTint>,
    _stipple_mode: StippleMode,
    palette_mode: PaletteMode,
    sky: Option<SkyConfig>,
    camera_dir: [f32; 3],
) -> Result<crate::tilebin::TileBinStats, RenderError>
where
    D: DrawTarget<Color = Rgb565> + OriginDimensions,
    D::Error: Debug,
{
    frame.validate()?;
    if let Some(sky_cfg) = sky {
        draw_sky_background(
            fb,
            frame.width,
            frame.height,
            sky_cfg,
            camera_dir,
            screen_tint,
            palette_mode,
        )?;
    }
    let (bins, stats) =
        crate::tilebin::build_bins::<MAX, BIN_CAP>(cmd, frame.width, frame.height, tile)?;
    let mut executed_draw = [false; MAX];

    for command in cmd.iter() {
        match command {
            RenderCommand::ClearColor(color) => {
                let w = frame.width as i32;
                let h = frame.height as i32;
                let clear_color = apply_post(*color, screen_tint, palette_mode);
                for y in 0..h {
                    for x in 0..w {
                        fb.draw_iter([Pixel(Point::new(x, y), clear_color)])
                            .map_err(|_| {
                                RenderError::InvalidInput("draw target rejected clear write")
                            })?;
                    }
                }
            }
            RenderCommand::ClearDepth(value) => frame.zbuffer.fill(*value),
            RenderCommand::Draw(_) => {}
        }
    }

    for bin in bins.iter() {
        for idx in bin.iter().copied() {
            if idx >= MAX || executed_draw[idx] {
                continue;
            }
            let Some(RenderCommand::Draw(primitive)) = cmd.get(idx) else {
                continue;
            };
            let prim = tint_primitive(primitive, screen_tint, palette_mode);
            draw_zbuffered_with_effects(prim, fb, frame.zbuffer, frame.width, fog, dither);
            executed_draw[idx] = true;
        }
    }

    Ok(stats)
}
