use std::io::IsTerminal;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{OutputMode, PlainRenderer};
use crate::{render_help, HelpTopic, TaskInvocation};

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

    let color_enabled = if output_json {
        false
    } else {
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
    };
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    render_help(&mut renderer, HelpTopic::General)?;
    let rendered = String::from_utf8(renderer.into_inner())
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))?;
    if output_json {
        let payload = json!({
            "schema": "effigy.help.v1",
            "schema_version": 1,
            "ok": true,
            "topic": "general",
            "text": rendered,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }
    Ok(Some(rendered))
}
