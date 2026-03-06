use serde_json::json;

use super::super::response::render_optional_text_with_schema_text_fields_lazy;
use crate::runner::error::RunnerError;

pub(super) fn render_watch_result_json(
    output_json: bool,
    runs: usize,
) -> Result<Option<String>, RunnerError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.watch.v1",
        || format!("watch complete after {runs} run(s)."),
        || {
            json!({
                "runs": runs,
            })
        },
    )
}
