use std::fs;
use std::process::{Command, Stdio};

use effigy_secrets::{
    local_dev_unlock_key_path, LocalDevUnlockKey, SecretValue, VaultPlaintextPayload,
    VaultSecretRecord,
};

#[test]
fn dev_uses_local_unlock_but_direct_get_still_requires_passphrase() {
    let root = tempfile::tempdir().expect("tempdir");
    write_fixture(root.path(), true);

    let dev = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .current_dir(root.path())
        .stdin(Stdio::null())
        .output()
        .expect("run dev");
    assert!(
        dev.status.success(),
        "dev failed: {}",
        String::from_utf8_lossy(&dev.stderr)
    );

    let direct_get = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["secrets", "get", "api_token"])
        .current_dir(root.path())
        .stdin(Stdio::null())
        .output()
        .expect("run direct get");
    assert!(!direct_get.status.success());
    let stderr = String::from_utf8_lossy(&direct_get.stderr);
    assert!(
        stderr.contains("secret input requires an interactive TTY"),
        "unexpected direct-get error: {stderr}"
    );
    assert!(!stderr.contains("tok_local_dev"));

    let other_task = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("serve")
        .current_dir(root.path())
        .stdin(Stdio::null())
        .output()
        .expect("run non-dev task");
    assert!(!other_task.status.success());
    assert!(String::from_utf8_lossy(&other_task.stderr)
        .contains("secret input requires an interactive TTY"));
}

#[test]
fn first_dev_unlock_upgrades_a_legacy_vault() {
    let root = tempfile::tempdir().expect("tempdir");
    let vault_path = write_fixture(root.path(), false);

    let upgrade = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("dev")
        .current_dir(root.path())
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
        .current_dir(root.path())
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

fn write_fixture(root: &std::path::Path, with_local_dev_unlock: bool) -> std::path::PathBuf {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

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
