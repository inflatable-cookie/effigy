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
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-missing-managed-flag",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ role = "lifecycle" }]
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("requires `[tasks.<name>.managed] container_lifecycle = true`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]

[tasks.dev.managed]
container_lifecycle = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("requires `container_session = \"<name>\"`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-rejects-run",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ role = "lifecycle", run = "printf nope" }]

[tasks.dev.managed]
container_lifecycle = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("omit `run` and `task`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-shell-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "shell" }]
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("`role = \"shell\"` requires `container_session = \"<name>\"`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-shell-rejects-run",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ role = "shell", run = "printf nope" }]
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("owns the container shell in this batch; omit `run` and `task`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-health-wait-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]

[tasks.dev.managed]
container_lifecycle = true
health_wait = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("`role = \"lifecycle\"` requires `container_session = \"<name>\"`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-health-wait-missing-lifecycle",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ name = "api", run = "printf api" }]

[tasks.dev.managed]
health_wait = true
"#,
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some("requires one `concurrent` entry with `role = \"lifecycle\"`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-ready-message-requires-health-wait",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ role = "lifecycle" }]

[tasks.dev.managed]
container_lifecycle = true
ready_message = "http://project.test"
"#,
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some("`managed.ready_message` requires `managed.health_wait = true`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-gateway-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]

[tasks.dev.managed]
container_lifecycle = true
gateway = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("`role = \"lifecycle\"` requires `container_session = \"<name>\"`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-gateway-missing-lifecycle",
            manifest: r#"[tasks.dev]
mode = "tui"
container_session = "web"
concurrent = [{ name = "api", run = "printf api" }]

[tasks.dev.managed]
gateway = true
"#,
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some("`managed.gateway = true` requires one `concurrent` entry with `role = \"lifecycle\"`"),
        },
    ];

    assert_managed_invalid_definition_case_table(&cases);
}
