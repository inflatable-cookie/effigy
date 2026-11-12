use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME;
use crate::runner::tests::prelude::{
    assert_file_text_equals, assert_output_equals, assert_run_task_ok_empty,
    assert_task_invocation_error_contains, create_workspace_dir, fs,
    parse_json_output_with_schema_version, run_task, run_task_in_workspace, temp_workspace,
    write_executable, write_manifest, write_root_manifest, EnvGuard,
};
use effigy_secrets::{
    local_dev_unlock_key_path, LocalDevUnlockKey, SecretValue, VaultPlaintextPayload,
    VaultSecretRecord,
};
use std::path::{Path, PathBuf};

/// A task child starts with the user's environment plus Effigy's documented
/// variables: an internal marker this process was started with never rides
/// along, while the one marker a consumer reads does (effigy#341).
#[test]
fn run_manifest_task_environment_carries_no_internal_marker() {
    let root = temp_workspace("task-env-no-internal-marker");
    let marker = root.join("task-env-internal.out");
    write_root_manifest(
        &root,
        &format!(
            r#"[tasks.dump]
run = "sh -lc 'env > \"{}\"'"
"#,
            marker.display()
        ),
    );
    let _env = EnvGuard::set_many(&[
        ("EFFIGY_INTERNAL_SUPPRESS_HEADER", Some("1".to_owned())),
        ("EFFIGY_INTERNAL_ENVIRONMENT_HOLDER", Some("1".to_owned())),
        ("EFFIGY_MANAGED_RUN_TOKEN", Some("token".to_owned())),
        ("EFFIGY_SECRETS_REQUIRED", Some("1".to_owned())),
        (CONTAINER_HANDOFF_ENV_NAME, Some("1".to_owned())),
    ]);

    assert_run_task_ok_empty(&root, "dump", &[]);

    let dumped = fs::read_to_string(&marker).expect("dumped environment");
    for key in [
        "EFFIGY_INTERNAL_SUPPRESS_HEADER",
        "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER",
        "EFFIGY_MANAGED_RUN_TOKEN",
        "EFFIGY_SECRETS_REQUIRED",
    ] {
        assert!(
            !dumped.contains(&format!("{key}=")),
            "`{key}` reached a workload environment"
        );
    }
    assert!(
        dumped.contains(&format!("{CONTAINER_HANDOFF_ENV_NAME}=")),
        "a consumer reads the container handoff marker from its own workload"
    );
}

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
fn run_dev_injects_vault_secrets_without_a_passphrase() {
    let root = temp_workspace("dev-vault-secret-local-unlock");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.dev]
run = "printf %s \"$DATABASE_URL\""
"#,
    );
    write_test_vault_with_local_dev_unlock(
        &root,
        "vault-passphrase",
        &[("database_url", "postgres://secret-value")],
    );
    let _env = EnvGuard::set_many(&[
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
    ]);

    let out = run_task(&root, "dev", &["--json"]).expect("dev should succeed");
    let parsed = parse_json_output_with_schema_version(&out, "effigy.task.run.v1", 1);

    assert_eq!(parsed["stdout"].as_str(), Some("[REDACTED]"));
    assert!(!out.contains("postgres://secret-value"));
}

#[test]
#[cfg(unix)]
fn run_dev_surfaces_an_unusable_local_dev_key_instead_of_asking_to_upgrade() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_workspace("dev-vault-secret-unsafe-local-key");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.dev]
run = "printf %s \"$DATABASE_URL\""
"#,
    );
    write_test_vault_with_local_dev_unlock(
        &root,
        "vault-passphrase",
        &[("database_url", "postgres://secret-value")],
    );
    // A key that exists but is readable by other users is a real failure, not
    // the "this machine has no local-dev key yet" state an interactive upgrade
    // answers. Resolving it must say what is wrong with the key.
    let key_path = local_dev_unlock_key_path(&root.join(".effigy/secrets/local.vault"));
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644))
        .expect("make the local-dev key readable beyond its owner");
    let _env = EnvGuard::set_many(&[
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
    ]);

    let err = run_task(&root, "dev", &[]).expect_err("an unsafe local-dev key must refuse");
    assert_task_invocation_error_contains(err, &["readable by other users", "effigy secrets lock"]);
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

/// Selected-catalog vault routing (effigy#662).
///
/// A child catalog's task-target keys and relative vault belong to that
/// catalog, whether the operator prefixes the task from the workspace root or
/// runs it with cwd inside the child. Fake values stay in marker predicates;
/// task stdout/stderr never carry them.
const CHILD_SIGNING: &str = "child-catalog-signing";
const SIBLING_SIGNING: &str = "sibling-catalog-signing";
const PARENT_SIGNING: &str = "parent-catalog-signing";
const ROOT_TOKEN: &str = "root-catalog-token";
const TASK_ENV_OVERRIDE: &str = "from-task-env";
const VAULT_PASSPHRASE: &str = "vault-passphrase";

