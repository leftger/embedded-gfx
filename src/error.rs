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
    BackendFault(BackendFaultKind),
    Stall(StallKind),
    Recoverable {
        fault: RuntimeFaultKind,
        action: RecoveryAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendFaultKind {
    DmaBusyTimeout,
    TransferStartFailed,
    InvalidBufferConfig,
    DeviceUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallKind {
    RecordStage,
    ExecuteStage,
    PresentStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFaultKind {
    Backend(BackendFaultKind),
    Budget(BudgetKind),
    Stall(StallKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    Retry,
    RetryWithFallback,
    DropEffects,
    ReduceQuality,
    SkipFrame,
    ResetBackend,
}

impl From<DisplayError> for RenderError {
    fn from(value: DisplayError) -> Self {
        match value {
            DisplayError::Busy => Self::BackendFault(BackendFaultKind::DmaBusyTimeout),
            DisplayError::HardwareError => {
                Self::BackendFault(BackendFaultKind::TransferStartFailed)
            }
            DisplayError::InvalidBuffer => {
                Self::BackendFault(BackendFaultKind::InvalidBufferConfig)
            }
        }
    }
}
