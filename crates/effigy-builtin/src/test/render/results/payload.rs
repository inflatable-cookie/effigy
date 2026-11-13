use serde_json::json;

use crate::test::planning::BuiltinTestTarget;
use crate::test::BuiltinTestExecResult;

use super::super::super::super::response::schema_payload_versioned;
use super::hint::build_builtin_test_filter_hint_payload;
use crate::test::render::suite_facts::SuiteFactsIndex;

pub(super) fn render_builtin_test_results_json(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> serde_json::Value {
    let facts = SuiteFactsIndex::new(targets);
    let mut failures = results
        .iter()
        .filter(|result| !result.success())
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
            let suite = facts.for_result(&result.root, &result.runner);
            json!({
                "target": result.name,
                "suite": result.runner,
                "root": result.root.display().to_string(),
                "suite_source": suite.suite_source,
                "cargo_env_match": suite.cargo_env_match,
                "available_suites": suite.available_suites,
                "suite_env": suite.suite_env,
                "suite_env_files": suite.suite_env_files,
                "setup_steps": suite.setup_steps,
                "teardown_steps": suite.teardown_steps,
                "teardown_policy": suite.teardown_policy.as_str(),
                "command": result.command,
                "success": result.success(),
                "outcome": result.outcome.as_str(),
                "code": result.code,
            })
        })
        .collect::<Vec<serde_json::Value>>();

    schema_payload_versioned(
        "effigy.test.results.v1",
        json!({
            "targets": target_values,
            "failures": failures,
            "hint": build_builtin_test_filter_hint_payload(results, requested_suite, passthrough),
        }),
    )
}
