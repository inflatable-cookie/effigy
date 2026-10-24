mod support;

use std::fs;
use std::process::{Command, Stdio};

use effigy_secrets::{
    local_dev_unlock_key_path, local_unlock_credential_path, LocalDevUnlockKey, SecretValue,
    VaultPlaintextPayload, VaultSecretRecord,
};

#[test]
fn dev_and_get_use_the_local_dev_key_and_other_tasks_still_need_a_passphrase() {
    let root = support::temp_workspace("effigy-secrets-cli", "execution");
    write_fixture(&root, true);

    let dev = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .expect("run dev");
    assert!(
        dev.status.success(),
        "dev failed: {}",
        String::from_utf8_lossy(&dev.stderr)
    );

    // The dev key is the factor this machine holds, and `get` is the one
    // command allowed to print a value through it.
    let direct_get = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "get", "api_token"])
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .expect("run direct get");
    assert!(
        direct_get.status.success(),
        "direct get failed: {}",
        String::from_utf8_lossy(&direct_get.stderr)
    );
    assert!(String::from_utf8_lossy(&direct_get.stdout).contains("tok_local_dev"));

    let other_task = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("serve")
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .expect("run non-dev task");
    assert!(!other_task.status.success());
    assert!(String::from_utf8_lossy(&other_task.stderr)
        .contains("secret input requires an interactive TTY"));
}

#[test]
fn dev_key_alone_lets_doctor_report_what_it_can_read() {
    let root = support::temp_workspace("effigy-secrets-cli", "execution");
    write_fixture(&root, true);

    let doctor = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "doctor", "--json"])
        .current_dir(&root)
        .stdin(Stdio::null())
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .output()
        .expect("run doctor");
    assert!(
        doctor.status.success(),
        "doctor failed: {}\nstdout: {}",
        String::from_utf8_lossy(&doctor.stderr),
        String::from_utf8_lossy(&doctor.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor json");
    assert_eq!(json["result"]["vault_state"]["status"], "unlocked");
    assert_eq!(json["result"]["vault_state"]["unlock"], "local-dev-key");
    assert_eq!(
        json["result"]["vault_state"]["stored_keys"],
        serde_json::json!(["api_token"])
    );
    assert!(!String::from_utf8_lossy(&doctor.stdout).contains("tok_local_dev"));
}

#[test]
fn first_dev_unlock_upgrades_a_legacy_vault() {
    let root = support::temp_workspace("effigy-secrets-cli", "execution");
    let vault_path = write_fixture(&root, false);

    let upgrade = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .current_dir(&root)
        .env("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase")
        .stdin(Stdio::null())
        .output()
        .expect("run first dev");
    assert!(
        upgrade.status.success(),
        "upgrade failed: {}",
        String::from_utf8_lossy(&upgrade.stderr)
    );
    assert!(local_dev_unlock_key_path(&vault_path).exists());

    let unattended = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .current_dir(&root)
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .stdin(Stdio::null())
        .output()
        .expect("run unattended dev");
    assert!(
        unattended.status.success(),
        "unattended dev failed: {}",
        String::from_utf8_lossy(&unattended.stderr)
    );
}

#[test]
fn manual_unlock_allows_commands_and_tasks_then_lock_revokes_access() {
    let root = support::temp_workspace("effigy-secrets-cli", "execution");
    let vault_path = write_fixture(&root, false);
    let unlock = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "unlock", "--json"])
        .current_dir(&root)
        .env("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase")
        .stdin(Stdio::null())
        .output()
        .expect("unlock vault");
    assert!(
        unlock.status.success(),
        "{}",
        String::from_utf8_lossy(&unlock.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&unlock.stdout).expect("unlock json");
    assert_eq!(json["result"]["action"], "unlock");
    let credential_path = local_unlock_credential_path(&vault_path);
    let credential = fs::read(&credential_path).expect("sealed credential");
    assert!(!credential
        .windows(b"vault-passphrase".len())
        .any(|part| part == b"vault-passphrase"));
    assert!(local_dev_unlock_key_path(&vault_path).exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&credential_path)
                .expect("credential mode")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    for args in [vec!["secrets", "get", "api_token"], vec!["serve"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(&args)
            .current_dir(&root)
            .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
            .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
            .stdin(Stdio::null())
            .output()
            .expect("use unlocked vault");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("tok_local_dev"));
    }

    let set = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "set", "api_token"])
        .current_dir(&root)
        .env("EFFIGY_TEST_SECRETS_VALUE", "tok_updated")
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .stdin(Stdio::null())
        .output()
        .expect("set with unlocked vault");
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );

    let doctor = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "doctor", "--json"])
        .current_dir(&root)
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .stdin(Stdio::null())
        .output()
        .expect("doctor with unlocked vault");
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor json");
    assert_eq!(json["result"]["vault_state"]["status"], "unlocked");

    let changed = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "change-passphrase"])
        .current_dir(&root)
        .env("EFFIGY_TEST_SECRETS_NEW_PASSPHRASE", "new-passphrase")
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .stdin(Stdio::null())
        .output()
        .expect("change passphrase with unlocked vault");
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let get = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "get", "api_token"])
        .current_dir(&root)
        .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
        .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
        .stdin(Stdio::null())
        .output()
        .expect("get after passphrase change");
    assert!(
        get.status.success(),
        "{}",
        String::from_utf8_lossy(&get.stderr)
    );
    assert!(String::from_utf8_lossy(&get.stdout).contains("tok_updated"));

    let lock = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "lock"])
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .expect("lock vault");
    assert!(
        lock.status.success(),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    assert!(!credential_path.exists());
    assert!(!local_dev_unlock_key_path(&vault_path).exists());
    for args in [
        vec!["secrets", "get", "api_token"],
        vec!["serve"],
        vec!["dev"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(&args)
            .current_dir(&root)
            .env_remove("EFFIGY_TEST_SECRETS_PASSPHRASE")
            .env_remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE")
            .stdin(Stdio::null())
            .output()
            .expect("use locked vault");
        assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
    }
}

fn write_fixture(root: &std::path::Path, with_local_dev_unlock: bool) -> std::path::PathBuf {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.dev]
run = "printf %s \"$API_TOKEN\""

[tasks.serve]
run = "printf %s \"$API_TOKEN\""
"#,
    )
    .expect("write manifest");

    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "api_token".to_owned(),
        VaultSecretRecord::new(SecretValue::new("tok_local_dev")),
    );
    let local_dev_key = with_local_dev_unlock
        .then(|| LocalDevUnlockKey::generate().expect("generate local-dev key"));
    let envelope = match &local_dev_key {
        Some(key) => payload
            .encrypt_with_passphrase_and_local_dev_key("vault-passphrase", key)
            .expect("encrypt vault"),
        None => payload
            .encrypt_with_passphrase("vault-passphrase")
            .expect("encrypt legacy vault"),
    };
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        &vault_path,
        envelope.to_json_pretty().expect("serialize vault"),
    )
    .expect("write vault");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&vault_path, fs::Permissions::from_mode(0o600)).expect("secure vault");
    }
    if let Some(key) = local_dev_key {
        let key_path = local_dev_unlock_key_path(&vault_path);
        fs::write(&key_path, key.expose()).expect("write local-dev key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
                .expect("secure local-dev key");
        }
    }
    vault_path
}

