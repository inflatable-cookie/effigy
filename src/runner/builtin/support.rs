use serde_json::json;
use std::path::Path;

use crate::{render_help, HelpTopic};

use super::super::model::catalog::TaskRuntimeArgs;
use super::super::render::{encode_json, render_utf8, standard_renderer};
use super::response::schema_payload;
use crate::runner::error::RunnerError;

pub(in crate::runner) fn reject_verbose_root_for_builtin(
    task_name: &str,
    runtime_args: &TaskRuntimeArgs,
) -> Result<(), RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::task_invocation(format!(
            "`--verbose-root` is not supported for built-in `{task_name}`",
        )));
    }
    if runtime_args.env_schema_override.is_some() {
        return Err(RunnerError::task_invocation(format!(
            "`--env-schema` is not supported for built-in `{task_name}`",
        )));
    }
    Ok(())
}

fn unknown_builtin_args(task_name: &str, args: &[String]) -> RunnerError {
    RunnerError::task_invocation(format!(
        "unknown argument(s) for built-in `{task_name}`: {}",
        args.join(" ")
    ))
}

pub(in crate::runner) fn ensure_no_unknown_builtin_args(
    task_name: &str,
    unknown: &[String],
) -> Result<(), RunnerError> {
    if unknown.is_empty() {
        return Ok(());
    }
    Err(unknown_builtin_args(task_name, unknown))
}

pub(in crate::runner) fn ensure_no_unknown_builtin_args_with_prefix(
    task_name: &str,
    prefix: &str,
    unknown: &[String],
) -> Result<(), RunnerError> {
    if unknown.is_empty() {
        return Ok(());
    }
    let mut args = Vec::with_capacity(unknown.len() + 1);
    args.push(prefix.to_owned());
    args.extend(unknown.iter().cloned());
    Err(unknown_builtin_args(task_name, &args))
}

pub(in crate::runner) fn has_builtin_help_flag(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

pub(in crate::runner) fn has_builtin_json_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

pub(in crate::runner) fn render_builtin_help_topic(
    topic: HelpTopic,
    topic_name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut renderer = standard_renderer(output_json);
    render_help(&mut renderer, topic)?;
    let rendered = render_utf8(renderer.into_inner())?;
    render_builtin_help_text(topic_name, rendered, output_json)
}

pub(in crate::runner) fn render_builtin_help_text(
    topic_name: &str,
    text: String,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        let payload = schema_payload(
            "effigy.help.v1",
            json!({
                "topic": topic_name,
                "text": text,
            }),
        );
        return encode_json(&payload, true);
    }
    Ok(text)
}

pub(in crate::runner) fn render_builtin_general_help_for_root(
    root: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut renderer = standard_renderer(output_json);
    let deferred_builtins = crate::runner::explicitly_deferred_builtins_for_root(root);
    crate::render_help_with_deferred_builtins(
        &mut renderer,
        HelpTopic::General,
        &deferred_builtins,
    )?;
    let rendered = render_utf8(renderer.into_inner())?;
    render_builtin_help_text("general", rendered, output_json)
}
