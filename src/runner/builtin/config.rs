use std::io::IsTerminal;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::OutputMode;
use crate::TaskInvocation;

use super::super::RunnerError;

mod options;
mod reference;
mod schema;

use options::parse_config_options;
use reference::{render_config_reference, style_schema_comments};
use schema::{
    normalize_test_runner_name, render_builtin_config_schema, render_builtin_config_schema_minimal,
    render_builtin_config_schema_target, render_builtin_config_schema_test_target,
};

pub(super) fn run_builtin_config(
    task: &TaskInvocation,
    args: &[String],
) -> Result<Option<String>, RunnerError> {
    let options = parse_config_options(task, args)?;

    if options.schema {
        let color_enabled = if options.output_json {
            false
        } else {
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
        };

        if let Some(section) = options.target.as_deref() {
            let selected = if section == "test" {
                let normalized_runner = match options.runner.as_deref() {
                    Some(value) => Some(normalize_test_runner_name(value).ok_or_else(|| {
                        RunnerError::TaskInvocation(format!(
                            "invalid `--runner` value `{value}` for built-in `config` (supported: vitest, cargo-nextest, cargo-test)"
                        ))
                    })?),
                    None => None,
                };
                render_builtin_config_schema_test_target(options.minimal, normalized_runner)
            } else {
                render_builtin_config_schema_target(section, options.minimal).ok_or_else(|| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--target` value `{section}` for built-in `config` (supported: package_manager, test, tasks, defer, shell)"
                    ))
                })?
            };
            let text = style_schema_comments(selected, color_enabled);
            if options.output_json {
                let payload = json!({
                    "schema": "effigy.config.v1",
                    "schema_version": 1,
                    "ok": true,
                    "mode": "schema",
                    "minimal": options.minimal,
                    "target": section,
                    "runner": options.runner,
                    "text": text,
                });
                return serde_json::to_string_pretty(&payload)
                    .map(Some)
                    .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
            }
            return Ok(Some(text));
        }

        if options.runner.is_some() {
            return Err(RunnerError::TaskInvocation(
                "`--runner` requires `--target test` for built-in `config`".to_owned(),
            ));
        }

        let rendered = if options.minimal {
            render_builtin_config_schema_minimal()
        } else {
            render_builtin_config_schema()
        };
        let text = style_schema_comments(rendered, color_enabled);
        if options.output_json {
            let payload = json!({
                "schema": "effigy.config.v1",
                "schema_version": 1,
                "ok": true,
                "mode": "schema",
                "minimal": options.minimal,
                "target": serde_json::Value::Null,
                "runner": serde_json::Value::Null,
                "text": text,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(text));
    }

    let color_enabled = if options.output_json {
        false
    } else {
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
    };
    let rendered = render_config_reference(color_enabled)?;
    if options.output_json {
        let payload = json!({
            "schema": "effigy.config.v1",
            "schema_version": 1,
            "ok": true,
            "mode": "reference",
            "minimal": false,
            "target": serde_json::Value::Null,
            "runner": serde_json::Value::Null,
            "text": rendered,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    Ok(Some(rendered))
}
