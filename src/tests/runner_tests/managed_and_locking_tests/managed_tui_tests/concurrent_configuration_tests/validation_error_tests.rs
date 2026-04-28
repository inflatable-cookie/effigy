use crate::runner::tests::prelude::{
    assert_managed_invalid_definition_case_table, ManagedInvalidDefinitionCase,
};

macro_rules! container_session_manifest {
    ($body:literal) => {
        concat!(
            "[tasks.dev]\n",
            "mode = \"tui\"\n",
            "workspace = \"app\"\n",
            $body,
            "\n[systems]\n",
            "default = \"dev\"\n\n",
            "[systems.dev]\n",
            "default_workspace = \"app\"\n\n",
            "[systems.dev.workspaces.app]\n",
            "container = \"web\"\n"
        )
    };
}

macro_rules! lifecycle_container_manifest {
    ($body:literal) => {
        concat!(
            "[tasks.dev]\n",
            "mode = \"tui\"\n",
            "workspace = \"app\"\n",
            "container_lifecycle = true\n",
            $body,
            "\n[systems]\n",
            "default = \"dev\"\n\n",
            "[systems.dev]\n",
            "default_workspace = \"app\"\n\n",
            "[systems.dev.workspaces.app]\n",
            "container = \"web\"\n"
        )
    };
}

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
            manifest: container_session_manifest!(
                r#"concurrent = [{ role = "lifecycle" }]
"#
            ),
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some(
                "requires `container_lifecycle = true` on `[tasks.<name>]`",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]
container_lifecycle = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some("requires a container-backed execution binding"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-rejects-run",
            manifest: lifecycle_container_manifest!(
                r#"concurrent = [{ role = "lifecycle", run = "printf nope" }]
"#
            ),
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
            expected_detail_substring: Some(
                "`role = \"shell\"` requires a container-backed execution binding",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-shell-rejects-run",
            manifest: container_session_manifest!(
                r#"concurrent = [{ role = "shell", run = "printf nope" }]
"#
            ),
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some(
                "owns the container shell in this batch; omit `run` and `task`",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-lifecycle-rejects-setup",
            manifest: lifecycle_container_manifest!(
                r#"concurrent = [{ role = "lifecycle", setup = [{ run = "printf nope" }] }]
"#
            ),
            expected_task: "dev",
            expected_process: "lifecycle",
            expected_detail_substring: Some(
                "`setup` is only supported on standard concurrent entries",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-health-wait-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]
container_lifecycle = true
health_wait = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some(
                "`role = \"lifecycle\"` requires a container-backed execution binding",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-health-wait-missing-lifecycle",
            manifest: container_session_manifest!(
                r#"concurrent = [{ name = "api", run = "printf api" }]
health_wait = true
"#
            ),
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some(
                "requires one `concurrent` entry with `role = \"lifecycle\"`",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-ready-message-requires-health-wait",
            manifest: lifecycle_container_manifest!(
                r#"concurrent = [{ role = "lifecycle" }]
ready_message = "http://project.test"
"#
            ),
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some("`ready_message` requires `health_wait = true`"),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-gateway-missing-container-session",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ role = "lifecycle" }]
container_lifecycle = true
gateway = true
"#,
            expected_task: "dev",
            expected_process: "process",
            expected_detail_substring: Some(
                "`role = \"lifecycle\"` requires a container-backed execution binding",
            ),
        },
        ManagedInvalidDefinitionCase {
            workspace: "managed-gateway-missing-lifecycle",
            manifest: container_session_manifest!(
                r#"concurrent = [{ name = "api", run = "printf api" }]
gateway = true
"#
            ),
            expected_task: "dev",
            expected_process: "managed",
            expected_detail_substring: Some(
                "`gateway = true` requires one `concurrent` entry with `role = \"lifecycle\"`",
            ),
        },
    ];

    assert_managed_invalid_definition_case_table(&cases);
}
