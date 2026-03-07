use std::collections::BTreeSet;

use serde_json::json;

use crate::runner::builtin::test::planning::BuiltinTestTarget;
use crate::runner::builtin::test::BuiltinTestExecResult;

use super::super::super::super::response::schema_payload_versioned;
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
                target.cargo_env_match.as_str().to_owned(),
                target
                    .plans
                    .iter()
                    .map(|plan| {
                        (
                            plan.suite.clone(),
                            plan.suite_env.clone(),
                            plan.suite_env_files.clone(),
                            plan.setup_steps,
                            plan.teardown_steps,
                            match plan.teardown_policy {
                                crate::runner::manifest::ManifestTestSuiteTeardownPolicy::Always => {
                                    "always".to_owned()
                                }
                                crate::runner::manifest::ManifestTestSuiteTeardownPolicy::OnSuccess => {
                                    "on-success".to_owned()
                                }
                            },
                        )
                    })
                    .collect::<Vec<(String, Option<String>, Vec<String>, usize, usize, String)>>(),
            )
        })
        .collect::<Vec<(
            String,
            String,
            String,
            Vec<(String, Option<String>, Vec<String>, usize, usize, String)>,
        )>>();
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
            let (
                suite_source,
                cargo_env_match,
                available_suites,
                suite_env,
                suite_env_files,
                setup_steps,
                teardown_steps,
                teardown_policy,
            ) = suite_source_by_root
                .iter()
                .find(|(root, _, _, _)| root == &root_rendered)
                .map(|(_, source, mode, suite_details)| {
                    let available_suites = suite_details
                        .iter()
                        .map(|(suite, _, _, _, _, _)| suite.clone())
                        .collect::<BTreeSet<String>>()
                        .into_iter()
                        .collect::<Vec<String>>();
                    let suite_metadata = suite_details
                        .iter()
                        .find(|(suite, _, _, _, _, _)| suite == &result.runner)
                        .cloned()
                        .unwrap_or_else(|| {
                            (
                                result.runner.clone(),
                                None,
                                Vec::new(),
                                0,
                                0,
                                "on-success".to_owned(),
                            )
                        });
                    (
                        source.clone(),
                        mode.clone(),
                        available_suites,
                        suite_metadata.1,
                        suite_metadata.2,
                        suite_metadata.3,
                        suite_metadata.4,
                        suite_metadata.5,
                    )
                })
                .unwrap_or_else(|| {
                    (
                        "unknown".to_owned(),
                        "unknown".to_owned(),
                        vec![result.runner.clone()],
                        None,
                        Vec::new(),
                        0,
                        0,
                        "on-success".to_owned(),
                    )
                });
            json!({
                "target": result.name,
                "suite": result.runner,
                "root": root_rendered,
                "suite_source": suite_source,
                "cargo_env_match": cargo_env_match,
                "available_suites": available_suites,
                "suite_env": suite_env,
                "suite_env_files": suite_env_files,
                "setup_steps": setup_steps,
                "teardown_steps": teardown_steps,
                "teardown_policy": teardown_policy,
                "command": result.command,
                "success": result.success,
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
