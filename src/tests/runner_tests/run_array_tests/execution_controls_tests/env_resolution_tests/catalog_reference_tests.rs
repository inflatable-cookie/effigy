use super::super::prelude::{
    assert_run_array_task_invocation_error_case_table,
    assert_run_array_task_output_case_table, write_catalog_api_single_env_capture_manifest,
    write_catalog_api_unreachable_manifest, write_catalog_manifest_with_alias, write_env_files,
    Path, RunArrayTaskInvocationErrorCase, RunArrayTaskOutputCase,
};

fn setup_relative_catalog_env_reference(root: &Path, marker: &Path) {
    write_catalog_manifest_with_alias(
        root,
        "sub-project1",
        "project1",
        r#"[env]
MY_VAR = "my value""#,
    );
    write_catalog_api_single_env_capture_manifest(
        root,
        "sub-project2",
        "project2",
        r#""../sub-project1/MY_VAR""#,
        "MY_VAR",
        marker,
    );
}

fn setup_relative_catalog_dotenv_env_reference(root: &Path, marker: &Path) {
    write_catalog_manifest_with_alias(root, "sub-project1", "project1", "");
    write_env_files(root, &[("sub-project1/.env", "MY_VAR=from-dotenv\n")]);
    write_catalog_api_single_env_capture_manifest(
        root,
        "sub-project2",
        "project2",
        r#""../sub-project1/MY_VAR""#,
        "MY_VAR",
        marker,
    );
}

fn setup_missing_relative_catalog_env_reference(root: &Path) {
    write_catalog_manifest_with_alias(
        root,
        "sub-project1",
        "project1",
        r#"[env]
MY_VAR = "my value""#,
    );
    write_catalog_api_unreachable_manifest(
        root,
        "sub-project2",
        "project2",
        r#""../sub-project1/MISSING_VAR""#,
    );
}

#[test]
fn run_manifest_task_run_array_relative_catalog_env_reference_contract_table() {
    let cases = [
        RunArrayTaskOutputCase {
            workspace: "run-array-env-relative-catalog-ref",
            task: "project2/api",
            marker_rel: "sub-project2/env-cross-catalog.out",
            expected: "my value",
            setup: setup_relative_catalog_env_reference,
        },
        RunArrayTaskOutputCase {
            workspace: "run-array-env-relative-catalog-dotenv-ref",
            task: "project2/api",
            marker_rel: "sub-project2/env-cross-catalog-dotenv.out",
            expected: "from-dotenv",
            setup: setup_relative_catalog_dotenv_env_reference,
        },
    ];

    assert_run_array_task_output_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_relative_catalog_env_reference_errors_are_stable() {
    let cases = [RunArrayTaskInvocationErrorCase {
        workspace: "run-array-env-relative-catalog-ref-missing",
        task: "project2/api",
        expected: &["unknown env entry `../sub-project1/MISSING_VAR`"],
        setup: setup_missing_relative_catalog_env_reference,
    }];

    assert_run_array_task_invocation_error_case_table(&cases);
}
