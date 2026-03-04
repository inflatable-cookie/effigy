use std::collections::BTreeMap;

use crate::ui::{KeyValue, Renderer};

use super::super::super::super::render::{render_utf8, text_renderer};
use super::super::super::response::render_text_or_json_lazy;
use super::super::planning::BuiltinTestTarget;
use super::super::{BuiltinTestExecResult, RunnerError};

#[path = "results/hint.rs"]
mod hint;
#[path = "results/payload.rs"]
mod payload;

pub(crate) fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    verbose: bool,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", results.len().to_string())])?;
    renderer.text("")?;
    let cargo_env_match_by_root = targets
        .iter()
        .map(|target| {
            (
                target.root.display().to_string(),
                target.cargo_env_match.as_str().to_owned(),
            )
        })
        .collect::<BTreeMap<String, String>>();
    let mut ordered = results
        .iter()
        .map(|result| {
            let root = result.root.display().to_string();
            (
                result.name.clone(),
                result.runner.clone(),
                root.clone(),
                cargo_env_match_by_root
                    .get(&root)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_owned()),
                result.command.clone(),
                result.success,
                result.code,
            )
        })
        .collect::<Vec<(String, String, String, String, String, bool, Option<i32>)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, runner, root, cargo_env_match, command, success, code) in ordered {
        let status = if success {
            "ok".to_owned()
        } else {
            match code {
                Some(value) => format!("exit={value}"),
                None => "terminated".to_owned(),
            }
        };
        let value = if verbose {
            format!(
                "{status}  runner:{runner}  root:{root}  cargo-env-match:{cargo_env_match}  command:{command}"
            )
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(name, value)])?;
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner())
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

    if failures.is_empty() {
        let rendered = render_builtin_test_outcome_rendered(
            results,
            targets,
            requested_suite,
            passthrough,
            verbose_results,
            output_json,
            false,
        )?;
        return Ok(Some(rendered));
    }

    let rendered = render_builtin_test_outcome_rendered(
        results,
        targets,
        requested_suite,
        passthrough,
        verbose_results,
        output_json,
        true,
    )?;
    Err(RunnerError::BuiltinTestNonZero { failures, rendered })
}

fn render_builtin_test_outcome_rendered(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    verbose_results: bool,
    output_json: bool,
    include_filter_hint: bool,
) -> Result<String, RunnerError> {
    render_text_or_json_lazy(
        output_json,
        || {
            let rendered = render_builtin_test_results(results, targets, verbose_results)?;
            if include_filter_hint {
                return Ok(append_builtin_test_filter_hint(
                    rendered,
                    results,
                    requested_suite,
                    passthrough,
                ));
            }
            Ok(rendered)
        },
        || {
            payload::render_builtin_test_results_json(
                results,
                targets,
                requested_suite,
                passthrough,
            )
        },
    )
}
