use crate::runner::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_root_manifest, EnvGuard,
};
use effigy_cli::{Command, SecretsArgs, SecretsExportFormat, SecretsSubcommand};
use effigy_secrets::{
    local_dev_unlock_key_path, LocalDevUnlockKey, VaultEnvelope, VaultPlaintextPayload,
};
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
    assert!(parsed["vault"].get("identity").is_none());
    assert!(parsed["vault"].get("unlock").is_none());
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
    let local_dev_key = LocalDevUnlockKey::from_bytes(
        fs::read(local_dev_unlock_key_path(&vault_path)).expect("read local-dev key"),
    )
    .expect("parse local-dev key");
    assert!(envelope
        .decrypt_with_local_dev_key(&local_dev_key)
        .expect("decrypt for local dev")
        .records
        .is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(local_dev_unlock_key_path(&vault_path))
            .expect("local-dev key metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn secrets_init_runs_configured_generate_task_when_present() {
    let root = temp_workspace("secrets-init-generate");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("scripts/generate-dev-secrets.rhai"),
        r#"secrets::set("database_url", "postgres://secret-value");"#,
    )
    .expect("write generator script");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
generate = { rhai = "scripts/generate-dev-secrets.rhai", run_in = "host" }

[secrets.keys.database_url]
required = false
targets = ["rhai"]
"#,
    );
    let _env = secret_test_env("vault-passphrase", None);

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("init should succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("init"));
    assert_eq!(
        parsed["summary"].as_str(),
        Some("generated local vault via configured task")
    );
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
    let local_dev_key = LocalDevUnlockKey::from_bytes(
        fs::read(local_dev_unlock_key_path(
            &root.join(".effigy/secrets/local.vault"),
        ))
        .expect("read local-dev key"),
    )
    .expect("parse local-dev key");
    let local_dev = envelope
        .decrypt_with_local_dev_key(&local_dev_key)
        .expect("decrypt generated local-dev payload");
    assert_eq!(
        local_dev
            .records
            .get("database_url")
            .expect("local-dev record")
            .value
            .expose(),
        "postgres://secret-value"
    );
}

#[test]
fn secrets_import_creates_vault_from_dot_env_and_normalizes_keys() {
    let root = temp_workspace("secrets-import-create");
    write_root_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.openai_api_key]
required = false
targets = ["containers"]

[secrets.keys.mailchimp_api_key]
required = false
targets = ["containers"]
"#,
    );
    fs::write(
        root.join(".env"),
        "OPENAI_API_KEY='openai-secret'\nMAILCHIMP_API_KEY=\"mailchimp-secret\"\nENV_MODE='development'\n",
    )
    .expect("write env");
    let _env = secret_test_env("vault-passphrase", None);

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Import {
            input: std::path::PathBuf::from(".env"),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("import should succeed");

    assert!(!out.contains("openai-secret"));
    assert!(!out.contains("mailchimp-secret"));
    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("import"));
    assert_eq!(parsed["format"].as_str(), Some("env"));
    assert_eq!(parsed["created_vault"].as_bool(), Some(true));
    assert_eq!(parsed["changed"].as_bool(), Some(true));
    assert_eq!(parsed["imported"].as_array().expect("imported").len(), 2);
    assert_eq!(parsed["skipped_undeclared"][0].as_str(), Some("ENV_MODE"));
    let envelope = read_test_vault(&root.join(".effigy/secrets/local.vault"));
    let decrypted = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt");
    assert_eq!(
        decrypted
            .records
            .get("openai_api_key")
            .expect("openai_api_key")
            .value
            .expose(),
        "openai-secret"
    );
    assert_eq!(
        decrypted
            .records
            .get("mailchimp_api_key")
            .expect("mailchimp_api_key")
            .value
            .expose(),
        "mailchimp-secret"
    );
}