struct NestedSecretWorkspace {
    root: PathBuf,
    desktop: PathBuf,
    sibling: PathBuf,
    bare: PathBuf,
}

fn nested_secret_workspace(name: &str) -> NestedSecretWorkspace {
    let root = temp_workspace(name);
    let apps = create_workspace_dir(&root, "apps");
    let desktop = create_workspace_dir(&apps, "bovine-desktop");
    let sibling = create_workspace_dir(&apps, "sibling");
    let bare = create_workspace_dir(&apps, "bare");
    write_manifest(
        &sibling.join("effigy.toml"),
        "[catalog]\nalias = \"sibling\"\n",
    );
    write_manifest(&bare.join("effigy.toml"), "[catalog]\nalias = \"bare\"\n");
    NestedSecretWorkspace {
        root,
        desktop,
        sibling,
        bare,
    }
}

fn child_secret_block() -> &'static str {
    r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.fixture_signing_identity]
required = true
targets = ["tasks"]
"#
}

fn write_presence_script(catalog: &Path) {
    let scripts = catalog.join("scripts");
    fs::create_dir_all(&scripts).expect("mkdir catalog scripts");
    write_executable(
        &scripts.join("preflight.sh"),
        r#"#!/bin/sh
set -eu
marker=$1
expected_file="$(dirname "$0")/expected"
if [ -f "$expected_file" ]; then
  got=${FIXTURE_SIGNING_IDENTITY-}
  exp=$(cat "$expected_file")
  if [ "$got" = "$exp" ]; then
    printf match > "$marker"
    exit 0
  fi
  if [ -z "$got" ]; then
    printf missing > "$marker"
    exit 1
  fi
  printf mismatch > "$marker"
  exit 1
fi
if [ -n "${FIXTURE_SIGNING_IDENTITY:-}" ]; then
  printf present > "$marker"
  exit 0
fi
printf missing > "$marker"
exit 0
"#,
    );
}

fn write_expected_signing(catalog: &Path, value: &str) {
    fs::write(catalog.join("scripts/expected"), value).expect("write expected signing value");
}

fn write_desktop_catalog(desktop: &Path) {
    write_presence_script(desktop);
    write_manifest(
        &desktop.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "desktop"
{secrets}
[tasks."package:mac:preflight"]
secrets = "required"
run = "sh scripts/preflight.sh {marker}"
"#,
            secrets = child_secret_block(),
            marker = desktop.join("preflight.marker").display(),
        ),
    );
}

fn write_root_with_members(root: &Path, body: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[catalog]
alias = "root"

[catalog.members]
desktop = "apps/bovine-desktop"
sibling = "apps/sibling"
bare = "apps/bare"

{body}
"#
        ),
    );
}

fn vault_passphrase_env() -> EnvGuard {
    EnvGuard::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        Some(VAULT_PASSPHRASE.to_owned()),
    )])
}

fn assert_secret_absent_from_output(out: &str) {
    for value in [CHILD_SIGNING, SIBLING_SIGNING, PARENT_SIGNING, ROOT_TOKEN] {
        assert!(
            !out.contains(value),
            "task output disclosed a fixture vault value"
        );
    }
}

fn run_prefixed_preflight(root: &Path) -> Result<String, crate::runner::error::RunnerError> {
    run_task(root, "desktop/package:mac:preflight", &[])
}

fn run_child_cwd_preflight(desktop: &Path) -> Result<String, crate::runner::error::RunnerError> {
    run_task_in_workspace(desktop, "package:mac:preflight", &[])
}

fn assert_preflight_matched(desktop: &Path, out: &str) {
    assert_output_equals(out, "");
    assert_secret_absent_from_output(out);
    assert_file_text_equals(&desktop.join("preflight.marker"), "match");
}

/// Causal reproduction: the root catalog has no child keys, the child stores
/// them, and the direct-shell run does not spell the env names. Prefix and
/// child-cwd selection both inject the child's vault.
#[test]
fn selected_catalog_eager_task_secrets_use_the_child_vault_from_prefix_and_cwd() {
    let workspace = nested_secret_workspace("catalog-task-secrets-causal");
    write_root_with_members(
        &workspace.root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.root_only_token]
required = true
targets = ["tasks"]

[tasks.root-probe]
run = "printf root"
"#,
    );
    write_desktop_catalog(&workspace.desktop);
    write_expected_signing(&workspace.desktop, CHILD_SIGNING);
    write_test_vault(
        &workspace.root,
        VAULT_PASSPHRASE,
        &[("root_only_token", ROOT_TOKEN)],
    );
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );
    let _env = vault_passphrase_env();

    let prefixed = run_prefixed_preflight(&workspace.root).expect("prefixed child task");
    assert_preflight_matched(&workspace.desktop, &prefixed);

    fs::remove_file(workspace.desktop.join("preflight.marker")).expect("reset marker");
    let child_cwd = run_child_cwd_preflight(&workspace.desktop).expect("child-cwd task");
    assert_preflight_matched(&workspace.desktop, &child_cwd);
}

