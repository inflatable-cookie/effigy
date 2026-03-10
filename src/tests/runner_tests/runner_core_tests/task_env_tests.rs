use super::prelude::{
    assert_file_text_equals, assert_run_task_ok_empty, assert_task_invocation_error_contains, fs,
    run_task, temp_workspace, write_root_manifest, EnvGuard,
};

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

#[test]
fn run_manifest_task_env_schema_override_uses_explicit_path_over_default_schema() {
    let root = temp_workspace("task-env-schema-override");
    let marker = root.join("env-schema.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf %s \"$API_URL\" > \"{}\"'"
"#,
            marker.display()
        ),
    );
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    fs::write(
        root.join(".env.schema"),
        "API_URL=https://default.example.test\n",
    )
    .expect("write default schema");
    fs::write(
        root.join("config/custom.env.schema"),
        "API_URL=https://override.example.test\n",
    )
    .expect("write override schema");

    assert_run_task_ok_empty(
        &root,
        "capture",
        &["--env-schema", "config/custom.env.schema"],
    );
    assert_file_text_equals(&marker, "https://override.example.test");
}

#[test]
fn run_manifest_task_env_schema_override_reports_missing_file() {
    let root = temp_workspace("task-env-schema-override-missing");
    write_root_manifest(
        &root,
        r#"[tasks.capture]
run = "printf should-not-run"
"#,
    );

    let err = run_task(
        &root,
        "capture",
        &["--env-schema", "config/missing.env.schema"],
    )
    .expect_err("task should fail");
    assert_task_invocation_error_contains(
        err,
        &["env schema file not found", "missing.env.schema"],
    );
}

#[test]
fn run_manifest_task_env_schema_pattern_validation_blocks_execution() {
    let root = temp_workspace("task-env-schema-pattern-validation");
    let marker = root.join("should-not-run.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf ran > \"{}\"'"
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env.schema"),
        "# @pattern=^https://[a-z]+\\.example\\.test$\nAPI_URL=http://service.example.test\n",
    )
    .expect("write env schema");

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("env schema validation failed"),
        "got: {rendered}"
    );
    assert!(rendered.contains("API_URL"), "got: {rendered}");
    assert!(rendered.contains("expected pattern /"), "got: {rendered}");
    assert!(
        !marker.exists(),
        "task should not execute when schema is invalid"
    );
}

#[test]
fn run_manifest_task_env_schema_sensitive_validation_redacts_value() {
    let root = temp_workspace("task-env-schema-sensitive-validation-redaction");
    let marker = root.join("should-not-run-sensitive.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf ran > \"{}\"'"
"#,
            marker.display()
        ),
    );
    fs::write(
        root.join(".env.schema"),
        "# @sensitive @pattern=^tok_[a-z0-9]+$\nAPI_TOKEN=super-secret-token\n",
    )
    .expect("write env schema");

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    let rendered = err.to_string();
    assert!(
        rendered.contains("env schema validation failed"),
        "got: {rendered}"
    );
    assert!(rendered.contains("API_TOKEN"), "got: {rendered}");
    assert!(rendered.contains("[REDACTED]"), "got: {rendered}");
    assert!(
        !rendered.contains("super-secret-token"),
        "sensitive value leaked: {rendered}"
    );
    assert!(
        !marker.exists(),
        "task should not execute when schema is invalid"
    );
}

#[test]
fn run_manifest_task_env_schema_config_enabled_false_skips_schema_loading() {
    let root = temp_workspace("task-env-schema-config-enabled-false");
    let marker = root.join("env-schema-disabled.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[env_schema]
enabled = false

[tasks.capture]
run = "sh -lc 'printf skipped > \"{}\"'"
"#,
            marker.display()
        ),
    );
    fs::write(root.join(".env.schema"), "BROKEN\n").expect("write invalid env schema");

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "skipped");
}

#[test]
fn run_manifest_task_env_schema_config_enabled_true_requires_schema_file() {
    let root = temp_workspace("task-env-schema-config-enabled-true");
    write_root_manifest(
        &root,
        r#"[env_schema]
enabled = true

[tasks.capture]
run = "printf should-not-run"
"#,
    );

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    assert_task_invocation_error_contains(err, &["env schema file not found", ".env.schema"]);
}

#[test]
fn run_manifest_task_env_schema_config_schema_uses_manifest_override() {
    let root = temp_workspace("task-env-schema-config-custom-schema");
    let marker = root.join("env-schema-config-custom.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[env_schema]
schema = "config/custom.env.schema"

[tasks.capture]
run = "sh -lc 'printf %s \"$API_URL\" > \"{}\"'"
"#,
            marker.display()
        ),
    );
    fs::create_dir_all(root.join("config")).expect("mkdir config");
    fs::write(
        root.join(".env.schema"),
        "API_URL=https://default-config.example.test\n",
    )
    .expect("write default env schema");
    fs::write(
        root.join("config/custom.env.schema"),
        "API_URL=https://custom-config.example.test\n",
    )
    .expect("write custom env schema");

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "https://custom-config.example.test");
}

#[test]
fn run_manifest_task_env_schema_config_exec_timeout_rejects_zero() {
    let root = temp_workspace("task-env-schema-config-timeout-zero");
    write_root_manifest(
        &root,
        r#"[env_schema]
exec_timeout = 0

[tasks.capture]
run = "printf should-not-run"
"#,
    );
    fs::write(root.join(".env.schema"), "API_URL=https://example.test\n").expect("write schema");

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    assert_task_invocation_error_contains(
        err,
        &["invalid `[env_schema].exec_timeout`", "at least 1 second"],
    );
}

#[test]
fn run_manifest_task_env_schema_config_schema_rejects_empty_value() {
    let root = temp_workspace("task-env-schema-config-empty-schema");
    write_root_manifest(
        &root,
        r#"[env_schema]
schema = "   "

[tasks.capture]
run = "printf should-not-run"
"#,
    );

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    assert_task_invocation_error_contains(
        err,
        &["invalid `[env_schema].schema`", "cannot be empty"],
    );
}
