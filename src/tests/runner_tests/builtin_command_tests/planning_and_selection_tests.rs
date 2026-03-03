use super::prelude::*;

#[test]
fn run_manifest_task_builtin_test_plan_renders_detection_summary() {
    let root = temp_workspace("builtin-test-plan");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
        &out,
        &[
            "Test Plan",
            "targets:",
            "runtime:",
            "text",
            "Target: root",
            "runner:",
            "available-suites:",
            "suite-source: auto-detected",
            "vitest",
            "fallback-chain",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_json_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-schema");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "--json"]);
    let parsed = parse_json_output(&out);
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["targets"].is_array());
    assert_eq!(parsed["recovery"], serde_json::Value::Null);
}

#[test]
fn run_manifest_task_builtin_test_plan_marks_configured_suite_source() {
    let root = temp_workspace("builtin-test-plan-configured-suite-source");
    write_test_suites_manifest(&root, &[("unit", "pnpm exec vitest run")]);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
        &out,
        &[
            "Test Plan",
            "available-suites: unit",
            "suite-source: configured",
            "test.suites.unit",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_mixed_workspace_reports_configured_and_auto_detected_sources(
) {
    let root = temp_workspace("builtin-test-plan-mixed-suite-sources");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_root_manifest(&root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
[test.suites]
unit = "pnpm exec vitest run"
"#,
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&dairy);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(
        &out,
        &[
            "Target Summary",
            "farmyard: source=configured suites=unit",
            "dairy: source=auto-detected suites=vitest",
            "Target: farmyard",
            "available-suites: unit",
            "suite-source: configured",
            "test.suites.unit",
            "Target: dairy",
            "suite-source: auto-detected",
            "vitest",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_uses_configured_suites_as_source_of_truth() {
    let root = temp_workspace("builtin-test-configured-suites-source-of-truth");
    let configured_marker = root.join("configured-suite.log");
    let vitest_marker = root.join("vitest-suite.log");
    let manifest = format!(
        r#"[test.suites]
unit = "sh -lc 'printf configured > \"{}\"'"
"#,
        configured_marker.display()
    );
    write_root_manifest(&root, &manifest);
    write_package_json_with_test_script(&root);
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.clone(), "test", &["--verbose-results"]);
    assert_contains_all(&out, &["Test Results", "runner:unit"]);
    assert!(configured_marker.exists(), "configured suite should run");
    assert!(
        !vitest_marker.exists(),
        "auto-detected vitest should not run"
    );
}

#[test]
fn run_manifest_task_builtin_test_with_configured_multi_suite_requires_explicit_suite() {
    let root = temp_workspace("builtin-test-configured-multi-suite-ambiguous");
    write_test_suites_manifest(&root, &[("unit", "true"), ("integration", "true")]);

    let err = run_builtin_err(root, "test", &["user-service"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "ambiguous",
            "unit",
            "integration",
            "effigy test unit user-service",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_supports_configured_custom_suite_selector() {
    let root = temp_workspace("builtin-test-configured-custom-suite-selector");
    let unit_marker = root.join("unit-suite.log");
    let integration_marker = root.join("integration-suite.log");
    let manifest = format!(
        r#"[test.suites]
unit = "sh -lc 'printf unit > \"{}\"'"
integration = "sh -lc 'printf integration > \"{}\"'"
"#,
        unit_marker.display(),
        integration_marker.display()
    );
    write_root_manifest(&root, &manifest);

    let out = run_builtin_ok(root, "test", &["unit"]);
    assert_contains_all(&out, &["Test Results"]);
    assert!(unit_marker.exists(), "selected suite should run");
    assert!(
        !integration_marker.exists(),
        "non-selected suite should not run"
    );
}

#[test]
fn run_manifest_task_builtin_test_multi_suite_selector_errors_include_recovery_hints() {
    let cases = [
        BuiltinTestErrorCase {
            workspace: "builtin-test-multi-suite-ambiguous",
            args: &["user-service"],
            expected: &[
                "ambiguous",
                "vitest",
                "cargo-",
                "Try one of:",
                "Use `effigy test --plan <args>`",
                "effigy test vitest user-service",
                "effigy test cargo-",
            ],
        },
        BuiltinTestErrorCase {
            workspace: "builtin-test-mistyped-suite-suggestion",
            args: &["viteest", "user-service"],
            expected: &[
                "runner `viteest` is not available",
                "Did you mean `vitest`?",
                "Try: effigy test vitest user-service",
                "Use `effigy test --plan <args>`",
            ],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        setup_multi_suite_repo(&root);
        let err = run_builtin_err(root, "test", case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}

#[test]
fn run_manifest_task_builtin_test_plan_multi_suite_recovery_outputs_hints() {
    let cases = [
        BuiltinTestRecoveryCase {
            workspace: "builtin-test-multi-suite-plan-recovery",
            args: &["--plan", "user-service"],
            expected: &[
                "Test Plan",
                "runtime: plan-recovery",
                "available-suites:",
                "ambiguous",
                "Try one of:",
            ],
        },
        BuiltinTestRecoveryCase {
            workspace: "builtin-test-plan-mistyped-suite-recovery",
            args: &["--plan", "viteest", "user-service"],
            expected: &[
                "Test Plan",
                "runtime: plan-recovery",
                "Did you mean `vitest`?",
                "Try: effigy test vitest user-service",
            ],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        setup_multi_suite_repo(&root);
        let out = run_builtin_ok(root, "test", case.args);
        assert_contains_all(&out, case.expected);
    }
}

#[test]
fn run_manifest_task_builtin_test_plan_json_recovery_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-recovery-schema");
    setup_multi_suite_repo(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "--json", "user-service"]);
    let parsed = parse_json_output(&out);
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["runtime"], "plan-recovery");
    assert!(parsed["recovery"].is_object());
}

#[test]
fn run_manifest_task_builtin_test_plan_text_and_json_projection_consistency() {
    let root = temp_workspace("builtin-test-plan-projection-consistency");
    write_test_suites_manifest(
        &root,
        &[
            ("unit", "pnpm exec vitest run"),
            ("integration", "pnpm exec vitest run --project integration"),
        ],
    );

    let text = run_builtin_ok(root.clone(), "test", &["--plan", "unit", "user-service"]);
    let json = run_builtin_ok(root, "test", &["--plan", "--json", "unit", "user-service"]);
    let parsed = parse_json_output(&json);
    let target = &parsed["targets"][0];

    let available_suites = target["available_suites"]
        .as_array()
        .expect("available suites array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<&str>>();
    let selected_suites = target["selected_suites"]
        .as_array()
        .expect("selected suites array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<&str>>();
    assert_eq!(selected_suites, vec!["unit"]);

    for suite in available_suites {
        assert!(
            text.contains(suite),
            "missing suite in text output: {suite}"
        );
    }
    for command in target["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|value| value.as_str())
    {
        assert!(
            text.contains(command),
            "missing command in text output: {command}"
        );
    }
    for evidence in target["evidence"]
        .as_array()
        .expect("evidence array")
        .iter()
        .filter_map(|value| value.as_str())
    {
        assert!(
            text.contains(evidence),
            "missing evidence in text output: {evidence}"
        );
    }
}

#[test]
fn run_manifest_task_builtin_test_supports_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector");
    setup_multi_suite_repo(&root);
    let vitest_marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.clone(), "test", &["vitest", "user-service"]);
    assert_contains_all(&out, &["Test Results", "root/vitest"]);
    assert!(!out.contains("root/cargo-"));
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_errors_for_unavailable_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector-unavailable");
    write_package_json_with_test_script(&root);

    let err = run_builtin_err(root, "test", &["nextest"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "not available",
            "nextest",
            "vitest",
            "Try one of:",
            "Use `effigy test --plan <args>`",
            "effigy test vitest",
        ],
    );
}

#[test]
fn run_manifest_task_explicit_test_task_overrides_builtin_auto_detection() {
    let root = temp_workspace("builtin-test-explicit-override");
    write_root_manifest(
        &root,
        "[tasks.test]\nrun = \"printf explicit > explicit-test.log\"\n",
    );
    write_package_json_with_test_script(&root);

    assert_builtin_ok_empty(root.clone(), "test", &[]);
    assert!(
        root.join("explicit-test.log").exists(),
        "explicit task should run before builtin test detection"
    );
}

#[test]
fn run_manifest_task_builtin_test_falls_through_to_deferral_when_no_detection_matches() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-defers");
    write_root_manifest(
        &root,
        "[defer]\nrun = \"test {request} = 'test' && test {args} = '--watch'\"\n",
    );

    assert_builtin_ok_empty(root, "test", &["--watch"]);
}
