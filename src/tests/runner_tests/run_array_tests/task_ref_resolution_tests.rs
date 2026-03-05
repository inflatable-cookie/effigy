use super::prelude::*;

fn setup_task_ref_inline_args(root: &Path, marker: &Path) {
    write_capture_task_ref_validate_manifest(
        root,
        marker,
        r#"sh -lc 'printf %s "$1" > "__MARKER__"' sh {{args}}"#,
        None,
        r#""capture hello-world""#,
    );
}

fn setup_task_ref_quoted_inline_args(root: &Path, marker: &Path) {
    write_capture_task_ref_validate_manifest(
        root,
        marker,
        r#"sh -lc 'printf "%s|%s" "$1" "$2" > "__MARKER__"' sh {{args}}"#,
        None,
        r#"'capture alpha "two words"'"#,
    );
}

fn setup_prefixed_builtin_test_task_ref(root: &Path, marker: &Path) {
    write_validate_manifest(
        root,
        r#"[tasks.validate]
run = [{ task = "farmyard/test" }, "printf validate-ok"]
"#,
    );
    write_catalog_builtin_test_suite_manifest(root, "farmyard", "farmyard", "unit", marker);
}

fn setup_task_ref_referenced_task_env(root: &Path, marker: &Path) {
    write_capture_task_ref_validate_manifest(
        root,
        marker,
        r#"sh -lc 'printf %s "$CARGO_HOME" > "__MARKER__"'"#,
        Some(r#"CARGO_HOME = "{{project}}/.cargo/referenced-home""#),
        r#""capture""#,
    );
}

fn expected_referenced_task_env(root: &Path) -> String {
    let canonical_root = fs::canonicalize(root).expect("canonicalize root");
    format!("{}/.cargo/referenced-home", canonical_root.display())
}

fn setup_env_directive_applies_to_task_ref(root: &Path, marker: &Path) {
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
CARGO_HOME = "{{project}}/.cargo/from-directive"

[tasks.capture]
run = "sh -lc 'printf %s \"$CARGO_HOME\" > \"{}\"'"

[tasks.api]
run = [
  {{ env = "CARGO_HOME" }},
  {{ task = "capture" }}
]
"#,
            marker.display()
        ),
    );
}

fn expected_env_directive_path(root: &Path) -> String {
    let canonical_root = fs::canonicalize(root).expect("canonicalize root");
    format!("{}/.cargo/from-directive", canonical_root.display())
}

#[test]
fn run_manifest_task_run_array_task_reference_output_contract_table() {
    let cases = [
        RunArrayTaskOutputCase {
            workspace: "run-array-task-ref-inline-args",
            task: "validate",
            marker_rel: "task-ref-inline-args.log",
            expected: "hello-world",
            setup: setup_task_ref_inline_args,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-task-ref-quoted-inline-args",
            task: "validate",
            marker_rel: "task-ref-quoted-inline-args.log",
            expected: "alpha|two words",
            setup: setup_task_ref_quoted_inline_args,
        },
    ];

    assert_run_array_task_output_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_task_reference_rejects_invalid_inline_args() {
    let cases = [
        RunArrayTaskRefParseErrorCase {
            workspace: "run-array-task-ref-unterminated-quote",
            manifest: "[tasks.validate]\nrun = [{ task = 'test \"unterminated' }]\n",
            expected_tail: "unterminated quote",
        },
        RunArrayTaskRefParseErrorCase {
            workspace: "run-array-task-ref-trailing-escape",
            manifest: "[tasks.validate]\nrun = [{ task = \"test vitest \\\\\" }]\n",
            expected_tail: "trailing escape",
        },
    ];

    assert_run_array_validate_task_ref_parse_error_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_supports_builtin_test_task_reference_steps() {
    let cases = [
        BuiltinTestTaskRefCase {
            workspace: "run-array-builtin-test-task-ref",
            suite_name: "unit",
            task_ref: "test",
        },
        BuiltinTestTaskRefCase {
            workspace: "run-array-builtin-test-task-ref-inline-suite",
            suite_name: "vitest",
            task_ref: "test vitest",
        },
    ];

    assert_run_array_builtin_test_task_ref_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_supports_prefixed_builtin_test_task_reference_steps() {
    let cases = [RunArrayValidateMarkerCase {
        workspace: "run-array-prefixed-builtin-test-task-ref",
        args: &["--verbose-root"],
        marker_rel: "farmyard/builtin-test-called.log",
        expected: &["validate-ok"],
        setup: setup_prefixed_builtin_test_task_ref,
    }];

    assert_run_array_validate_marker_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_task_reference_env_contract_table() {
    let cases = [
        RunArrayTaskOutputDerivedCase {
            workspace: "run-array-task-ref-task-env",
            task: "validate",
            marker_rel: "task-ref-task-env.log",
            expected: expected_referenced_task_env,
            setup: setup_task_ref_referenced_task_env,
        },
        RunArrayTaskOutputDerivedCase {
            workspace: "run-array-env-directive-task-ref",
            task: "api",
            marker_rel: "task-ref-env-directive.log",
            expected: expected_env_directive_path,
            setup: setup_env_directive_applies_to_task_ref,
        },
    ];

    assert_run_array_task_output_derived_case_table(&cases);
}
