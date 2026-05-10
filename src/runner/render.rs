use serde_json::Value;

use super::RunnerError;

pub(in crate::runner) fn render_command_result(
    output_json: bool,
    ok: bool,
    json: Value,
    text: String,
) -> Result<String, RunnerError> {
    if output_json {
        let rendered = json.to_string();
        if ok {
            Ok(rendered)
        } else {
            Err(RunnerError::task_invocation(rendered))
        }
    } else if ok {
        Ok(text)
    } else {
        Err(RunnerError::task_invocation(text))
    }
}
