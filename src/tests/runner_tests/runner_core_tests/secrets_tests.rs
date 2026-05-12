use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_root_manifest, EnvGuard,
};
use effigy_cli::{Command, SecretsArgs, SecretsSubcommand};
use effigy_secrets::VaultEnvelope;
use std::fs;

#[test]
fn secrets_doctor_json_treats_missing_section_as_ok() {
    let root = temp_workspace("secrets-doctor-missing");
    write_root_manifest(&root, "[tasks.dev]\nrun = \"printf ok\"\n");

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("doctor should succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["ok"].as_bool(), Some(true));
    assert_eq!(parsed["declared"].as_bool(), Some(false));
    assert_eq!(parsed["keys"].as_array().expect("keys array").len(), 0);
    assert_eq!(
        parsed["blockers"].as_array().expect("blockers array").len(),
        0
    );
}

#[test]
fn secrets_list_json_reports_declarations_without_values() {
    let root = temp_workspace("secrets-list-json");
    write_root_manifest(&root, declared_secrets_manifest());

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::List,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("list should succeed");

    assert!(
        !out.contains("postgres://secret-value"),
        "secrets list must not expose secret values"
    );
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["ok"].as_bool(), Some(true));
    assert_eq!(parsed["declared"].as_bool(), Some(true));
    assert_eq!(parsed["backend"].as_str(), Some("effigy-vault"));
    assert_eq!(
        parsed["vault"]["path"].as_str(),
        Some(".effigy/secrets/local.vault")
    );
    let keys = parsed["keys"].as_array().expect("keys array");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"].as_str(), Some("database_url"));
    assert_eq!(keys[0]["required"].as_bool(), Some(true));
    assert_eq!(
        keys[0]["targets"].as_array().expect("targets array")[0].as_str(),
        Some("tasks")
    );
}

#[test]
fn secrets_list_text_reports_names_not_values() {
    let root = temp_workspace("secrets-list-text");
    write_root_manifest(&root, declared_secrets_manifest());

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::List,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("list should succeed");

    assert!(out.contains("[secrets] declarations"));
    assert!(out.contains("- database_url: required; targets=tasks,containers"));
    assert!(!out.contains("postgres://secret-value"));
}

#[test]
fn secrets_doctor_blocks_missing_vault_config() {
    let root = temp_workspace("secrets-doctor-missing-vault");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"
"#,
    );

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("doctor should fail on blockers");

    assert!(error
        .to_string()
        .contains("`[secrets]` selects `effigy-vault` but `[secrets.vault]` is missing"));
}

#[test]
fn secrets_init_creates_empty_encrypted_vault() {
    let root = temp_workspace("secrets-init");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("init should succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("init"));
    assert_eq!(parsed["changed"].as_bool(), Some(true));
    let vault_path = root.join(".effigy/secrets/local.vault");
    assert!(vault_path.exists());
    let envelope = read_test_vault(&vault_path);
    let decrypted = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt");
    assert!(decrypted.records.is_empty());
}

#[test]
fn secrets_set_stores_declared_secret_without_printing_value() {
    let root = temp_workspace("secrets-set");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", Some("postgres://secret-value"));
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Set {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("set should succeed");

    assert!(!out.contains("postgres://secret-value"));
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("set"));
    assert_eq!(parsed["name"].as_str(), Some("database_url"));
    let envelope = read_test_vault(&root.join(".effigy/secrets/local.vault"));
    let decrypted = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt");
    assert_eq!(
        decrypted
            .records
            .get("database_url")
            .expect("record")
            .value
            .expose(),
        "postgres://secret-value"
    );
}

#[test]
fn secrets_unset_removes_declared_secret() {
    let root = temp_workspace("secrets-unset");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", Some("postgres://secret-value"));
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Set {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("set should succeed");

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Unset {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("unset should succeed");

    assert!(!out.contains("postgres://secret-value"));
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("unset"));
    let envelope = read_test_vault(&root.join(".effigy/secrets/local.vault"));
    let decrypted = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt");
    assert!(!decrypted.records.contains_key("database_url"));
}

#[test]
fn secrets_set_rejects_undeclared_secret() {
    let root = temp_workspace("secrets-set-undeclared");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", Some("secret-value"));

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Set {
            name: "missing".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("set should fail");

    assert!(error
        .to_string()
        .contains("secret `missing` is not declared under `[secrets.keys]`"));
}

fn secret_test_env(passphrase: &str, value: Option<&str>) -> EnvGuard {
    EnvGuard::set_many(&[
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            Some(passphrase.to_owned()),
        ),
        ("EFFIGY_TEST_SECRETS_VALUE", value.map(str::to_owned)),
    ])
}

fn read_test_vault(path: &std::path::Path) -> VaultEnvelope {
    let raw = fs::read_to_string(path).expect("read vault");
    VaultEnvelope::from_json(&raw).expect("parse vault")
}

fn declared_secrets_manifest() -> &'static str {
    r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "ssh-agent"
unlock = "key-and-passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Postgres connection string"
"#
}
