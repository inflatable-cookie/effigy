use super::prelude::*;

#[test]
fn run_manifest_task_run_array_task_reference_supports_inline_args() {
    let root = temp_workspace("run-array-task-ref-inline-args");
    let marker = root.join("task-ref-inline-args.log");
    write_validate_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf %s \"$1\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = "capture hello-world" }}]
"#,
            marker.display()
        ),
    );

    assert_validate_ok_empty(&root, &[]);
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "hello-world");
}

#[test]
fn run_manifest_task_run_array_task_reference_supports_quoted_inline_args() {
    let root = temp_workspace("run-array-task-ref-quoted-inline-args");
    let marker = root.join("task-ref-quoted-inline-args.log");
    write_validate_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf \"%s|%s\" \"$1\" \"$2\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = 'capture alpha "two words"' }}]
"#,
            marker.display()
        ),
    );

    assert_validate_ok_empty(&root, &[]);
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "alpha|two words");
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

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_validate_manifest(&root, case.manifest);
        let err = run_validate_err(&root, &[]);
        assert_task_invocation_error_contains(err, &["run step task ref", case.expected_tail]);
    }
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

    for case in cases {
        let root = temp_workspace(case.workspace);
        let marker = root.join("builtin-test-called.log");
        write_validate_manifest(
            &root,
            &format!(
                "[test.suites]\n{} = \"sh -lc 'printf called > \\\"{}\\\"'\"\n\n[tasks.validate]\nrun = [{{ task = \"{}\" }}, \"printf validate-ok\"]\n",
                case.suite_name,
                marker.display(),
                case.task_ref
            ),
        );

        let out = run_validate_ok(&root, &["--verbose-root"]);
        assert_contains_all(&out, &["validate-ok"]);
        assert!(marker.exists(), "built-in test task ref should execute");
    }
}

#[test]
fn run_manifest_task_run_array_supports_prefixed_builtin_test_task_reference_steps() {
    let root = temp_workspace("run-array-prefixed-builtin-test-task-ref");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    let marker = farmyard.join("builtin-test-called.log");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [{ task = "farmyard/test" }, "printf validate-ok"]
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "farmyard"
[test.suites]
unit = "sh -lc 'printf called > \"{}\"'"
"#,
            marker.display()
        ),
    );

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(&out, &["validate-ok"]);
    assert!(
        marker.exists(),
        "prefixed built-in test task ref should execute"
    );
}