#[test]
fn secrets_import_does_not_create_vault_when_no_declared_keys_match() {
    let root = temp_workspace("secrets-import-no-match");
    write_root_manifest(&root, declared_secrets_manifest());
    fs::write(root.join(".env"), "ENV_MODE='development'\n").expect("write env");
    let _env = secret_test_env("vault-passphrase", None);

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Import {
            input: std::path::PathBuf::from(".env"),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("import should succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["action"].as_str(), Some("import"));
    assert_eq!(parsed["changed"].as_bool(), Some(false));
    assert_eq!(parsed["created_vault"].as_bool(), Some(false));
    assert_eq!(parsed["imported"].as_array().expect("imported").len(), 0);
    assert!(!root.join(".effigy/secrets/local.vault").exists());
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
    // A vault no factor can open: no passphrase in this invocation, and no
    // local-dev key for a dev session to read it through.
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::remove_file(local_dev_unlock_key_path(&vault_path)).expect("remove local-dev key");
    let _env = secret_test_env_clear();

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("locked doctor should warn but succeed");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert!(parsed["vault"].get("identity").is_none());
    assert!(parsed["vault"].get("unlock").is_none());
    assert_eq!(parsed["vault_state"]["status"].as_str(), Some("locked"));
    assert!(parsed["vault_state"]["unlock"].is_null());
    assert_eq!(parsed["ok"].as_bool(), Some(true));
}

#[test]
fn secrets_doctor_reports_the_local_dev_key_and_the_keys_it_can_read() {
    let root = temp_workspace("secrets-doctor-local-dev-key");
    write_root_manifest(&root, declared_secrets_manifest());
    {
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
    }
    let _env = secret_test_env_clear();

    let out = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("doctor should read the vault through the local-dev key");

    let parsed = parse_json_output_with_schema_version(&out, "effigy.secrets.v1", 1);
    assert_eq!(parsed["vault_state"]["status"].as_str(), Some("unlocked"));
    assert_eq!(
        parsed["vault_state"]["unlock"].as_str(),
        Some("local-dev-key")
    );
    assert_eq!(
        parsed["vault_state"]["stored_keys"],
        serde_json::json!(["database_url"])
    );
    // Names only: doctor never prints a stored value.
    assert!(!out.contains("postgres://secret-value"));
}

#[cfg(unix)]
#[test]
fn secrets_doctor_blocks_unsafe_local_dev_key_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_workspace("secrets-doctor-local-dev-key-permissions");
    write_root_manifest(&root, declared_secrets_manifest());
    let _env = secret_test_env("vault-passphrase", None);
    run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Init,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("init should succeed");
    let key_path = local_dev_unlock_key_path(&root.join(".effigy/secrets/local.vault"));
    fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644))
        .expect("loosen local-dev key permissions");

    let error = run_command(Command::Secrets(SecretsArgs {
        subcommand: SecretsSubcommand::Doctor,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect_err("doctor should block unsafe local-dev key permissions");

    assert!(error
        .to_string()
        .contains("local-dev unlock key permissions are unsafe"));
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
    // The guard holds the test lock, so no other test can cache a passphrase
    // between this clear and the caller's reads.
    let guard = EnvGuard::set_many(&[
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
        ("EFFIGY_SECRET_PASSPHRASE", None),
        ("EFFIGY_TEST_SECRETS_VALUE", None),
        ("EFFIGY_TEST_SECRETS_NEW_PASSPHRASE", None),
    ]);
    crate::runner::secret_session::forget_session_passphrase_for_tests();
    guard
}

fn read_test_vault(path: &std::path::Path) -> VaultEnvelope {
    effigy_secrets::storage::read_envelope(path).expect("read vault")
}

fn declared_secrets_manifest() -> &'static str {
    r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Postgres connection string"
"#
}

// ---- A managed session's vault access (032, Q-007) --------------------------
//
// A managed session resolves the access it shares with everything it starts
// once, before any front end renders: from what the invocation holds, the
// sealed command credential `effigy secrets unlock` leaves, or one prompt on
// the real terminal. These proofs exercise that resolution directly; the CLI
// proof in `tests/cli_output_tests` drives a real session on a pseudo
// terminal.

use crate::runner::execute::resolve_managed_session_secret_access;
use crate::runner::secret_session::{session_passphrase, session_secret_passphrase_env};
use effigy_secrets::{local_unlock_credential_path, SecretValue, VaultSecretRecord};

fn write_session_vault(
    root: &std::path::Path,
    passphrase: &str,
    with_local_dev_key: bool,
    records: &[(&str, &str)],
) {
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    let envelope = if with_local_dev_key {
        let local_dev_key = LocalDevUnlockKey::generate().expect("generate local-dev key");
        let key_path = local_dev_unlock_key_path(&vault_path);
        fs::write(&key_path, local_dev_key.expose()).expect("write local-dev key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
                .expect("secure local-dev key");
        }
        payload
            .encrypt_with_passphrase_and_local_dev_key(passphrase, &local_dev_key)
            .expect("encrypt vault with local-dev key")
    } else {
        payload
            .encrypt_with_passphrase(passphrase)
            .expect("encrypt vault")
    };
    fs::write(
        &vault_path,
        envelope.to_json_pretty().expect("serialize vault"),
    )
    .expect("write vault");
}

fn session_secrets_manifest() -> &'static str {
    r#"
[tasks.dev]
run = "printf dev"

[tasks.probe]
run = "printf probe"
secrets = "required"

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["tasks"]
"#
}

