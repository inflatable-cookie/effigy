use super::prelude::{
    assert_run_array_task_invocation_error_case_table, assert_run_array_task_output_case_table,
    write_env_files, write_task_api_env_capture_manifest, write_task_api_env_unreachable_manifest,
    Path, RunArrayTaskInvocationErrorCase, RunArrayTaskOutputCase,
};

fn setup_task_env_file_dotenv_fallback(root: &Path, marker: &Path) {
    write_task_api_env_capture_manifest(
        root,
        marker,
        Some(r#"".env.test""#),
        None,
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[
            (".env", "DATABASE_URL=postgres://from-default\n"),
            (".env.test", "DATABASE_URL=postgres://from-test\n"),
        ],
    );
}

fn setup_env_file_directive_dotenv_fallback(root: &Path, marker: &Path) {
    write_task_api_env_capture_manifest(
        root,
        marker,
        None,
        Some(r#"".env.test""#),
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[
            (".env", "DATABASE_URL=postgres://from-default\n"),
            (".env.test", "DATABASE_URL=postgres://from-test\n"),
        ],
    );
}

fn setup_directive_overrides_task_env_file(root: &Path, marker: &Path) {
    write_task_api_env_capture_manifest(
        root,
        marker,
        Some(r#"".env.test""#),
        Some(r#"".env.local""#),
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[
            (".env.test", "DATABASE_URL=postgres://from-test\n"),
            (".env.local", "DATABASE_URL=postgres://from-local\n"),
        ],
    );
}

fn setup_task_env_file_array_precedence(root: &Path, marker: &Path) {
    write_task_api_env_capture_manifest(
        root,
        marker,
        Some(r#"[".env.local", ".env.test"]"#),
        None,
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[
            (".env.test", "DATABASE_URL=postgres://from-test\n"),
            (".env.local", "DATABASE_URL=postgres://from-local\n"),
        ],
    );
}

fn setup_env_file_directive_array_precedence(root: &Path, marker: &Path) {
    write_task_api_env_capture_manifest(
        root,
        marker,
        None,
        Some(r#"[".env.local", ".env.test"]"#),
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[
            (".env.test", "DATABASE_URL=postgres://from-test\n"),
            (".env.local", "DATABASE_URL=postgres://from-local\n"),
        ],
    );
}

fn setup_task_env_file_empty_entry(root: &Path) {
    write_task_api_env_unreachable_manifest(
        root,
        Some(r#"[".env.test", "   "]"#),
        None,
        r#""DATABASE_URL""#,
    );
}

#[test]
fn run_manifest_task_run_array_env_file_precedence_contract_table() {
    let cases = [
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-task-env-file",
            task: "api",
            marker_rel: "env-value-task-env-file.out",
            expected: "postgres://from-test",
            setup: setup_task_env_file_dotenv_fallback,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-step-env-file",
            task: "api",
            marker_rel: "env-value-step-env-file.out",
            expected: "postgres://from-test",
            setup: setup_env_file_directive_dotenv_fallback,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-step-env-file-overrides-task",
            task: "api",
            marker_rel: "env-value-step-env-file-overrides-task.out",
            expected: "postgres://from-local",
            setup: setup_directive_overrides_task_env_file,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-task-env-file-array",
            task: "api",
            marker_rel: "env-value-task-env-file-array.out",
            expected: "postgres://from-local",
            setup: setup_task_env_file_array_precedence,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-step-env-file-array",
            task: "api",
            marker_rel: "env-value-step-env-file-array.out",
            expected: "postgres://from-local",
            setup: setup_env_file_directive_array_precedence,
        },
    ];

    assert_run_array_task_output_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_env_file_validation_errors_are_stable() {
    let cases = [RunArrayTaskInvocationErrorCase {
        workspace: "run-array-env-value-task-env-file-array-empty-entry",
        task: "api",
        expected: &["task env_file[1] is invalid"],
        setup: setup_task_env_file_empty_entry,
    }];

    assert_run_array_task_invocation_error_case_table(&cases);
}
