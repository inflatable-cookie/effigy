use super::*;

struct BuiltinTestErrorCase {
    workspace: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
}

struct BuiltinTestRecoveryCase {
    workspace: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
}

fn setup_fanout_catalog_repo(root: &PathBuf) -> (PathBuf, PathBuf) {
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_package_json_with_test_script(&farmyard);
    write_package_json_with_test_script(&dairy);
    (farmyard, dairy)
}

fn assert_builtin_test_non_zero(
    err: RunnerError,
    expected_failures: Option<Vec<(String, Option<i32>)>>,
    expected_rendered_snippets: &[&str],
    unexpected_rendered_snippets: &[&str],
) {
    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            if let Some(expected) = expected_failures {
                assert_eq!(failures, expected);
            }
            assert_contains_all(&rendered, expected_rendered_snippets);
            for snippet in unexpected_rendered_snippets {
                assert!(!rendered.contains(snippet));
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

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
fn run_manifest_task_builtin_test_executes_local_vitest() {
    let root = temp_workspace("builtin-test-exec-vitest");
    let marker = root.join("vitest-called.log");
    write_package_json_with_test_script(&root);
    install_local_vitest_marker(&root, &marker);

    let out = run_builtin_ok(root.clone(), "test", &["--run"]);
    assert_contains_all(&out, &["Test Results", "targets:", "root"]);
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(marker.exists(), "vitest stub should be invoked");
}

#[test]
fn run_manifest_task_builtin_test_json_suppresses_child_process_output() {
    let root = temp_workspace("builtin-test-json-suppresses-child-output");
    write_package_json_with_test_script(&root);
    install_local_vitest(
        &root,
        "#!/bin/sh\nprintf noisy-stdout\nprintf noisy-stderr >&2\nexit 0\n",
    );

    let out = run_builtin_ok(root, "test", &["--json", "--run"]);

    assert!(
        !out.contains("noisy-stdout"),
        "child stdout leaked into json output"
    );
    assert!(
        !out.contains("noisy-stderr"),
        "child stderr leaked into json output"
    );
    let parsed = parse_json_output(&out);
    assert_eq!(parsed["schema"], "effigy.test.results.v1");
}

#[test]
fn run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-multi-context");
    write_package_json_with_test_script(&root);
    write_multi_suite_cargo_manifest(&root);
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn smoke() {\n        assert!(super::ok());\n    }\n}\n",
    )
    .expect("write lib");

    let vitest_marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root/vitest", "root/cargo-"]);
    assert!(vitest_marker.exists(), "vitest suite should run");
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

#[test]
fn run_manifest_task_builtin_test_fans_out_across_catalog_roots() {
    let root = temp_workspace("builtin-test-fanout");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");
    install_local_vitest_marker(&farmyard, &farmyard_marker);
    install_local_vitest_marker(&dairy, &dairy_marker);

    let out = run_builtin_ok(root, "test", &[]);
    assert_contains_all(&out, &["Test Results", "targets:", "dairy", "farmyard"]);
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(dairy_marker.exists(), "dairy vitest should run");
}

