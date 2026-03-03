use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_rejects_invalid_task_ref_syntax() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedTaskRefInvalidCase {
            workspace: "managed-compact-profile-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-trailing-escape",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = "test vitest \\" }]
"#,
            expected_reference: "test vitest \\",
            expected_detail: "trailing escape",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        let err = run_dev_with_repo(&root, &[]).expect_err("invalid process task ref should fail");
        assert_managed_task_reference_invalid(
            err,
            "dev",
            "tests",
            case.expected_reference,
            case.expected_detail,
        );
    }
}
