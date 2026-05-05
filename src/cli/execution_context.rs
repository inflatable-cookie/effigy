use std::path::Path;

use effigy_ui::OutputMode;

pub struct CliExecutionContext<'a> {
    pub output_mode: OutputMode,
    pub runtime_context: &'a effigy_context::EffigyRuntimeContext,
    pub command_root: &'a Path,
    pub suppress_header: bool,
    pub emit_json_envelope: bool,
    pub command_kind: &'a str,
    pub command_name: &'a str,
}
