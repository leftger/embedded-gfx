use crate::display_backend::DisplayError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    DrawPrimitives {
        attempted: usize,
        max: usize,
    },
    MeshesPerFrame {
        attempted: usize,
        max: usize,
    },
    Textures {
        attempted: usize,
        max: usize,
    },
    FramebufferDimensions {
        width: usize,
        height: usize,
        max_width: usize,
        max_height: usize,
    },
    ZBufferLength {
        expected: usize,
        got: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    OutOfBudget(BudgetKind),
    InvalidInput(&'static str),
    Backend(DisplayError),
}

impl From<DisplayError> for RenderError {
    fn from(value: DisplayError) -> Self {
        Self::Backend(value)
    }
}