fn clear_session_secret_env() -> EnvGuard {
    let guard = EnvGuard::set_many(&[
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
        (session_secret_passphrase_env(), None),
        ("EFFIGY_TEST_SECRETS_VALUE", None),
        ("EFFIGY_TEST_SECRETS_NEW_PASSPHRASE", None),
    ]);
    crate::runner::secret_session::forget_session_passphrase_for_tests();
    guard
}

/// With no terminal and no source the session refuses before it starts
/// anything, and the refusal names what is locked and what to run next.
#[test]
fn managed_session_refuses_a_locked_vault_without_a_terminal() {
    let root = temp_workspace("session-secrets-refusal");
    write_root_manifest(&root, session_secrets_manifest());
    write_session_vault(&root, "vault-passphrase", false, &[]);
    let _env = clear_session_secret_env();

    let error = resolve_managed_session_secret_access(&root, "dev", "default")
        .expect_err("a locked vault with no terminal must refuse");
    let message = error.to_string();
    assert!(message.contains("managed task `dev`"), "{message}");
    assert!(message.contains("is locked"), "{message}");
    assert!(
        message.contains("secret input requires an interactive TTY"),
        "{message}"
    );
    assert!(message.contains("effigy secrets unlock"), "{message}");
    assert!(
        message.contains(session_secret_passphrase_env()),
        "{message}"
    );
    // Nothing was written and nothing was started: a refusal is not an
    // implicit `secrets unlock`.
    assert!(!local_unlock_credential_path(&root.join(".effigy/secrets/local.vault")).exists());
    assert!(session_passphrase().is_none());
}

/// A caller-supplied passphrase answers without a terminal, and every child
/// the session starts receives it on the internal route, not the public name.
#[test]
fn managed_session_shares_a_caller_supplied_passphrase_with_children() {
    let root = temp_workspace("session-secrets-caller");
    write_root_manifest(&root, session_secrets_manifest());
    write_session_vault(
        &root,
        "vault-passphrase",
        false,
        &[("database_url", "postgres://secret-value")],
    );
    let _env = EnvGuard::set_many(&[
        (
            session_secret_passphrase_env(),
            Some("vault-passphrase".to_owned()),
        ),
        ("EFFIGY_TEST_SECRETS_PASSPHRASE", None),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
    ]);
    crate::runner::secret_session::forget_session_passphrase_for_tests();

    resolve_managed_session_secret_access(&root, "dev", "default")
        .expect("the caller supplied the passphrase");
    assert_eq!(
        session_passphrase().map(|value| value.expose().to_owned()),
        Some("vault-passphrase".to_owned())
    );
    // Resolving again is a cache read: a session asks at most once.
    resolve_managed_session_secret_access(&root, "dev", "default")
        .expect("a resolved session resolves from what it holds");

    // The session's own children receive that access through the one-shot
    // descriptor grant, not an environment variable (effigy#338).
    let grant =
        crate::runner::secret_session::take_child_grant().expect("a session with access grants it");
    let reader = std::fs::File::from(grant.into_reader());
    let frame = {
        use std::io::Read;
        let mut frame = Vec::new();
        let mut reader = reader;
        reader
            .read_to_end(&mut frame)
            .expect("read the grant frame");
        frame
    };
    assert!(
        frame.ends_with(b"vault-passphrase"),
        "the grant carries the session's passphrase"
    );
    assert!(
        !frame
            .windows(session_secret_passphrase_env().len())
            .any(|window| window == session_secret_passphrase_env().as_bytes()),
        "the public source name never reaches a child"
    );
    assert!(!local_unlock_credential_path(&root.join(".effigy/secrets/local.vault")).exists());

    crate::runner::secret_session::forget_session_passphrase_for_tests();
}

