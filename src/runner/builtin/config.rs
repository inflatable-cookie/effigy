use serde_json::json;

use crate::TaskInvocation;

use super::super::RunnerError;
use super::command_spec::run_builtin_command;
use super::render_builtin_help_text;
use super::response::{builtin_output_color_enabled, render_optional_text_or_schema_json_lazy};

mod docs;
mod reference;
#[path = "config/request.rs"]
mod request;
mod schema;

use reference::{render_config_reference, style_schema_comments};
use request::parse_config_request;
use schema::{
    render_builtin_config_schema, render_builtin_config_schema_minimal,
    render_builtin_config_schema_target, render_builtin_config_schema_test_target,
};

pub(super) fn run_builtin_config(
    task: &TaskInvocation,
    args: &[String],
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        render_config_help_payload,
        || parse_config_request(task, args),
        run_config_request,
    )
}

fn run_config_request(request: request::ConfigRequest) -> Result<Option<String>, RunnerError> {
    if request.schema {
        let color_enabled = builtin_output_color_enabled(request.output_json);

        if let Some(section) = request.target {
            let selected = if section == request::ConfigSchemaTarget::Test {
                render_builtin_config_schema_test_target(request.minimal, request.runner)
            } else {
                render_builtin_config_schema_target(section, request.minimal)
            };
            let text = style_schema_comments(selected, color_enabled);
            return render_config_payload(
                request.output_json,
                "schema",
                request.minimal,
                Some(section.as_str()),
                request.runner.map(request::ConfigTestRunner::as_str),
                text,
            );
        }

        if request.runner.is_some() {
            return Err(RunnerError::task_invocation(
                "`--runner` requires `--target test` for built-in `config`",
            ));
        }

        let rendered = if request.minimal {
            render_builtin_config_schema_minimal()
        } else {
            render_builtin_config_schema()
        };
        let text = style_schema_comments(rendered, color_enabled);
        return render_config_payload(
            request.output_json,
            "schema",
            request.minimal,
            None,
            None,
            text,
        );
    }

    render_config_reference_payload(request.output_json)
}

fn render_config_reference_payload(output_json: bool) -> Result<Option<String>, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_config_payload(output_json, "reference", false, None, None, rendered)
}

fn render_config_help_payload(output_json: bool) -> Result<String, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_builtin_help_text("config", rendered, output_json)
}

fn render_config_payload(
    output_json: bool,
    mode: &'static str,
    minimal: bool,
    target: Option<&'static str>,
    runner: Option<&'static str>,
    text: String,
) -> Result<Option<String>, RunnerError> {
    let json_text = text.clone();
    let target_json = target.map_or(serde_json::Value::Null, |value| json!(value));
    let runner_json = runner.map_or(serde_json::Value::Null, |value| json!(value));
    render_optional_text_or_schema_json_lazy(
        output_json,
        "effigy.config.v1",
        move || text,
        move || {
            json!({
                "mode": mode,
                "minimal": minimal,
                "target": target_json,
                "runner": runner_json,
                "text": json_text,
            })
        },
    )
}

#[cfg(test)]
pub(in crate::runner) use request::ConfigParseContract;

#[cfg(test)]
pub(in crate::runner) use request::parse_config_contract_request;
