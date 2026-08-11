use std::collections::BTreeSet;
use std::path::Path;

use serde_json::json;

use crate::test::planning::BuiltinTestTargetSet;
use effigy_cli::TaskInvocation;

use super::super::super::super::response::schema_payload_versioned;
use super::plan_projection::project_target_plan;

pub(super) fn build_builtin_test_plan_payload(
    task: &TaskInvocation,
    resolved_root: &Path,
    target_set: &BuiltinTestTargetSet,
    requested_suite: Option<&str>,
    passthrough: &[String],
    runtime_mode: &str,
) -> serde_json::Value {
    let target_values = target_set
        .targets
        .iter()
        .map(|target| {
            let projection = project_target_plan(target, requested_suite, passthrough);
            json!({
                "name": target.name,
                "root": target.root.display().to_string(),
                "suite_source": target.suite_source,
                "cargo_env_match": projection.cargo_env_match,
                "available_suites": projection.available_suites,
                "selected_suites": projection.selected_suites,
                "default_suites": projection.default_suites,
                "commands": projection.commands,
                "evidence": projection.evidence,
                "suite_details": projection.suite_details.iter().map(|suite| json!({
                    "suite": suite.suite,
                    "command": suite.command,
                    "evidence": suite.evidence,
                    "suite_env": suite.suite_env,
                    "suite_env_files": suite.suite_env_files,
                    "setup_steps": suite.setup_steps,
                    "teardown_steps": suite.teardown_steps,
                    "teardown_policy": suite.teardown_policy,
                    "default": suite.is_default,
                })).collect::<Vec<serde_json::Value>>(),
                "fallback_chain": target.fallback_chain,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    schema_payload_versioned(
        "effigy.test.plan.v1",
        json!({
            "request": task.name,
            "root": resolved_root.display().to_string(),
            "runtime": runtime_mode,
            "targets": target_values,
            "excluded_targets": target_set.excluded_targets,
            "warnings": target_set.warnings,
            "recovery": serde_json::Value::Null,
        }),
    )
}

pub(super) fn build_builtin_test_plan_recovery_payload(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
) -> serde_json::Value {
    schema_payload_versioned(
        "effigy.test.plan.v1",
        json!({
            "request": task.name,
            "root": root.display().to_string(),
            "runtime": "plan-recovery",
            "targets": [],
            "recovery": {
                "message": message,
                "available_suites": available_runners.iter().cloned().collect::<Vec<String>>(),
            }
        }),
    )
}
