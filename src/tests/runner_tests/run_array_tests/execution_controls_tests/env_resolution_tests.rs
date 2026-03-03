use super::prelude::*;

#[test]
fn run_manifest_task_run_array_supports_compact_env_directive_entry() {
    let root = temp_workspace("run-array-compact-env-directive-entry");
    let marker = root.join("compact-env.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
api = [
  {{ env = {{ CARGO_HOME = "{{project}}/.cargo/home", CARGO_TARGET_DIR = "{{project}}/.cargo/target" }} }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_task_output_equals(&root, "api", &marker, &expected_cargo_paths(&root));
}

#[test]
fn run_manifest_task_run_array_supports_named_env_profile_directive() {
    let root = temp_workspace("run-array-env-profile-directive-entry");
    let marker = root.join("env-profile.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
cargo = [
  {{ CARGO_HOME = "{{project}}/.cargo/home" }},
  {{ CARGO_TARGET_DIR = "{{project}}/.cargo/target" }}
]

[tasks]
api = [
  {{ env = "cargo" }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_task_output_equals(&root, "api", &marker, &expected_cargo_paths(&root));
}

#[test]
fn run_manifest_task_run_array_supports_named_env_value_directive() {
    let root = temp_workspace("run-array-env-value-directive-entry");
    let marker = root.join("env-value.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
CARGO_HOME = "{{project}}/.cargo/home"
CARGO_TARGET_DIR = "{{project}}/.cargo/target"

[tasks]
api = [
  {{ env = "CARGO_HOME" }},
  {{ env = "CARGO_TARGET_DIR" }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_task_output_equals(&root, "api", &marker, &expected_cargo_paths(&root));
}

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
    assert_task_invocation_error_contains(err, &["unknown env entry `../sub-project1/MISSING_VAR`"]);
}
