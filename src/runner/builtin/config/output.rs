use serde_json::json;

use super::super::render_builtin_help_text;
use super::super::response::{
    builtin_output_color_enabled, render_optional_text_with_schema_text_fields_lazy,
};
use super::reference::{render_config_reference, style_schema_comments};
use super::request::{ConfigRequest, ConfigSchemaTarget, ConfigTestRunner};
use super::schema::{
    render_builtin_config_schema, render_builtin_config_schema_minimal,
    render_builtin_config_schema_target, render_builtin_config_schema_test_target,
};
use crate::runner::error::RunnerError;

pub(super) fn render_config_request(request: ConfigRequest) -> Result<Option<String>, RunnerError> {
    if request.schema {
        return render_schema_payload(request);
    }
    render_reference_payload(request.output_json)
}

pub(super) fn render_config_help_payload(output_json: bool) -> Result<String, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_builtin_help_text("config", rendered, output_json)
}

fn render_schema_payload(request: ConfigRequest) -> Result<Option<String>, RunnerError> {
    let color_enabled = builtin_output_color_enabled(request.output_json);
    let target = request.target;
    let runner = request.runner;

    let rendered = match target {
        Some(ConfigSchemaTarget::Test) => {
            render_builtin_config_schema_test_target(request.minimal, runner)
        }
        Some(target) => render_builtin_config_schema_target(target, request.minimal),
        None if request.minimal => render_builtin_config_schema_minimal(),
        None => render_builtin_config_schema(),
    };

    let text = style_schema_comments(rendered, color_enabled);
    render_config_payload(
        request.output_json,
        ConfigPayload::schema(request.minimal, target, runner, text),
    )
}

fn render_reference_payload(output_json: bool) -> Result<Option<String>, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_config_payload(output_json, ConfigPayload::reference(rendered))
}

fn render_config_payload(
    output_json: bool,
    payload: ConfigPayload,
) -> Result<Option<String>, RunnerError> {
    let mode = payload.mode;
    let minimal = payload.minimal;
    let target = payload.target.map(ConfigSchemaTarget::as_str);
    let runner = payload.runner.map(ConfigTestRunner::as_str);
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.config.v1",
        move || payload.text,
        move || {
            json!({
                "mode": mode,
                "minimal": minimal,
                "target": target,
                "runner": runner,
            })
        },
    )
}

struct ConfigPayload {
    mode: &'static str,
    minimal: bool,
    target: Option<ConfigSchemaTarget>,
    runner: Option<ConfigTestRunner>,
    text: String,
}

impl ConfigPayload {
    fn reference(text: String) -> Self {
        Self {
            mode: "reference",
            minimal: false,
            target: None,
            runner: None,
            text,
        }
    }

    fn schema(
        minimal: bool,
        target: Option<ConfigSchemaTarget>,
        runner: Option<ConfigTestRunner>,
        text: String,
    ) -> Self {
        Self {
            mode: "schema",
            minimal,
            target,
            runner,
            text,
        }
    }
}
