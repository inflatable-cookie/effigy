use super::prelude::*;

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