#[test]
fn run_manifest_task_prefixed_builtin_test_targets_catalog_root_only() {
    let root = temp_workspace("builtin-test-prefixed-catalog");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");
    install_local_vitest_marker(&farmyard, &farmyard_marker);
    install_local_vitest_marker(&dairy, &dairy_marker);

    let out = run_builtin_ok(root, "farmyard/test", &[]);
    assert_contains_all(&out, &["Test Results", "farmyard"]);
    assert!(!out.contains("dairy"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(!dairy_marker.exists(), "dairy vitest should not run");
}

#[test]
fn run_manifest_task_builtin_test_failure_keeps_rendered_results_summary() {
    let root = temp_workspace("builtin-test-fanout-failure-summary");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&farmyard, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&dairy, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &[]);

    assert_builtin_test_non_zero(
        err,
        Some(vec![("farmyard".to_owned(), Some(1))]),
        &["Test Results", "dairy", "ok", "farmyard", "exit=1"],
        &["runner:vitest", "command:"],
    );
}

#[test]
fn run_manifest_task_builtin_test_json_failure_includes_results_and_failures() {
    let root = temp_workspace("builtin-test-fanout-failure-json");
    let (farmyard, dairy) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&farmyard, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&dairy, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &["--json"]);
    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            assert_eq!(failures, vec![("farmyard".to_owned(), Some(1))]);
            let parsed = parse_json_output(&rendered);
            assert_eq!(parsed["schema"], "effigy.test.results.v1");
            assert_eq!(parsed["failures"][0]["target"], "farmyard");
            let target_names = parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .filter_map(|entry| entry["target"].as_str())
                .collect::<Vec<&str>>();
            assert!(target_names.contains(&"dairy"));
            assert!(target_names.contains(&"farmyard"));
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

#[test]
fn run_manifest_task_builtin_test_text_and_json_outputs_share_target_identity() {
    let root = temp_workspace("builtin-test-json-text-target-parity");
    write_package_json_with_test_script(&root);
    let marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &marker);

    let text = run_builtin_ok(root.clone(), "test", &["--run"]);
    assert_contains_all(&text, &["Test Results", "root", "ok"]);

    let json = run_builtin_ok(root, "test", &["--json", "--run"]);
    let parsed = parse_json_output(&json);
    assert_eq!(parsed["schema"], "effigy.test.results.v1");
    assert_eq!(parsed["targets"][0]["target"], "root");
    assert_eq!(parsed["targets"][0]["success"], true);
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_include_runner_root_and_command() {
    let root = temp_workspace("builtin-test-verbose-results");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 0\n");

    let out = run_builtin_ok(root, "test", &["--verbose-results", "--run"]);
    assert_contains_all(
        &out,
        &[
            "Test Results",
            "runner:vitest",
            "root:",
            "command:vitest run '--run'",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_tui_flag_falls_back_to_text_when_non_interactive() {
    let root = temp_workspace("builtin-test-tui-fallback");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 0\n");

    let out = run_builtin_ok(root, "test", &["--tui"]);
    assert_contains_all(&out, &["Test Results", "root"]);
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_configured_package_manager() {
    let root = temp_workspace("builtin-test-plan-package-manager");
    write_js_package_manager_manifest(&root, "pnpm");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["pnpm exec vitest run", "package_manager.js=pnpm"]);
}

#[test]
fn run_manifest_task_builtin_test_exec_uses_configured_package_manager() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-exec-package-manager");
    write_js_package_manager_manifest(&root, "bun");
    write_package_json_with_vitest_dev_dependency(&root);

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let bun_stub = bin_dir.join("bun");
    let args_log = root.join("bun-args.log");
    write_executable(
        &bun_stub,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_BUN_ARGS_FILE\"\n",
    );

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_BUN_ARGS_FILE",
            Some(args_log.display().to_string()),
        ),
    ]);

    let out = run_builtin_ok(root, "test", &["vitest"]);
    assert_contains_all(&out, &["Test Results"]);
    let args = fs::read_to_string(args_log).expect("read bun args");
    assert_eq!(args, "x\nvitest\nrun\n");
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_runner_command_override() {
    let root = temp_workspace("builtin-test-plan-runner-override");
    write_root_manifest(
        &root,
        r#"[test.runners]
vitest = "pnpm exec vitest run --config vitest.config.ts"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "vitest"]);
    assert_contains_all(
        &out,
        &[
            "pnpm exec vitest run --config vitest.config.ts",
            "test.runners.vitest command override applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_runner_override_wins_over_package_manager() {
    let root = temp_workspace("builtin-test-plan-override-precedence");
    write_root_manifest(
        &root,
        r#"[package_manager]
js = "bun"

[test.runners]
vitest = "npx vitest run --reporter=dot"
"#,
    );
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan", "vitest"]);
    assert_contains_all(
        &out,
        &[
            "npx vitest run --reporter=dot",
            "package_manager.js=bun",
            "test.runners.vitest command override applied",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_prints_reference() {
    let root = temp_workspace("builtin-config");
    write_root_manifest(&root, "");

    let out = run_builtin_ok(root, "config", &[]);
    assert_contains_all(
        &out,
        &[
            "effigy.toml Reference",
            "[test.runners]",
            "[tasks]",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_has_blank_line_between_sections() {
    let root = temp_workspace("builtin-test-plan-section-spacing");
    write_package_json_with_vitest_dev_dependency(&root);

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["\n\nTarget Summary\n", "\n\nTarget: root\n"]);
}
