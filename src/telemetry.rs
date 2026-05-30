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

#[cfg(test)]
mod tests {
    use super::{ExecuteTelemetry, RecordTelemetry};

    #[test]
    fn record_telemetry_defaults_to_zero_and_false() {
        let t = RecordTelemetry::default();
        assert_eq!(t.meshes_total, 0);
        assert_eq!(t.meshes_visible, 0);
        assert_eq!(t.unique_textures, 0);
        assert_eq!(t.draw_commands, 0);
        assert!(!t.fallback_used);
        assert_eq!(t.degradation_steps_applied, 0);
        assert_eq!(t.dropped_meshes, 0);
    }

    #[test]
    fn execute_telemetry_defaults_to_zero() {
        let t = ExecuteTelemetry::default();
        assert_eq!(t.commands_total, 0);
        assert_eq!(t.draw_commands, 0);
        assert_eq!(t.clear_color_commands, 0);
        assert_eq!(t.clear_depth_commands, 0);
    }
}
