use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer};

use super::super::planning::BuiltinTestTarget;
use super::super::{BuiltinTestExecResult, RunnerError};

#[path = "results/hint.rs"]
mod hint;
#[path = "results/payload.rs"]
mod payload;

pub(crate) fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    verbose: bool,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", results.len().to_string())])?;
    renderer.text("")?;
    let mut ordered = results
        .iter()
        .map(|result| {
            (
                result.name.clone(),
                result.runner.clone(),
                result.root.display().to_string(),
                result.command.clone(),
                result.success,
                result.code,
            )
        })
        .collect::<Vec<(String, String, String, String, bool, Option<i32>)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, runner, root, command, success, code) in ordered {
        let status = if success {
            "ok".to_owned()
        } else {
            match code {
                Some(value) => format!("exit={value}"),
                None => "terminated".to_owned(),
            }
        };
        let value = if verbose {
            format!("{status}  runner:{runner}  root:{root}  command:{command}")
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(name, value)])?;
    }
    renderer.text("")?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(crate) fn render_builtin_test_results_json(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> Result<String, RunnerError> {
    let payload =
        payload::render_builtin_test_results_json(results, targets, requested_suite, passthrough);
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(crate) fn append_builtin_test_filter_hint(
    rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    hint::append_builtin_test_filter_hint(rendered, results, requested_suite, passthrough)
}

pub(crate) fn finalize_builtin_test_outcome(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    let mut failures = results
        .iter()
        .filter_map(|result| {
            if result.success {
                None
            } else {
                Some((result.name.clone(), result.code))
            }
        })
        .collect::<Vec<(String, Option<i32>)>>();
    failures.sort_by(|a, b| a.0.cmp(&b.0));

    let rendered_text = render_builtin_test_results(results, verbose_results)?;
    let rendered_json = if output_json {
        Some(render_builtin_test_results_json(
            results,
            targets,
            requested_suite,
            passthrough,
        )?)
    } else {
        None
    };

    if failures.is_empty() {
        return Ok(Some(rendered_json.unwrap_or(rendered_text)));
    }

    if let Some(json) = rendered_json {
        return Err(RunnerError::BuiltinTestNonZero {
            failures,
            rendered: json,
        });
    }

    let rendered = append_builtin_test_filter_hint(
        rendered_text,
        results,
        requested_suite,
        passthrough,
    );
    Err(RunnerError::BuiltinTestNonZero { failures, rendered })
}
