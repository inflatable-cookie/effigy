use crate::runner::tests::prelude::{
    assert_run_array_task_output_case_table, run_builtin_err, temp_workspace,
    write_env_files, write_root_api_single_env_capture_manifest, write_validate_manifest, Path,
    RunArrayTaskOutputCase, RunnerError,
};

fn setup_named_env_value_dotenv_fallback(root: &Path, marker: &Path) {
    write_root_api_single_env_capture_manifest(
        root,
        marker,
        None,
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(
        root,
        &[(
            ".env",
            "DATABASE_URL=postgres://postgres:postgres@localhost:5432/acowtancy\n",
        )],
    );
}

fn setup_manifest_env_over_dotenv_fallback(root: &Path, marker: &Path) {
    write_root_api_single_env_capture_manifest(
        root,
        marker,
        Some(
            r#"[env]
DATABASE_URL = "postgres://from-manifest""#,
        ),
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    write_env_files(root, &[(".env", "DATABASE_URL=postgres://from-dotenv\n")]);
}

fn setup_env_schema_value_fallback(root: &Path, marker: &Path) {
    write_root_api_single_env_capture_manifest(
        root,
        marker,
        None,
        r#""DATABASE_URL""#,
        "DATABASE_URL",
    );
    std::fs::write(
        root.join(".env.schema"),
        "DATABASE_URL=postgres://from-env-schema\n",
    )
    .expect("write env schema");
}

fn setup_unknown_env_profile(root: &Path) {
    write_validate_manifest(
        root,
        r#"[tasks.validate]
run = [
  { env = "missing-profile" },
  { run = "printf unreachable" }
]
"#,
    );
}

#[test]
fn run_manifest_task_run_array_env_resolution_fallback_contract_table() {
    let cases = [
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-dotenv-fallback",
            task: "api",
            marker_rel: "env-value-dotenv.out",
            expected: "postgres://postgres:postgres@localhost:5432/acowtancy",
            setup: setup_named_env_value_dotenv_fallback,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-manifest-precedence",
            task: "api",
            marker_rel: "env-value-manifest-precedence.out",
            expected: "postgres://from-manifest",
            setup: setup_manifest_env_over_dotenv_fallback,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-value-env-schema-fallback",
            task: "api",
            marker_rel: "env-value-env-schema-fallback.out",
            expected: "postgres://from-env-schema",
            setup: setup_env_schema_value_fallback,
        },
    ];

    assert_run_array_task_output_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_env_resolution_errors_for_unknown_profiles() {
    let root = temp_workspace("run-array-env-profile-missing");
    setup_unknown_env_profile(&root);
    let error = run_builtin_err(root, "validate", &[]);
    match error {
        RunnerError::TaskInvocation(message) => assert_eq!(
            message,
            "task `validate` run step references unknown env entry `missing-profile`"
        ),
        other => panic!("unexpected error: {other}"),
    }
}
