use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;
use serde_json::json;

use super::super::super::util::shell_quote;
use super::planning::{BuiltinTestCliFlags, BuiltinTestTarget};
use super::suite_selection::{render_available_suites, BuiltinSuiteSelectionError};
use super::{execution::should_run_builtin_test_tui, BuiltinTestExecResult, RunnerError};

pub(super) fn render_suite_selection_failure(
    task: &TaskInvocation,
    resolved_root: &Path,
    flags: BuiltinTestCliFlags,
    selection_error: BuiltinSuiteSelectionError,
) -> Result<Option<String>, RunnerError> {
    if flags.plan_mode {
        return render_builtin_test_plan_recovery(
            task,
            resolved_root,
            &selection_error.available_runners,
            &selection_error.message,
            flags.output_json,
        )
        .map(Some);
    }
    Err(RunnerError::TaskInvocation(selection_error.message))
}

pub(super) fn render_builtin_test_plan(
    task: &TaskInvocation,
    root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runnable_count: usize,
    flags: BuiltinTestCliFlags,
) -> Result<String, RunnerError> {
    let runtime_mode = if should_run_builtin_test_tui(flags.tui, runnable_count) {
        "tui"
    } else {
        "text"
    };

    if flags.output_json {
        let payload = build_builtin_test_plan_payload(
            task,
            root,
            targets,
            requested_suite,
            passthrough,
            runtime_mode,
        );
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Test Plan")?;
    renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("root", root.display().to_string()),
        KeyValue::new("targets", runnable_count.to_string()),
        KeyValue::new("runtime", runtime_mode.to_owned()),
    ])?;
    renderer.text("")?;
    renderer.section("Target Summary")?;
    let summary_lines = targets
        .iter()
        .map(|target| {
            let available_suites = target
                .plans
                .iter()
                .map(|plan| plan.suite.as_str())
                .collect::<BTreeSet<&str>>()
                .into_iter()
                .collect::<Vec<&str>>()
                .join(", ");
            format!(
                "{}: source={} suites={}",
                target.name, target.suite_source, available_suites
            )
        })
        .collect::<Vec<String>>();
    renderer.bullet_list("targets", &summary_lines)?;
    renderer.text("")?;
    for target in targets {
        let available_suites = target
            .plans
            .iter()
            .map(|plan| plan.suite.as_str())
            .collect::<BTreeSet<&str>>()
            .into_iter()
            .collect::<Vec<&str>>()
            .join(", ");
        let mut selected_plans = target.plans.clone();
        if let Some(requested) = requested_suite {
            selected_plans.retain(|plan| plan.suite == requested);
        }
        renderer.section(&format!("Target: {}", target.name))?;
        if !selected_plans.is_empty() {
            let args_rendered = passthrough
                .iter()
                .map(|arg| shell_quote(arg))
                .collect::<Vec<String>>()
                .join(" ");
            let runners = selected_plans
                .iter()
                .map(|plan| plan.suite.as_str())
                .collect::<Vec<&str>>()
                .join(", ");
            let commands = selected_plans
                .iter()
                .map(|plan| {
                    if args_rendered.is_empty() {
                        plan.command.clone()
                    } else {
                        format!("{} {}", plan.command, args_rendered)
                    }
                })
                .collect::<Vec<String>>();
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", runners),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
            ])?;
            renderer.text("")?;
            renderer.bullet_list("command", &commands)?;
            renderer.text("")?;
            let mut evidence = Vec::<String>::new();
            for plan in &selected_plans {
                for line in &plan.evidence {
                    evidence.push(format!("{}: {line}", plan.suite));
                }
            }
            renderer.bullet_list("evidence", &evidence)?;
        } else {
            renderer.key_values(&[
                KeyValue::new("root", target.root.display().to_string()),
                KeyValue::new("runner", "<none>".to_owned()),
                KeyValue::new("available-suites", available_suites.clone()),
                KeyValue::new("suite-source", target.suite_source.clone()),
                KeyValue::new("command", "<none>".to_owned()),
            ])?;
            renderer.text("")?;
            renderer.notice(
                NoticeLevel::Warning,
                "no supported test runner detected for this target",
            )?;
        }
        renderer.text("")?;
        renderer.bullet_list("fallback-chain", &target.fallback_chain)?;
        renderer.text("")?;
    }
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn render_builtin_test_results(
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

pub(super) fn render_builtin_test_results_json(
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

pub(super) fn append_builtin_test_filter_hint(
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

fn build_builtin_test_plan_payload(
    task: &TaskInvocation,
    resolved_root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runtime_mode: &str,
) -> serde_json::Value {
    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let target_values = targets
        .iter()
        .map(|target| {
            let available = target
                .plans
                .iter()
                .map(|plan| plan.suite.clone())
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect::<Vec<String>>();
            let mut selected_plans = target.plans.clone();
            if let Some(requested) = requested_suite {
                selected_plans.retain(|plan| plan.suite == requested);
            }
            let selected_suites = selected_plans
                .iter()
                .map(|plan| plan.suite.clone())
                .collect::<Vec<String>>();
            let commands = selected_plans
                .iter()
                .map(|plan| {
                    if args_rendered.is_empty() {
                        plan.command.clone()
                    } else {
                        format!("{} {}", plan.command, args_rendered)
                    }
                })
                .collect::<Vec<String>>();
            let evidence = selected_plans
                .iter()
                .flat_map(|plan| {
                    plan.evidence
                        .iter()
                        .map(|line| format!("{}: {line}", plan.suite))
                        .collect::<Vec<String>>()
                })
                .collect::<Vec<String>>();
            json!({
                "name": target.name,
                "root": target.root.display().to_string(),
                "suite_source": target.suite_source,
                "available_suites": available,
                "selected_suites": selected_suites,
                "commands": commands,
                "evidence": evidence,
                "fallback_chain": target.fallback_chain,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    json!({
        "schema": "effigy.test.plan.v1",
        "schema_version": 1,
        "request": task.name,
        "root": resolved_root.display().to_string(),
        "runtime": runtime_mode,
        "targets": target_values,
        "recovery": serde_json::Value::Null,
    })
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

fn render_builtin_test_plan_recovery(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        let payload = json!({
            "schema": "effigy.test.plan.v1",
            "schema_version": 1,
            "request": task.name,
            "root": root.display().to_string(),
            "runtime": "plan-recovery",
            "targets": [],
            "recovery": {
                "message": message,
                "available_suites": available_runners.iter().cloned().collect::<Vec<String>>(),
            }
        });
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Test Plan")?;
    renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("root", root.display().to_string()),
        KeyValue::new("runtime", "plan-recovery".to_owned()),
        KeyValue::new(
            "available-suites",
            render_available_suites(available_runners),
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(NoticeLevel::Warning, message)?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
