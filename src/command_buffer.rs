use heapless::Vec;

use crate::{
    DrawPrimitive,
    error::{BudgetKind, RenderError},
};

#[derive(Debug, Clone)]
pub enum RenderCommand {
    ClearColor(embedded_graphics_core::pixelcolor::Rgb565),
    ClearDepth(u32),
    Draw(DrawPrimitive),
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
}

impl<const MAX: usize> Default for CommandBuffer<MAX> {
    fn default() -> Self {
        Self::new()
    }
}
