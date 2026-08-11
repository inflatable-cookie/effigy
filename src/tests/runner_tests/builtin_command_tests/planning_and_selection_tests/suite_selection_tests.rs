use crate::runner::tests::prelude::cases::*;
use crate::runner::tests::prelude::harness::*;
use crate::runner::tests::prelude::output::*;

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
