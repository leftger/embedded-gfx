#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordTelemetry {
    pub meshes_total: usize,
    pub meshes_visible: usize,
    pub unique_textures: usize,
    pub draw_commands: usize,
    pub fallback_used: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecuteTelemetry {
    pub commands_total: usize,
    pub draw_commands: usize,
    pub clear_color_commands: usize,
    pub clear_depth_commands: usize,
}
