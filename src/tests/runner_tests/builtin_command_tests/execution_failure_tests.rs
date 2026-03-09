use super::prelude::{
    assert_builtin_test_non_zero, assert_json_array_field, assert_json_string_field_eq,
    assert_output_excludes_all, install_local_vitest, parse_json_output_with_schema,
    run_builtin_err, setup_fanout_catalog_repo, temp_workspace,
    write_package_json_with_test_script, RunnerError,
};

#[test]
fn run_manifest_task_builtin_test_failure_keeps_rendered_results_summary() {
    let root = temp_workspace("builtin-test-fanout-failure-summary");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&catalog_a, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&catalog_b, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &[]);

    assert_builtin_test_non_zero(
        err,
        Some(vec![("catalog_a".to_owned(), Some(1))]),
        &["Test Results", "catalog_b", "ok", "catalog_a", "exit=1"],
        &["runner:vitest", "command:"],
    );
}

#[test]
fn run_manifest_task_builtin_test_json_failure_includes_results_and_failures() {
    let root = temp_workspace("builtin-test-fanout-failure-json");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&catalog_a, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&catalog_b, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &["--json"]);
    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            assert_eq!(failures, vec![("catalog_a".to_owned(), Some(1))]);
            let parsed = parse_json_output_with_schema(&rendered, "effigy.test.results.v1");
            assert_json_string_field_eq(&parsed["failures"][0], "target", "catalog_a");
            let target_names = parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .filter_map(|entry| entry["target"].as_str())
                .collect::<Vec<&str>>();
            assert!(target_names.contains(&"catalog_b"));
            assert!(target_names.contains(&"catalog_a"));
            assert!(parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .all(|entry| entry["cargo_env_match"].is_string()));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_json_failure_does_not_include_text_hint_footer() {
    let root = temp_workspace("builtin-test-json-filtered-failure-no-hint");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 1\n");

    let err = run_builtin_err(root, "test", &["--json", "vitest", "user-service"]);
    match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => {
            assert_output_excludes_all(&rendered, &["\nHint\n────\n"]);
            let parsed = parse_json_output_with_schema(&rendered, "effigy.test.results.v1");
            assert_json_array_field(&parsed, "failures");
            assert_json_string_field_eq(&parsed["hint"], "kind", "selected-suite-filter-no-match");
            assert_json_string_field_eq(
                &parsed["hint"],
                "suggestion",
                "Try again without the filter to verify suite execution.",
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_failure_with_suite_filter_shows_no_match_hint() {
    let root = temp_workspace("builtin-test-filtered-failure-hint");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 1\n");

    let err = run_builtin_err(root, "test", &["vitest", "user-service"]);

    assert_builtin_test_non_zero(
        err,
        None,
        &[
            "Hint",
            "often means no tests matched",
            "vitest run 'user-service'",
            "Try again without the filter",
        ],
        &[],
    );
}
