use serde_json::json;

use crate::{render_help, HelpTopic, TaskInvocation};

use super::super::render::{encode_pretty_json_optional, render_utf8, standard_renderer};
use super::super::RunnerError;

pub(super) fn run_builtin_help(
    task: &TaskInvocation,
    args: &[String],
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    for arg in args {
        if arg == "--json" {
            output_json = true;
            continue;
        }
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            args.join(" ")
        )));
    }

    let mut renderer = standard_renderer(output_json);
    render_help(&mut renderer, HelpTopic::General)?;
    let rendered = render_utf8(renderer.into_inner())?;
    if output_json {
        let payload = json!({
            "schema": "effigy.help.v1",
            "schema_version": 1,
            "ok": true,
            "topic": "general",
            "text": rendered,
        });
        return encode_pretty_json_optional(&payload);
    }
    Ok(Some(rendered))
}
