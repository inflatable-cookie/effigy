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

    assert_managed_task_ref_invalid_case_table(&cases);
}
