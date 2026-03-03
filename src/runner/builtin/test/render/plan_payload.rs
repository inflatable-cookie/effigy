use std::collections::BTreeSet;
use std::path::Path;

use serde_json::json;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::TaskInvocation;

use super::plan_projection::project_target_plan;

pub(super) fn build_builtin_test_plan_payload(
    task: &TaskInvocation,
    resolved_root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runtime_mode: &str,
) -> serde_json::Value {
    let target_values = targets
        .iter()
        .map(|target| {
            let projection = project_target_plan(target, requested_suite, passthrough);
            json!({
                "name": target.name,
                "root": target.root.display().to_string(),
                "suite_source": target.suite_source,
                "available_suites": projection.available_suites,
                "selected_suites": projection.selected_suites,
                "commands": projection.commands,
                "evidence": projection.evidence,
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
