use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;
use serde_json::json;

use super::super::super::super::util::shell_quote;
use super::super::execution::should_run_builtin_test_tui;
use super::super::planning::{BuiltinTestCliFlags, BuiltinTestTarget};
use super::super::suite_selection::{render_available_suites, BuiltinSuiteSelectionError};
use super::super::RunnerError;

pub(crate) fn render_suite_selection_failure(
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

pub(crate) fn render_builtin_test_plan(
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
