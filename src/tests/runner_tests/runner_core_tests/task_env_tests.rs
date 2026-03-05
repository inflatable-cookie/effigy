use super::prelude::*;

#[test]
fn run_manifest_task_applies_task_env_with_project_substitution() {
    let root = temp_workspace("task-env-project-substitution");
    let marker = root.join("task-env-paths.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.build]
run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'"
env = {{ CARGO_HOME = "{{project}}/.cargo/home", CARGO_TARGET_DIR = "{{repo}}/.cargo/target" }}
"#,
            marker.display()
        ),
    );

    let _env = EnvGuard::set_many(&[
        ("CARGO_HOME", Some("/tmp/global-cargo-home".to_owned())),
        (
            "CARGO_TARGET_DIR",
            Some("/tmp/global-cargo-target".to_owned()),
        ),
    ]);

    assert_run_task_ok_empty(&root, "build", &[]);

    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    let expected = format!(
        "{}/.cargo/home|{}/.cargo/target",
        canonical_root.display(),
        canonical_root.display()
    );
    assert_file_text_equals(&marker, &expected);
}

#[test]
fn run_manifest_task_supports_compact_inline_task_env_definition() {
    let root = temp_workspace("task-env-compact-inline-table");
    let marker = root.join("task-env-inline.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks]
build = {{ run = "sh -lc 'printf %s \"$CARGO_HOME\" > \"{}\"'", env = {{ CARGO_HOME = "{{project}}/.cargo/inline-home" }} }}
"#,
            marker.display()
        ),
    );

    assert_run_task_ok_empty(&root, "build", &[]);

    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    assert_file_text_equals(
        &marker,
        &format!("{}/.cargo/inline-home", canonical_root.display()),
    );
}
