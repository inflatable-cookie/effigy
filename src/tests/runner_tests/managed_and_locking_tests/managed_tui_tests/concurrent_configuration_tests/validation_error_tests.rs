use crate::runner::tests::prelude::{
    assert_managed_invalid_definition_case_table, ManagedInvalidDefinitionCase,
};

#[test]
fn run_manifest_task_managed_tui_validation_error_contract_table() {
    let cases = [
        ManagedInvalidDefinitionCase {
            workspace: "managed-concurrent-invalid-entry",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", run = "printf oops", start = 1, tab = 1 }
]

[tasks.api]
run = "printf api"
"#,
            expected_task: "dev",
            expected_process: "api",
            expected_detail_substring: Some("either `task` or `run`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-tab-order-invalid",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "jobs" }]
"#,
            expected_task: "dev",
            expected_process: "jobs",
            expected_detail_substring: Some("missing both `task` and `run`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-invalid-process-def",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "printf api", task = "api" }]
"#,
            expected_task: "dev",
            expected_process: "api",
            expected_detail_substring: None,
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-shell-name-invalid",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "term", task = "shell" }]
"#,
            expected_task: "dev",
            expected_process: "term",
            expected_detail_substring: Some("task `shell` must use process name `shell`"),
        },
    ];

    assert_managed_invalid_definition_case_table(&cases);
}
