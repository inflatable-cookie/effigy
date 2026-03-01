use std::collections::BTreeSet;
use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer};
use serde_json::json;

use super::super::planning::BuiltinTestTarget;
use super::super::{BuiltinTestExecResult, RunnerError};

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
    let suite_source_by_root = targets
        .iter()
        .map(|target| {
            (
                target.root.display().to_string(),
                target.suite_source.clone(),
                target
                    .plans
                    .iter()
                    .map(|plan| plan.suite.clone())
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect::<Vec<String>>(),
            )
        })
        .collect::<Vec<(String, String, Vec<String>)>>();
    let mut failures = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| {
            json!({
                "target": result.name,
                "suite": result.runner,
                "code": result.code,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    failures.sort_by(|a, b| {
        a.get("target")
            .and_then(|v| v.as_str())
            .cmp(&b.get("target").and_then(|v| v.as_str()))
    });
    let target_values = results
        .iter()
        .map(|result| {
            let root_rendered = result.root.display().to_string();
            let (suite_source, available_suites) = suite_source_by_root
                .iter()
                .find(|(root, _, _)| root == &root_rendered)
                .map(|(_, source, suites)| (source.clone(), suites.clone()))
                .unwrap_or_else(|| ("unknown".to_owned(), vec![result.runner.clone()]));
            json!({
                "target": result.name,
                "suite": result.runner,
                "root": root_rendered,
                "suite_source": suite_source,
                "available_suites": available_suites,
                "command": result.command,
                "success": result.success,
                "code": result.code,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let payload = json!({
        "schema": "effigy.test.results.v1",
        "schema_version": 1,
        "targets": target_values,
        "failures": failures,
        "hint": build_builtin_test_filter_hint_payload(results, requested_suite, passthrough),
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(crate) fn append_builtin_test_filter_hint(
    mut rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    if requested_suite.is_none() || passthrough.is_empty() {
        return rendered;
    }

    let failed = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| result.command.clone())
        .collect::<Vec<String>>();
    if failed.is_empty() {
        return rendered;
    }

    rendered.push_str("\nHint\n────\n");
    rendered.push_str(
        "Selected suite failed while using a test filter. This often means no tests matched.\n",
    );
    rendered.push_str("failed command(s):\n");
    for command in failed {
        rendered.push_str("- ");
        rendered.push_str(&command);
        rendered.push('\n');
    }
    rendered.push_str("Try again without the filter to verify suite execution.\n");
    rendered
}

fn build_builtin_test_filter_hint_payload(
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> Option<serde_json::Value> {
    if requested_suite.is_none() || passthrough.is_empty() {
        return None;
    }
    let failed = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| result.command.clone())
        .collect::<Vec<String>>();
    if failed.is_empty() {
        return None;
    }
    Some(json!({
        "kind": "selected-suite-filter-no-match",
        "message": "Selected suite failed while using a test filter. This often means no tests matched.",
        "failed_commands": failed,
        "suggestion": "Try again without the filter to verify suite execution.",
    }))
}
