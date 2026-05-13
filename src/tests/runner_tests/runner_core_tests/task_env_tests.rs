use crate::runner::tests::prelude::{
    assert_file_text_equals, assert_run_task_ok_empty, assert_task_invocation_error_contains, fs,
    parse_json_output_with_schema_version, run_task, temp_workspace, write_root_manifest, EnvGuard,
};
use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};

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

#[test]
fn run_manifest_task_injects_declared_vault_secret_into_env() {
    let root = temp_workspace("task-vault-secret-env");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.capture]
run = "printf %s \"$DATABASE_URL\""
"#,
    );
    write_test_vault(
        &root,
        "vault-passphrase",
        &[("database_url", "postgres://secret-value")],
    );
    let _env = EnvGuard::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        Some("vault-passphrase".to_owned()),
    )]);

    let out = run_task(&root, "capture", &["--json"]).expect("task should succeed");
    let parsed = parse_json_output_with_schema_version(&out, "effigy.task.run.v1", 1);

    assert_eq!(parsed["stdout"].as_str(), Some("[REDACTED]"));
    assert!(
        !out.contains("postgres://secret-value"),
        "task JSON leaked vault secret: {out}"
    );
}

#[test]
fn run_manifest_task_blocks_missing_required_vault_secret_before_spawn() {
    let root = temp_workspace("task-vault-secret-missing");
    let marker = root.join("should-not-run.out");
    write_root_manifest(
        &root,
        &format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf %s \"$DATABASE_URL\" > \"{}\"'"
"#,
            marker.display()
        ),
    );
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = EnvGuard::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        Some("vault-passphrase".to_owned()),
    )]);

    let err = run_task(&root, "capture", &[]).expect_err("task should fail");
    assert_task_invocation_error_contains(
        err,
        &[
            "required task secret(s) missing from the vault",
            "database_url",
        ],
    );
    assert!(
        !marker.exists(),
        "task should not execute when a required vault secret is missing"
    );
}

#[test]
fn run_manifest_task_required_secrets_generate_missing_vault_before_spawn() {
    let root = temp_workspace("task-vault-secret-generate-missing-vault");
    let marker = root.join("generated-secret.out");
    write_root_manifest(
        &root,
        &format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"
generate = {{ task = "secrets:generate-dev" }}

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf %s \"$API_TOKEN\" > \"{}\"'"
secrets = "required"

[tasks."secrets:generate-dev"]
run = [
  {{ task = "secrets init" }},
  {{ task = "secrets set api_token" }},
]
"#,
            marker.display()
        ),
    );
    let _env = EnvGuard::set_many(&[
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            Some("vault-passphrase".to_owned()),
        ),
        (
            "EFFIGY_TEST_SECRETS_VALUE",
            Some("tok_generated".to_owned()),
        ),
    ]);

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "tok_generated");
}

#[test]
fn run_manifest_task_required_secrets_generate_missing_key_in_existing_vault() {
    let root = temp_workspace("task-vault-secret-generate-missing-key");
    let marker = root.join("generated-secret-existing-vault.out");
    write_root_manifest(
        &root,
        &format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"
generate = {{ task = "secrets:generate-dev" }}

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf %s \"$API_TOKEN\" > \"{}\"'"
secrets = "required"

[tasks."secrets:generate-dev"]
run = {{ task = "secrets set api_token" }}
"#,
            marker.display()
        ),
    );
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = EnvGuard::set_many(&[
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            Some("vault-passphrase".to_owned()),
        ),
        (
            "EFFIGY_TEST_SECRETS_VALUE",
            Some("tok_generated".to_owned()),
        ),
    ]);

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "tok_generated");
}

#[test]
fn run_manifest_task_skips_unreferenced_required_vault_secret_for_shell_task() {
    let root = temp_workspace("task-vault-secret-unreferenced");
    let marker = root.join("unrelated-task.out");
    write_root_manifest(
        &root,
        &format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf ok > \"{}\"'"
"#,
            marker.display()
        ),
    );

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "ok");
}

#[test]
fn run_manifest_rhai_task_does_not_preload_task_secret_env() {
    let root = temp_workspace("task-vault-secret-rhai-no-preload");
    let marker = root.join("rhai-ran.out");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("scripts/write-marker.rhai"),
        format!(r#"fs::write_file("{}", "ran");"#, marker.display()),
    )
    .expect("write rhai script");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.capture]
run = [{ rhai = "scripts/write-marker.rhai" }]
"#,
    );

    assert_run_task_ok_empty(&root, "capture", &[]);
    assert_file_text_equals(&marker, "ran");
}

fn write_test_vault(root: &std::path::Path, passphrase: &str, records: &[(&str, &str)]) {
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let envelope = payload
        .encrypt_with_passphrase(passphrase)
        .expect("encrypt test vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        vault_path,
        envelope.to_json_pretty().expect("serialize test vault"),
    )
    .expect("write test vault");
}