/// Parent and sibling catalogs store the same key names with different fake
/// values. The selected child must not receive either, and a child with no
/// secret declarations must not inherit them.
#[test]
fn selected_catalog_task_secrets_do_not_cross_parent_or_sibling_vaults() {
    let workspace = nested_secret_workspace("catalog-task-secrets-isolation");
    write_root_with_members(
        &workspace.root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.fixture_signing_identity]
required = true
targets = ["tasks"]

[tasks.root-probe]
run = "printf root"
"#,
    );
    write_desktop_catalog(&workspace.desktop);
    write_expected_signing(&workspace.desktop, CHILD_SIGNING);
    write_presence_script(&workspace.sibling);
    write_manifest(
        &workspace.sibling.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "sibling"
{secrets}
[tasks."package:mac:preflight"]
secrets = "required"
run = "sh scripts/preflight.sh {marker}"
"#,
            secrets = child_secret_block(),
            marker = workspace.sibling.join("preflight.marker").display(),
        ),
    );
    write_expected_signing(&workspace.sibling, SIBLING_SIGNING);
    write_presence_script(&workspace.bare);
    write_manifest(
        &workspace.bare.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "bare"

[tasks.probe]
run = "sh scripts/preflight.sh {marker}"
"#,
            marker = workspace.bare.join("preflight.marker").display(),
        ),
    );
    write_test_vault(
        &workspace.root,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", PARENT_SIGNING)],
    );
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );
    write_test_vault(
        &workspace.sibling,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", SIBLING_SIGNING)],
    );
    let _env = vault_passphrase_env();

    let child = run_prefixed_preflight(&workspace.root).expect("child keeps its vault");
    assert_preflight_matched(&workspace.desktop, &child);

    let sibling = run_task(&workspace.root, "sibling/package:mac:preflight", &[])
        .expect("sibling keeps its vault");
    assert_output_equals(&sibling, "");
    assert_secret_absent_from_output(&sibling);
    assert_file_text_equals(&workspace.sibling.join("preflight.marker"), "match");

    let bare =
        run_task(&workspace.root, "bare/probe", &[]).expect("undeclared child does not inherit");
    assert_output_equals(&bare, "");
    assert_secret_absent_from_output(&bare);
    assert_file_text_equals(&workspace.bare.join("preflight.marker"), "missing");
}

#[test]
fn selected_catalog_task_env_outranks_the_child_vault() {
    let workspace = nested_secret_workspace("catalog-task-secrets-env-precedence");
    write_root_with_members(
        &workspace.root,
        "[tasks.root-probe]\nrun = \"printf root\"\n",
    );
    write_presence_script(&workspace.desktop);
    write_manifest(
        &workspace.desktop.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "desktop"
{secrets}
[tasks.override]
secrets = "required"
env = {{ FIXTURE_SIGNING_IDENTITY = "{override_value}" }}
run = "sh scripts/preflight.sh {marker}"
"#,
            secrets = child_secret_block(),
            override_value = TASK_ENV_OVERRIDE,
            marker = workspace.desktop.join("override.marker").display(),
        ),
    );
    write_expected_signing(&workspace.desktop, TASK_ENV_OVERRIDE);
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );
    let _env = vault_passphrase_env();

    let out = run_task(&workspace.root, "desktop/override", &[]).expect("env outranks vault");
    assert_output_equals(&out, "");
    assert_secret_absent_from_output(&out);
    assert_file_text_equals(&workspace.desktop.join("override.marker"), "match");
}

#[test]
fn selected_catalog_missing_required_secret_refuses_before_spawn() {
    let workspace = nested_secret_workspace("catalog-task-secrets-missing");
    write_root_with_members(
        &workspace.root,
        "[tasks.root-probe]\nrun = \"printf root\"\n",
    );
    write_desktop_catalog(&workspace.desktop);
    write_test_vault(&workspace.desktop, VAULT_PASSPHRASE, &[]);
    let _env = vault_passphrase_env();

    let err = run_prefixed_preflight(&workspace.root).expect_err("missing required child secret");
    assert_task_invocation_error_contains(
        err,
        &[
            "required task secret(s) missing from the vault",
            "fixture_signing_identity",
        ],
    );
    assert!(
        !workspace.desktop.join("preflight.marker").exists(),
        "missing required secrets must not spawn the child command"
    );
}

