use crate::runner::tests::prelude::cases::*;
use crate::runner::tests::prelude::harness::*;
use crate::runner::tests::prelude::json::*;
use crate::runner::tests::prelude::output::*;
use crate::runner::tests::prelude::setup_fanout_catalog_repo;
use std::fs;

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

    let out = run_builtin_ok(root.to_path_buf(), "test", &["--verbose-results"]);
    assert_output_contains_all(&out, &["Test Results", "runner:unit"]);
    assert_path_exists(&configured_marker, "configured suite marker");
    assert_path_missing(&vitest_marker, "auto-detected vitest marker");
}

#[test]
fn run_manifest_task_builtin_test_plans_and_runs_managed_suite_steps() {
    let root = temp_workspace("builtin-test-managed-suite-steps");
    let prepare_marker = root.join("prepare.log");
    let suite_marker = root.join("suite.log");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.prepare]
run = "printf prepared > {}"

[test.suites.composed]
run = [
  {{ task = "prepare" }},
  {{ run = "printf suite > {}" }},
]
"#,
            prepare_marker.display(),
            suite_marker.display()
        ),
    );

    let plan = run_builtin_ok(root.to_path_buf(), "test", &["--plan", "composed"]);
    assert_output_contains_all(&plan, &["Test Plan", "composed", "printf prepared"]);
    assert_path_missing(&prepare_marker, "planned prepare marker");
    assert_path_missing(&suite_marker, "planned suite marker");

    let out = run_builtin_ok(root, "test", &["composed"]);
    assert_output_contains_all(&out, &["Test Results", "root: ok"]);
    assert_path_exists(&prepare_marker, "executed prepare marker");
    assert_path_exists(&suite_marker, "executed suite marker");
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
    assert_output_contains_all(&out, &["Test Results"]);
    assert_path_exists(&unit_marker, "unit suite marker");
    assert_path_missing(&integration_marker, "integration suite marker");
}

#[test]
fn run_manifest_task_builtin_test_skips_on_demand_suites_by_default() {
    let root = temp_workspace("builtin-test-on-demand-suite");
    let unit_marker = root.join("unit-suite.log");
    let focused_marker = root.join("focused-suite.log");
    write_root_manifest(
        &root,
        &format!(
            r#"[test.suites.unit]
run = "sh -lc 'printf unit > \"{}\"'"

[test.suites.focused]
run = "sh -lc 'printf focused > \"{}\"'"
default = false
"#,
            unit_marker.display(),
            focused_marker.display()
        ),
    );

    let default_out = run_builtin_ok(root.to_path_buf(), "test", &[]);
    assert_output_contains_all(&default_out, &["Test Results", "root/unit: ok"]);
    assert_path_exists(&unit_marker, "default suite marker");
    assert_path_missing(&focused_marker, "on-demand suite marker");

    let focused_out = run_builtin_ok(root, "test", &["focused"]);
    assert_output_contains_all(&focused_out, &["Test Results", "root/focused: ok"]);
    assert_path_exists(&focused_marker, "selected on-demand suite marker");
}

#[test]
fn run_manifest_task_builtin_test_multi_suite_selector_errors_include_recovery_hints() {
    let cases = [
        BuiltinInvocationCase {
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
        BuiltinInvocationCase {
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

    assert_builtin_error_case_table_with_setup("test", &cases, setup_multi_suite_repo);
}

#[test]
fn run_manifest_task_builtin_test_supports_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector");
    setup_multi_suite_repo(&root);
    let vitest_marker = root.join("vitest-called.log");
    install_local_vitest_marker(&root, &vitest_marker);

    let out = run_builtin_ok(root.to_path_buf(), "test", &["vitest", "user-service"]);
    assert_output_contains_all(&out, &["Test Results", "root/vitest"]);
    assert_output_excludes_all(&out, &["root/cargo-"]);
    assert_path_exists(&vitest_marker, "vitest suite marker");
}

#[test]
fn run_manifest_task_builtin_test_treats_package_name_as_catalog_not_filter() {
    let root = temp_workspace("builtin-test-catalog-not-filter");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "vitest", "catalog_a"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let names = parsed["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .filter_map(|target| target["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["catalog_a"]);
    let commands = parsed["targets"][0]["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(
        commands
            .iter()
            .all(|command| !command.contains("catalog_a")),
        "package name was forwarded as a vitest filter: {commands:?}"
    );
    assert!(catalog_a.exists() && catalog_b.exists());
}

#[test]
fn run_manifest_task_builtin_test_suite_task_ref_keeps_container_run_in() {
    let root = temp_workspace("builtin-test-suite-task-ref-run-in");
    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    write_root_manifest(
        &root,
        r#"[catalog.members]
api = "api"

[test.suites.api]
run = [{ task = "api/test:unit" }]
"#,
    );
    write_manifest(
        &api.join("effigy.toml"),
        r#"[catalog]
alias = "api"

[tasks."test:unit"]
run = "cargo test --workspace --all-features"
run_in = "container"
"#,
    );

    let json = run_builtin_ok(root, "test", &["--plan", "--json", "api"]);
    let parsed = parse_json_output_with_schema(&json, "effigy.test.plan.v1");
    let command = parsed["targets"][0]["commands"][0]
        .as_str()
        .expect("command");
    assert!(
        command.contains("api/test:unit") || command.contains("test:unit"),
        "expected nested task invocation, got {command}"
    );
    assert!(
        !command.contains("cargo test --workspace --all-features"),
        "container task-ref was inlined onto the host: {command}"
    );
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
