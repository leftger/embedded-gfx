use core::fmt::Debug;

use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::{OriginDimensions, Point},
};

use crate::{
    DrawPrimitive,
    command_buffer::{CommandBuffer, RenderCommand},
    draw::{FogConfig, draw_zbuffered_with_effects},
    error::{BudgetKind, RenderError},
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
    let _ = execute_commands_with_dirty_region(fb, frame, cmd, fog)?;
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
    frame.validate()?;
    let mut dirty_bounds: Option<(i32, i32, i32, i32)> = None;

    for c in cmd.iter() {
        match c {
            RenderCommand::ClearColor(color) => {
                let w = frame.width as i32;
                let h = frame.height as i32;
                for y in 0..h {
                    for x in 0..w {
                        fb.draw_iter([Pixel(Point::new(x, y), *color)])
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
                draw_zbuffered_with_effects(primitive.clone(), fb, frame.zbuffer, frame.width, fog, None);
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
    frame.validate()?;
    let (bins, stats) =
        crate::tilebin::build_bins::<MAX, BIN_CAP>(cmd, frame.width, frame.height, tile)?;
    let mut executed_draw = [false; MAX];

    for command in cmd.iter() {
        match command {
            RenderCommand::ClearColor(color) => {
                let w = frame.width as i32;
                let h = frame.height as i32;
                for y in 0..h {
                    for x in 0..w {
                        fb.draw_iter([Pixel(Point::new(x, y), *color)])
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
            draw_zbuffered_with_effects(primitive.clone(), fb, frame.zbuffer, frame.width, fog, None);
            executed_draw[idx] = true;
        }
    }

    Ok(stats)
}
