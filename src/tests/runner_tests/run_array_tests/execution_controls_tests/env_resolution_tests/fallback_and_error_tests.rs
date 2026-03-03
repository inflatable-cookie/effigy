use super::super::prelude::*;

#[test]
fn run_manifest_task_run_array_supports_dotenv_fallback_for_named_env_value_directive() {
    let root = temp_workspace("run-array-env-value-dotenv-fallback");
    let marker = root.join("env-value-dotenv.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
api = [
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env"),
        "DATABASE_URL=postgres://postgres:postgres@localhost:5432/acowtancy\n",
    )
    .expect("write .env");

    assert_task_output_equals(
        &root,
        "api",
        &marker,
        "postgres://postgres:postgres@localhost:5432/acowtancy",
    );
}

#[test]
fn run_manifest_task_run_array_prefers_manifest_env_over_dotenv_fallback() {
    let root = temp_workspace("run-array-env-value-manifest-precedence");
    let marker = root.join("env-value-manifest-precedence.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
DATABASE_URL = "postgres://from-manifest"

[tasks]
api = [
  {{ env = "DATABASE_URL" }},
  {{ run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );
    fs::write(root.join(".env"), "DATABASE_URL=postgres://from-dotenv\n").expect("write .env");

    assert_task_output_equals(&root, "api", &marker, "postgres://from-manifest");
}

#[test]
fn run_manifest_task_run_array_errors_for_unknown_env_profile() {
    let root = temp_workspace("run-array-env-profile-missing");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { env = "missing-profile" },
  { run = "printf unreachable" }
]
"#,
    );
    let err = run_validate_err(&root, &[]);
    assert_task_invocation_error_contains(err, &["unknown env entry `missing-profile`"]);
}
