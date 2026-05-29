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
    TrianglesPerMesh {
        attempted: usize,
        max: usize,
    },
    VerticesPerMesh {
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

impl BudgetKind {
    pub fn key(&self) -> &'static str {
        match self {
            BudgetKind::DrawPrimitives { .. } => "DrawPrimitives",
            BudgetKind::MeshesPerFrame { .. } => "MeshesPerFrame",
            BudgetKind::TrianglesPerMesh { .. } => "TrianglesPerMesh",
            BudgetKind::VerticesPerMesh { .. } => "VerticesPerMesh",
            BudgetKind::Textures { .. } => "Textures",
            BudgetKind::FramebufferDimensions { .. } => "FramebufferDimensions",
            BudgetKind::ZBufferLength { .. } => "ZBufferLength",
        }
    }
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
