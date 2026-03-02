use crate::ui::{KeyValue, Renderer};
use crate::TaskInvocation;

use super::super::render::trace_renderer;
use super::super::{DeferredCommand, RunnerError};

pub(super) fn render_deferral_trace(
    task: &TaskInvocation,
    deferral: &DeferredCommand,
    command: &str,
    cause: &RunnerError,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Deferral");
    let _ = renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("defer-source", deferral.source.clone()),
        KeyValue::new("working-dir", deferral.working_dir.display().to_string()),
        KeyValue::new("command", command.to_owned()),
        KeyValue::new("reason", cause.to_string()),
    ]);
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}