#[test]
fn selected_catalog_referenced_only_skips_unmentioned_required_keys() {
    let workspace = nested_secret_workspace("catalog-task-secrets-referenced-only");
    write_root_with_members(
        &workspace.root,
        "[tasks.root-probe]\nrun = \"printf root\"\n",
    );
    write_presence_script(&workspace.desktop);
    write_manifest(
        &workspace.desktop.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "desktop"
{secrets}
[tasks.quiet]
run = "sh -lc 'printf ok > \"{marker}\"'"
"#,
            secrets = child_secret_block(),
            marker = workspace.desktop.join("quiet.marker").display(),
        ),
    );
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );

    assert_run_task_ok_empty(&workspace.root, "desktop/quiet", &[]);
    assert_file_text_equals(&workspace.desktop.join("quiet.marker"), "ok");
}

#[test]
fn root_catalog_task_secrets_still_use_the_root_vault() {
    let workspace = nested_secret_workspace("catalog-task-secrets-root-regression");
    let marker = workspace.root.join("root.marker");
    write_root_with_members(
        &workspace.root,
        &format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.root_only_token]
required = true
targets = ["tasks"]

[tasks.root-probe]
secrets = "required"
run = "sh -lc 'test -n \"$ROOT_ONLY_TOKEN\" && printf ok > \"{marker}\"'"
"#,
            marker = marker.display(),
        ),
    );
    write_desktop_catalog(&workspace.desktop);
    write_test_vault(
        &workspace.root,
        VAULT_PASSPHRASE,
        &[("root_only_token", ROOT_TOKEN)],
    );
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );
    let _env = vault_passphrase_env();

    assert_run_task_ok_empty(&workspace.root, "root-probe", &[]);
    assert_file_text_equals(&marker, "ok");
}

#[test]
fn selected_catalog_required_secrets_generate_against_the_child_vault() {
    let workspace = nested_secret_workspace("catalog-task-secrets-generate");
    write_root_with_members(
        &workspace.root,
        "[tasks.root-probe]\nrun = \"printf root\"\n",
    );
    write_presence_script(&workspace.desktop);
    write_expected_signing(&workspace.desktop, "tok_generated");
    write_manifest(
        &workspace.desktop.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "desktop"

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
generate = {{ task = "secrets:generate-dev" }}

[secrets.keys.fixture_signing_identity]
required = true
targets = ["tasks"]

[tasks."package:mac:preflight"]
secrets = "required"
run = "sh scripts/preflight.sh {marker}"

[tasks."secrets:generate-dev"]
run = [
  {{ task = "secrets init" }},
  {{ task = "secrets set fixture_signing_identity" }},
]
"#,
            marker = workspace.desktop.join("preflight.marker").display(),
        ),
    );
    let _env = EnvGuard::set_many(&[
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            Some(VAULT_PASSPHRASE.to_owned()),
        ),
        (
            "EFFIGY_TEST_SECRETS_VALUE",
            Some("tok_generated".to_owned()),
        ),
    ]);

    let out = run_prefixed_preflight(&workspace.root).expect("child generate hook");
    assert_preflight_matched(&workspace.desktop, &out);
}

#[test]
fn selected_catalog_included_secret_declarations_stay_on_the_child_vault() {
    let workspace = nested_secret_workspace("catalog-task-secrets-include");
    write_root_with_members(
        &workspace.root,
        "[tasks.root-probe]\nrun = \"printf root\"\n",
    );
    write_presence_script(&workspace.desktop);
    write_expected_signing(&workspace.desktop, CHILD_SIGNING);
    fs::write(workspace.desktop.join("secrets.toml"), child_secret_block())
        .expect("write included secrets fragment");
    write_manifest(
        &workspace.desktop.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "desktop"

[manifest]
include = ["secrets.toml"]

[tasks."package:mac:preflight"]
secrets = "required"
run = "sh scripts/preflight.sh {marker}"
"#,
            marker = workspace.desktop.join("preflight.marker").display(),
        ),
    );
    write_test_vault(
        &workspace.desktop,
        VAULT_PASSPHRASE,
        &[("fixture_signing_identity", CHILD_SIGNING)],
    );
    let _env = vault_passphrase_env();

    let out = run_prefixed_preflight(&workspace.root).expect("included child secrets");
    assert_preflight_matched(&workspace.desktop, &out);
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

fn write_test_vault_with_local_dev_unlock(
    root: &std::path::Path,
    passphrase: &str,
    records: &[(&str, &str)],
) {
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let key = LocalDevUnlockKey::generate().expect("generate local-dev key");
    let envelope = payload
        .encrypt_with_passphrase_and_local_dev_key(passphrase, &key)
        .expect("encrypt test vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        &vault_path,
        envelope.to_json_pretty().expect("serialize test vault"),
    )
    .expect("write test vault");
    let key_path = local_dev_unlock_key_path(&vault_path);
    fs::write(&key_path, key.expose()).expect("write local-dev key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .expect("secure local-dev key");
    }
}
