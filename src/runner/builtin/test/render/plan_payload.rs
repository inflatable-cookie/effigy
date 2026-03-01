use std::collections::BTreeSet;
use std::path::Path;

use serde_json::json;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::util::shell_quote;
use crate::TaskInvocation;

pub(super) fn build_builtin_test_plan_payload(
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

pub(super) fn build_builtin_test_plan_recovery_payload(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
) -> serde_json::Value {
    json!({
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
    })
}
