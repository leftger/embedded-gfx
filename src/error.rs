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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_kind_key_is_stable() {
        assert_eq!(
            BudgetKind::DrawPrimitives {
                attempted: 1,
                max: 2
            }
            .key(),
            "DrawPrimitives"
        );
        assert_eq!(
            BudgetKind::FramebufferDimensions {
                width: 1,
                height: 1,
                max_width: 2,
                max_height: 2
            }
            .key(),
            "FramebufferDimensions"
        );
        assert_eq!(
            BudgetKind::ZBufferLength {
                expected: 16,
                got: 8
            }
            .key(),
            "ZBufferLength"
        );
    }

    #[test]
    fn display_error_maps_to_backend_fault() {
        assert_eq!(
            RenderError::from(DisplayError::Busy),
            RenderError::BackendFault(BackendFaultKind::DmaBusyTimeout)
        );
        assert_eq!(
            RenderError::from(DisplayError::HardwareError),
            RenderError::BackendFault(BackendFaultKind::TransferStartFailed)
        );
        assert_eq!(
            RenderError::from(DisplayError::InvalidBuffer),
            RenderError::BackendFault(BackendFaultKind::InvalidBufferConfig)
        );
    }
}
