use serde_json::json;

use crate::{render_help, HelpTopic};

use super::super::super::render::{encode_pretty_json_optional, render_utf8, standard_renderer};
use super::super::super::RunnerError;

pub(super) fn render_watch_help_payload(output_json: bool) -> Result<Option<String>, RunnerError> {
    let mut renderer = standard_renderer(output_json);
    render_help(&mut renderer, HelpTopic::Watch)?;
    let rendered = render_utf8(renderer.into_inner())?;
    if output_json {
        let payload = json!({
            "schema": "effigy.help.v1",
            "schema_version": 1,
            "ok": true,
            "topic": "watch",
            "text": rendered,
        });
        return encode_pretty_json_optional(&payload);
    }
    Ok(Some(rendered))
}

pub(super) fn render_watch_result_json(
    output_json: bool,
    runs: usize,
) -> Result<Option<String>, RunnerError> {
    if !output_json {
        return Ok(Some(format!("watch complete after {runs} run(s).")));
    }
    let payload = json!({
        "schema": "effigy.watch.v1",
        "schema_version": 1,
        "ok": true,
        "runs": runs,
    });
    encode_pretty_json_optional(&payload)
}
