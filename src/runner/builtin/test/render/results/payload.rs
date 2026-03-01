use std::collections::BTreeSet;

use serde_json::json;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::builtin::test::BuiltinTestExecResult;

use super::hint::build_builtin_test_filter_hint_payload;

pub(super) fn render_builtin_test_results_json(
    results: &[BuiltinTestExecResult],
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> serde_json::Value {
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

    json!({
        "schema": "effigy.test.results.v1",
        "schema_version": 1,
        "targets": target_values,
        "failures": failures,
        "hint": build_builtin_test_filter_hint_payload(results, requested_suite, passthrough),
    })
}
