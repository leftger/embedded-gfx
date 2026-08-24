use crate::command_buffer::{CommandBuffer, RenderCommand};
use crate::error::RenderError;
use crate::primitive::DrawPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileConfig {
    pub tile_width: usize,
    pub tile_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    pub cols: usize,
    pub rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileBinStats {
    pub draw_commands: usize,
    pub bins_used: usize,
}

#[inline(always)]
fn primitive_bounds(primitive: &DrawPrimitive) -> (i32, i32, i32, i32) {
    primitive.bounds()
}

pub fn tile_grid(width: usize, height: usize, config: TileConfig) -> Result<TileGrid, RenderError> {
    if config.tile_width == 0 || config.tile_height == 0 {
        return Err(RenderError::InvalidInput("tile dimensions must be >= 1"));
    }
    let cols = width.div_ceil(config.tile_width);
    let rows = height.div_ceil(config.tile_height);
    Ok(TileGrid { cols, rows })
}

pub fn build_bins<const MAX: usize, const BIN_CAP: usize>(
    commands: &CommandBuffer<MAX>,
    width: usize,
    height: usize,
    config: TileConfig,
) -> Result<
    (
        heapless::Vec<heapless::Vec<usize, BIN_CAP>, BIN_CAP>,
        TileBinStats,
    ),
    RenderError,
> {
    let grid = tile_grid(width, height, config)?;
    let bin_count = grid.cols * grid.rows;
    if bin_count > BIN_CAP {
        return Err(RenderError::InvalidInput("tile bin count exceeds BIN_CAP"));
    }

    let mut bins: heapless::Vec<heapless::Vec<usize, BIN_CAP>, BIN_CAP> = heapless::Vec::new();
    for _ in 0..bin_count {
        bins.push(heapless::Vec::new())
            .map_err(|_| RenderError::InvalidInput("unable to allocate tile bins"))?;
    }

    let mut draw_commands = 0usize;
    for (idx, command) in commands.iter().enumerate() {
        let RenderCommand::Draw(primitive) = command else {
            continue;
        };
        draw_commands += 1;
        let (min_x, min_y, max_x, max_y) = primitive_bounds(primitive);
        let clamp =
            |v: i32, max_v: usize| -> usize { v.clamp(0, max_v.saturating_sub(1) as i32) as usize };
        let x0 = clamp(min_x, width) / config.tile_width;
        let y0 = clamp(min_y, height) / config.tile_height;
        let x1 = clamp(max_x, width) / config.tile_width;
        let y1 = clamp(max_y, height) / config.tile_height;
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let bin_index = ty * grid.cols + tx;
                bins[bin_index].push(idx).map_err(|_| {
                    RenderError::OutOfBudget(crate::error::BudgetKind::DrawPrimitives {
                        attempted: idx + 1,
                        max: BIN_CAP,
                    })
                })?;
            }
        }
    }

    let bins_used = bins.iter().filter(|bin| !bin.is_empty()).count();
    Ok((
        bins,
        TileBinStats {
            draw_commands,
            bins_used,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_buffer::RenderCommand;
    use embedded_graphics_core::pixelcolor::Rgb565;
    use nalgebra::Point2;

    #[test]
    fn tile_grid_rejects_zero_tile_size() {
        let err = tile_grid(
            64,
            64,
            TileConfig {
                tile_width: 0,
                tile_height: 8,
            },
        )
        .expect_err("zero tile width must fail");
        assert!(matches!(err, RenderError::InvalidInput(_)));
    }

    #[test]
    fn build_bins_tracks_draw_count_and_bins_used() {
        let mut commands: CommandBuffer<8> = CommandBuffer::new();
        commands
            .push(RenderCommand::Draw(DrawPrimitive::ColoredTriangle(
                [
                    Point2::new(18, 18),
                    Point2::new(30, 18),
                    Point2::new(24, 30),
                ],
                Rgb565::new(31, 0, 0),
            )))
            .unwrap();

        let (bins, stats) = build_bins::<8, 64>(
            &commands,
            64,
            64,
            TileConfig {
                tile_width: 16,
                tile_height: 16,
            },
        )
        .expect("binning should succeed");

        assert_eq!(stats.draw_commands, 1);
        assert_eq!(stats.bins_used, 1);
        let populated = bins.iter().filter(|b| !b.is_empty()).count();
        assert_eq!(populated, 1);
    }

    #[test]
    fn build_bins_rejects_excessive_grid_for_capacity() {
        let commands: CommandBuffer<4> = CommandBuffer::new();
        let err = build_bins::<4, 8>(
            &commands,
            64,
            64,
            TileConfig {
                tile_width: 1,
                tile_height: 1,
            },
        )
        .expect_err("grid should exceed BIN_CAP");
        assert!(matches!(err, RenderError::InvalidInput(_)));
    }
}
