use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_root_manifest, EnvGuard,
};
use effigy_cli::{Command, SecretsArgs, SecretsExportFormat, SecretsSubcommand};
use effigy_secrets::{VaultEnvelope, VaultPlaintextPayload};
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
fn secrets_get_prints_one_declared_secret_value() {
    let root = temp_workspace("secrets-get");
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
        subcommand: SecretsSubcommand::Get {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("get should succeed");

    assert_eq!(out, "postgres://secret-value");
}

#[test]
fn secrets_get_json_returns_one_declared_secret_value() {
    let root = temp_workspace("secrets-get-json");
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
        subcommand: SecretsSubcommand::Get {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("get should succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("get"));
    assert_eq!(parsed["name"].as_str(), Some("database_url"));
    assert_eq!(parsed["value"].as_str(), Some("postgres://secret-value"));
}

#[test]
fn secrets_get_rejects_missing_stored_value() {
    let root = temp_workspace("secrets-get-missing-value");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Get {
            name: "database_url".to_owned(),
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("get should fail");

    assert!(error
        .to_string()
        .contains("secret `database_url` is not stored"));
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

#[test]
fn secrets_doctor_reports_locked_vault_without_passphrase() {
    let root = temp_workspace("secrets-doctor-locked");
    write_root_manifest(&root, declared_secrets_manifest());
    {
        let _env = secret_test_env("vault-passphrase", None);
        run_command(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override: Some(root.clone()),
            output_json: false,
        }))
        .expect("init should succeed");
    }
    let _env = secret_test_env_clear();

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("locked doctor should warn but succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["vault_state"]["status"].as_str(), Some("locked"));
    assert_eq!(parsed["ok"].as_bool(), Some(true));
}

#[test]
fn secrets_doctor_blocks_missing_required_when_unlocked() {
    let root = temp_workspace("secrets-doctor-missing-required");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect_err("doctor should block missing required");

    let rendered = error.to_string();
    assert!(rendered.contains("required secret `database_url` is missing from the vault"));
    assert!(!rendered.contains("vault-passphrase"));
}

#[test]
fn secrets_doctor_blocks_corrupt_vault() {
    let root = temp_workspace("secrets-doctor-corrupt");
    write_root_manifest(&root, declared_secrets_manifest());
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("parent")).expect("mkdir");
    fs::write(&vault_path, "not-json").expect("write corrupt");
    let _env = secret_test_env("vault-passphrase", None);

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("doctor should block corrupt vault");

    assert!(error.to_string().contains("failed to parse secrets vault"));
}

#[test]
fn secrets_doctor_blocks_undeclared_stored_values() {
    let root = temp_workspace("secrets-doctor-undeclared");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);
    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "orphan".to_owned(),
        effigy_secrets::VaultSecretRecord::new(effigy_secrets::SecretValue::new("hidden")),
    );
    let envelope = payload
        .encrypt_with_passphrase("vault-passphrase")
        .expect("encrypt");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("parent")).expect("mkdir");
    fs::write(&vault_path, envelope.to_json_pretty().expect("json")).expect("write vault");

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect_err("doctor should block undeclared stored value");

    let rendered = error.to_string();
    assert!(rendered.contains("stored secret `orphan` is not declared under `[secrets.keys]`"));
    assert!(!rendered.contains("hidden"));
}

#[test]
fn secrets_export_writes_env_file_without_printing_values() {
    let root = temp_workspace("secrets-export-env");
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
        subcommand: SecretsSubcommand::Export {
            format: SecretsExportFormat::Env,
            output: std::path::PathBuf::from(".effigy/runtime/secrets/local.env"),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("export should succeed");

    assert!(!out.contains("postgres://secret-value"));
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("export"));
    assert_eq!(parsed["format"].as_str(), Some("env"));
    assert_eq!(parsed["keys_exported"][0].as_str(), Some("DATABASE_URL"));
    let exported =
        fs::read_to_string(root.join(".effigy/runtime/secrets/local.env")).expect("read export");
    assert_eq!(exported, "DATABASE_URL=postgres://secret-value\n");
}

#[test]
fn secrets_export_requires_yes_and_refuses_repo_root_env() {
    let root = temp_workspace("secrets-export-guardrails");
    write_root_manifest(&root, declared_secrets_manifest());

    let missing_yes = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Export {
            format: SecretsExportFormat::Env,
            output: std::path::PathBuf::from(".effigy/runtime/secrets/local.env"),
            yes: false,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("missing yes should fail");
    assert!(missing_yes.to_string().contains("requires `--yes`"));

    let root_env = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Export {
            format: SecretsExportFormat::Env,
            output: std::path::PathBuf::from(".env"),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("repo root .env should fail");
    assert!(root_env
        .to_string()
        .contains("refuses to write repo-root `.env`"));
}

#[test]
fn secrets_export_blocks_missing_required_before_writing() {
    let root = temp_workspace("secrets-export-missing-required");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Export {
            format: SecretsExportFormat::Env,
            output: std::path::PathBuf::from(".effigy/runtime/secrets/local.env"),
            yes: true,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("missing required should fail");

    assert!(error
        .to_string()
        .contains("required secret(s) missing from the vault"));
    assert!(!root.join(".effigy/runtime/secrets/local.env").exists());
}

#[test]
fn secrets_change_passphrase_reencrypts_existing_vault() {
    let root = temp_workspace("secrets-change-passphrase");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env_with_new_passphrase(
        "old-vault-passphrase",
        Some("postgres://secret-value"),
        Some("new-vault-passphrase"),
    );
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
        subcommand: SecretsSubcommand::ChangePassphrase,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("change-passphrase should succeed");

    assert!(!out.contains("postgres://secret-value"));
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("change-passphrase"));
    assert_eq!(parsed["changed"].as_bool(), Some(true));
    assert_eq!(parsed["records_preserved"].as_u64(), Some(1));

    let envelope = read_test_vault(&root.join(".effigy/secrets/local.vault"));
    assert!(envelope
        .decrypt_with_passphrase("old-vault-passphrase")
        .is_err());
    let decrypted = envelope
        .decrypt_with_passphrase("new-vault-passphrase")
        .expect("decrypt with new passphrase");
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

fn secret_test_env(passphrase: &str, value: Option<&str>) -> EnvGuard {
    secret_test_env_with_new_passphrase(passphrase, value, None)
}

fn secret_test_env_with_new_passphrase(
    passphrase: &str,
    value: Option<&str>,
    new_passphrase: Option<&str>,
) -> EnvGuard {
    EnvGuard::set_many(&[
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
            Some(passphrase.to_owned()),
        ),
        ("EFFIGY_TEST_SECRETS_VALUE", value.map(str::to_owned)),
        (
            "EFFIGY_TEST_SECRETS_NEW_PASSPHRASE",
            new_passphrase.map(str::to_owned),
        ),
    ])
}

fn secret_test_env_clear() -> EnvGuard {
    EnvGuard::set_many(&[
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_TEST_SECRETS_VALUE", None),
        ("EFFIGY_TEST_SECRETS_NEW_PASSPHRASE", None),
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