/// The sealed command credential answers without a terminal, exactly as it
/// does for a task today, and the session then shares what it read.
#[test]
fn managed_session_uses_a_sealed_credential_without_a_terminal() {
    let root = temp_workspace("session-secrets-sealed");
    write_root_manifest(&root, session_secrets_manifest());
    write_session_vault(&root, "vault-passphrase", false, &[]);
    {
        let _env = secret_test_env("vault-passphrase", None);
        run_command(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Unlock,
            repo_override: Some(root.clone()),
            output_json: false,
        }))
        .expect("unlock should seal a command credential");
    }
    let _env = clear_session_secret_env();
    let vault_path = root.join(".effigy/secrets/local.vault");
    assert!(local_unlock_credential_path(&vault_path).exists());

    resolve_managed_session_secret_access(&root, "dev", "default")
        .expect("the sealed credential answers");
    assert_eq!(
        session_passphrase().map(|value| value.expose().to_owned()),
        Some("vault-passphrase".to_owned())
    );

    crate::runner::secret_session::forget_session_passphrase_for_tests();
}

/// The local-dev key is a `dev` session's own factor, not the session's
/// children's: a session that is not `dev` still asks once.
#[test]
fn managed_session_with_only_the_local_dev_key_still_asks_once() {
    let root = temp_workspace("session-secrets-local-dev");
    write_root_manifest(&root, session_secrets_manifest());
    write_session_vault(
        &root,
        "vault-passphrase",
        true,
        &[("database_url", "postgres://secret-value")],
    );
    let _env = clear_session_secret_env();

    let error = resolve_managed_session_secret_access(&root, "serve", "default")
        .expect_err("the local-dev key does not answer a non-dev session");
    let message = error.to_string();
    assert!(message.contains("managed task `serve`"), "{message}");
    assert!(message.contains("effigy secrets unlock"), "{message}");
    assert!(!local_unlock_credential_path(&root.join(".effigy/secrets/local.vault")).exists());
}

/// A session that declares no built-in vault has nothing locked to resolve.
#[test]
fn managed_session_without_a_declared_vault_asks_nothing() {
    let root = temp_workspace("session-secrets-undeclared");
    write_root_manifest(&root, "[tasks.dev]\nrun = \"printf dev\"\n");
    let _env = clear_session_secret_env();

    resolve_managed_session_secret_access(&root, "dev", "default")
        .expect("nothing is declared, so nothing is asked");
    assert!(session_passphrase().is_none());
}

/// The local-dev marker still reaches a session's children beside the
/// passphrase, on the same grant: the marker is what a `dev` session's own
/// work uses.
#[test]
fn managed_session_child_grant_carries_the_local_dev_marker() {
    let root = temp_workspace("session-secrets-marker");
    write_root_manifest(&root, session_secrets_manifest());
    write_session_vault(&root, "vault-passphrase", false, &[]);
    let _env = EnvGuard::set_many(&[
        (
            session_secret_passphrase_env(),
            Some("vault-passphrase".to_owned()),
        ),
        ("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None),
    ]);
    crate::runner::secret_session::forget_session_passphrase_for_tests();
    let local_dev = crate::runner::secret_session::activate_local_dev_secret_access(true);

    resolve_managed_session_secret_access(&root, "dev", "default")
        .expect("the caller supplied the passphrase");
    let grant =
        crate::runner::secret_session::take_child_grant().expect("a session with access grants it");
    let frame = {
        use std::io::Read;
        let mut frame = Vec::new();
        let mut reader = std::fs::File::from(grant.into_reader());
        reader
            .read_to_end(&mut frame)
            .expect("read the grant frame");
        frame
    };
    assert_eq!(
        frame[9] & 1,
        1,
        "the grant marks the local-dev access a dev session's own work uses"
    );

    drop(local_dev);
    crate::runner::secret_session::forget_session_passphrase_for_tests();
}
