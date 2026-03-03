use super::super::prelude::*;

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