#[test]
fn cli_and_rhai_concurrent_vault_writers_preserve_both_updates() {
    let fixture = tempfile::tempdir().expect("fresh fixture");
    let root = fixture.path().join("repo");
    let home = fixture.path().join("home");
    fs::create_dir_all(&root).expect("repo");
    fs::create_dir(&home).expect("HOME");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"
[secrets.vault]
path = ".effigy/secrets/local.vault"
[secrets.keys.cli_token]
required = false
targets = ["rhai"]
[secrets.keys.rhai_token]
required = false
targets = ["rhai"]
[tasks.write-rhai]
run = [{ rhai = "write-rhai.rhai" }]
"#,
    )
    .expect("manifest");
    fs::write(
        root.join("write-rhai.rhai"),
        r#"secrets::set("rhai_token", "from-rhai");"#,
    )
    .expect("script");
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .current_dir(&root)
            .env("HOME", &home)
            .env("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        command
    };
    let output = command().args(["secrets", "init"]).output().expect("init");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first = command()
        .args(["secrets", "set", "cli_token"])
        .env("EFFIGY_TEST_SECRETS_VALUE", "from-cli")
        .spawn()
        .expect("CLI writer");
    let second = command().arg("write-rhai").spawn().expect("Rhai writer");
    for child in [first, second] {
        let output = child.wait_with_output().expect("writer exits");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let path = root.join(".effigy/secrets/local.vault");
    let payload = effigy_secrets::storage::read_envelope(&path)
        .expect("selected vault")
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt");
    assert_eq!(payload.records["cli_token"].value.expose(), "from-cli");
    assert_eq!(payload.records["rhai_token"].value.expose(), "from-rhai");
}
