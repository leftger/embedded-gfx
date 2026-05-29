use embedded_3dgfx::telemetry::{ExecuteTelemetry, RecordTelemetry};

pub fn telemetry_summary(record: &RecordTelemetry, execute: &ExecuteTelemetry) -> String {
    format!(
        "Record: meshes {}/{} tex {} draws {} fallback {}\nExecute: cmds {} draws {} clears c{} d{}",
        record.meshes_visible,
        record.meshes_total,
        record.unique_textures,
        record.draw_commands,
        if record.fallback_used { "on" } else { "off" },
        execute.commands_total,
        execute.draw_commands,
        execute.clear_color_commands,
        execute.clear_depth_commands
    )
}
