use heapless::Vec;

use crate::{
    DrawPrimitive,
    error::{BudgetKind, RenderError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveHeader {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl PrimitiveHeader {
    pub fn new(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_primitive(primitive: &DrawPrimitive) -> Self {
        let (min_x, min_y, max_x, max_y) = primitive.bounds();
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        (self.min_x, self.min_y, self.max_x, self.max_y)
    }
}

#[derive(Debug, Clone)]
pub enum RenderCommand {
    ClearColor(embedded_graphics_core::pixelcolor::Rgb565),
    ClearDepth(crate::ZDepth),
    Draw(DrawPrimitive),
}

impl RenderCommand {
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        match self {
            RenderCommand::Draw(primitive) => Some(primitive.bounds()),
            _ => None,
        }
    }
}

pub struct CommandBuffer<const MAX: usize> {
    commands: Vec<RenderCommand, MAX>,
}

impl<const MAX: usize> CommandBuffer<MAX> {
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn push(&mut self, cmd: RenderCommand) -> Result<(), RenderError> {
        self.commands.push(cmd).map_err(|_| {
            RenderError::OutOfBudget(BudgetKind::DrawPrimitives {
                attempted: self.commands.len() + 1,
                max: MAX,
            })
        })
    }

    pub fn iter(&self) -> core::slice::Iter<'_, RenderCommand> {
        self.commands.iter()
    }

    pub fn get(&self, index: usize) -> Option<&RenderCommand> {
        self.commands.get(index)
    }
}

impl<const MAX: usize> Default for CommandBuffer<MAX> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics_core::pixelcolor::Rgb565;
    use nalgebra::Point2;

    #[test]
    fn new_buffer_starts_empty() {
        let buf: CommandBuffer<4> = CommandBuffer::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn push_and_get_roundtrip() {
        let mut buf: CommandBuffer<4> = CommandBuffer::new();
        buf.push(RenderCommand::Draw(DrawPrimitive::ColoredPoint(
            Point2::new(3, 7),
            Rgb565::new(31, 0, 0),
        )))
        .unwrap();
        assert_eq!(buf.len(), 1);
        assert!(matches!(
            buf.get(0),
            Some(RenderCommand::Draw(DrawPrimitive::ColoredPoint(_, _)))
        ));
    }

    #[test]
    fn push_over_capacity_returns_budget_error() {
        let mut buf: CommandBuffer<1> = CommandBuffer::new();
        buf.push(RenderCommand::ClearDepth(0)).unwrap();
        let err = buf
            .push(RenderCommand::ClearDepth(1))
            .expect_err("overflow must fail");
        assert_eq!(
            err,
            RenderError::OutOfBudget(BudgetKind::DrawPrimitives {
                attempted: 2,
                max: 1
            })
        );
    }
}
