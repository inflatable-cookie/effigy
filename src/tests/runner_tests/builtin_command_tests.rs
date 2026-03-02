use super::*;

#[test]
fn run_manifest_task_builtin_test_plan_renders_detection_summary() {
    let root = temp_workspace("builtin-test-plan");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

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
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.suites]
unit = "pnpm exec vitest run"
"#,
    );

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

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
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
    fs::write(
        dairy.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write dairy package");

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
    write_manifest(&root.join("effigy.toml"), &manifest);
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            vitest_marker.display()
        ),
    )
    .expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.suites]
unit = "true"
integration = "true"
"#,
    );

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
    write_manifest(&root.join("effigy.toml"), &manifest);

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
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            marker.display()
        ),
    )
    .expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let out = run_builtin_ok(root.clone(), "test", &["--run"]);
    assert_contains_all(&out, &["Test Results", "targets:", "root"]);
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(marker.exists(), "vitest stub should be invoked");
}

#[test]
fn run_manifest_task_builtin_test_json_suppresses_child_process_output() {
    let root = temp_workspace("builtin-test-json-suppresses-child-output");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(
        &vitest,
        "#!/bin/sh\nprintf noisy-stdout\nprintf noisy-stderr >&2\nexit 0\n",
    )
    .expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

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
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn smoke() {\n        assert!(super::ok());\n    }\n}\n",
    )
    .expect("write lib");

    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let vitest_marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            vitest_marker.display()
        ),
    )
    .expect("write vitest");
    let mut vitest_perms = fs::metadata(&vitest).expect("stat").permissions();
    vitest_perms.set_mode(0o755);
    fs::set_permissions(&vitest, vitest_perms).expect("chmod");

    let out = run_builtin_ok(root.clone(), "test", &[]);
    assert_contains_all(&out, &["Test Results", "root/vitest", "root/cargo-"]);
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_with_named_args_errors_when_multi_suite_is_ambiguous() {
    let root = temp_workspace("builtin-test-multi-suite-ambiguous");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let err = run_builtin_err(root, "test", &["user-service"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "ambiguous",
            "vitest",
            "cargo-",
            "Try one of:",
            "Use `effigy test --plan <args>`",
            "effigy test vitest user-service",
            "effigy test cargo-",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_with_named_args_in_multi_suite_returns_recovery_output() {
    let root = temp_workspace("builtin-test-multi-suite-plan-recovery");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let out = run_builtin_ok(root, "test", &["--plan", "user-service"]);
    assert_contains_all(
        &out,
        &[
            "Test Plan",
            "runtime: plan-recovery",
            "available-suites:",
            "ambiguous",
            "Try one of:",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_plan_json_recovery_has_versioned_schema() {
    let root = temp_workspace("builtin-test-plan-json-recovery-schema");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let out = run_builtin_ok(root, "test", &["--plan", "--json", "user-service"]);
    let parsed = parse_json_output(&out);
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["runtime"], "plan-recovery");
    assert!(parsed["recovery"].is_object());
}

#[test]
fn run_manifest_task_builtin_test_supports_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let vitest_marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            vitest_marker.display()
        ),
    )
    .expect("write vitest");
    let mut vitest_perms = fs::metadata(&vitest).expect("stat").permissions();
    vitest_perms.set_mode(0o755);
    fs::set_permissions(&vitest, vitest_perms).expect("chmod");

    let out = run_builtin_ok(root.clone(), "test", &["vitest", "user-service"]);
    assert_contains_all(&out, &["Test Results", "root/vitest"]);
    assert!(!out.contains("root/cargo-"));
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_plan_mistyped_suite_returns_recovery_output() {
    let root = temp_workspace("builtin-test-plan-mistyped-suite-recovery");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let out = run_builtin_ok(root, "test", &["--plan", "viteest", "user-service"]);
    assert_contains_all(
        &out,
        &[
            "Test Plan",
            "runtime: plan-recovery",
            "Did you mean `vitest`?",
            "Try: effigy test vitest user-service",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_test_errors_for_unavailable_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector-unavailable");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");

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
fn run_manifest_task_builtin_test_mistyped_suite_suggests_nearest_runner() {
    let root = temp_workspace("builtin-test-mistyped-suite-suggestion");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let err = run_builtin_err(root, "test", &["viteest", "user-service"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "runner `viteest` is not available",
            "Did you mean `vitest`?",
            "Try: effigy test vitest user-service",
            "Use `effigy test --plan <args>`",
        ],
    );
}

#[test]
fn run_manifest_task_explicit_test_task_overrides_builtin_auto_detection() {
    let root = temp_workspace("builtin-test-explicit-override");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.test]\nrun = \"printf explicit > explicit-test.log\"\n",
    );
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");

    let out = run_builtin_ok(root.clone(), "test", &[]);

    assert_eq!(out, "");
    assert!(
        root.join("explicit-test.log").exists(),
        "explicit task should run before builtin test detection"
    );
}

#[test]
fn run_manifest_task_builtin_test_falls_through_to_deferral_when_no_detection_matches() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-defers");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'test' && test {args} = '--watch'\"\n",
    );

    let out = run_builtin_ok(root, "test", &["--watch"]);

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_builtin_test_fans_out_across_catalog_roots() {
    let root = temp_workspace("builtin-test-fanout");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(
        &farmyard_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            farmyard_marker.display()
        ),
    )
    .expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(
        &dairy_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            dairy_marker.display()
        ),
    )
    .expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

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
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(
        &farmyard_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            farmyard_marker.display()
        ),
    )
    .expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(
        &dairy_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            dairy_marker.display()
        ),
    )
    .expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

    let out = run_builtin_ok(root, "farmyard/test", &[]);
    assert_contains_all(&out, &["Test Results", "farmyard"]);
    assert!(!out.contains("dairy"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(!dairy_marker.exists(), "dairy vitest should not run");
}

#[test]
fn run_manifest_task_builtin_test_failure_keeps_rendered_results_summary() {
    let root = temp_workspace("builtin-test-fanout-failure-summary");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(&farmyard_vitest, "#!/bin/sh\nexit 1\n").expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(&dairy_vitest, "#!/bin/sh\nexit 0\n").expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

    let err = run_builtin_err(root, "test", &[]);

    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            assert_eq!(failures, vec![("farmyard".to_owned(), Some(1))]);
            assert_contains_all(
                &rendered,
                &["Test Results", "dairy", "ok", "farmyard", "exit=1"],
            );
            assert!(!rendered.contains("runner:vitest"));
            assert!(!rendered.contains("command:"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_failure_with_suite_filter_shows_no_match_hint() {
    let root = temp_workspace("builtin-test-filtered-failure-hint");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let err = run_builtin_err(root, "test", &["vitest", "user-service"]);

    match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => {
            assert_contains_all(
                &rendered,
                &[
                    "Hint",
                    "often means no tests matched",
                    "vitest run 'user-service'",
                    "Try again without the filter",
                ],
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_include_runner_root_and_command() {
    let root = temp_workspace("builtin-test-verbose-results");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 0\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

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
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 0\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let out = run_builtin_ok(root, "test", &["--tui"]);
    assert_contains_all(&out, &["Test Results", "root"]);
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_configured_package_manager() {
    let root = temp_workspace("builtin-test-plan-package-manager");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "pnpm"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["pnpm exec vitest run", "package_manager.js=pnpm"]);
}

#[test]
fn run_manifest_task_builtin_test_exec_uses_configured_package_manager() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-test-exec-package-manager");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "bun"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let bun_stub = bin_dir.join("bun");
    let args_log = root.join("bun-args.log");
    fs::write(
        &bun_stub,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_BUN_ARGS_FILE\"\n",
    )
    .expect("write bun stub");
    let mut perms = fs::metadata(&bun_stub).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bun_stub, perms).expect("chmod");

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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.runners]
vitest = "pnpm exec vitest run --config vitest.config.ts"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "bun"

[test.runners]
vitest = "npx vitest run --reporter=dot"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

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
    write_manifest(&root.join("effigy.toml"), "");

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
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_builtin_ok(root, "test", &["--plan"]);
    assert_contains_all(&out, &["\n\nTarget Summary\n", "\n\nTarget: root\n"]);
}
