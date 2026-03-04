use super::super::prelude::*;

#[test]
fn run_manifest_task_run_array_supports_relative_catalog_env_reference() {
    let root = temp_workspace("run-array-env-relative-catalog-ref");
    let sub_project1 = root.join("sub-project1");
    let sub_project2 = root.join("sub-project2");
    fs::create_dir_all(&sub_project1).expect("mkdir sub-project1");
    fs::create_dir_all(&sub_project2).expect("mkdir sub-project2");
    let marker = sub_project2.join("env-cross-catalog.out");

    write_manifest(
        &sub_project1.join("effigy.toml"),
        r#"[catalog]
alias = "project1"

[env]
MY_VAR = "my value"
"#,
    );
    write_manifest(
        &sub_project2.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "project2"

[tasks]
api = [
  {{ env = "../sub-project1/MY_VAR" }},
  {{ run = "sh -lc 'printf %s \"$MY_VAR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_task_output_equals(&root, "project2/api", &marker, "my value");
}

#[test]
fn run_manifest_task_run_array_supports_relative_catalog_dotenv_env_reference() {
    let root = temp_workspace("run-array-env-relative-catalog-dotenv-ref");
    let sub_project1 = root.join("sub-project1");
    let sub_project2 = root.join("sub-project2");
    fs::create_dir_all(&sub_project1).expect("mkdir sub-project1");
    fs::create_dir_all(&sub_project2).expect("mkdir sub-project2");
    let marker = sub_project2.join("env-cross-catalog-dotenv.out");

    write_manifest(
        &sub_project1.join("effigy.toml"),
        r#"[catalog]
alias = "project1"
"#,
    );
    fs::write(sub_project1.join(".env"), "MY_VAR=from-dotenv\n").expect("write project1 .env");
    write_manifest(
        &sub_project2.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "project2"

[tasks]
api = [
  {{ env = "../sub-project1/MY_VAR" }},
  {{ run = "sh -lc 'printf %s \"$MY_VAR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_task_output_equals(&root, "project2/api", &marker, "from-dotenv");
}

#[test]
fn run_manifest_task_run_array_errors_for_missing_relative_catalog_env_reference() {
    let root = temp_workspace("run-array-env-relative-catalog-ref-missing");
    let sub_project1 = root.join("sub-project1");
    let sub_project2 = root.join("sub-project2");
    fs::create_dir_all(&sub_project1).expect("mkdir sub-project1");
    fs::create_dir_all(&sub_project2).expect("mkdir sub-project2");

    write_manifest(
        &sub_project1.join("effigy.toml"),
        r#"[catalog]
alias = "project1"

[env]
MY_VAR = "my value"
"#,
    );
    write_manifest(
        &sub_project2.join("effigy.toml"),
        r#"[catalog]
alias = "project2"

[tasks]
api = [
  { env = "../sub-project1/MISSING_VAR" },
  { run = "printf unreachable" }
]
"#,
    );

    let err = run_builtin_err(root, "project2/api", &[]);
    assert_task_invocation_error_contains(
        err,
        &["unknown env entry `../sub-project1/MISSING_VAR`"],
    );
}
