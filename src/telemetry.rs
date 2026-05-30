/// Re-export BSP telemetry from the bsp module for ergonomic imports.
pub use crate::bsp::BspTelemetry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordTelemetry {
    pub meshes_total: usize,
    pub meshes_visible: usize,
    pub unique_textures: usize,
    pub draw_commands: usize,
    pub fallback_used: bool,
    pub degradation_steps_applied: usize,
    pub dropped_meshes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecuteTelemetry {
    pub commands_total: usize,
    pub draw_commands: usize,
    pub clear_color_commands: usize,
    pub clear_depth_commands: usize,
}
