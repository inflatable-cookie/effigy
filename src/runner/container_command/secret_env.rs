use std::path::Path;

use effigy_env::secret::SecretString;
use effigy_manifest::{ManifestSecretTarget, ManifestSecretsBackend};

use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest;

pub(super) fn resolve_container_secret_env(
    repo_root: &Path,
    force_required: bool,
) -> Result<Vec<(String, SecretString)>, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join("effigy.toml"))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(Vec::new());
    };
    let container_keys = secrets
        .keys
        .iter()
        .filter(|(_, key)| key.targets.contains(&ManifestSecretTarget::Containers))
        .collect::<Vec<_>>();
    if container_keys.is_empty() {
        return Ok(Vec::new());
    }

    let required_names = container_keys
        .iter()
        .filter(|(_, key)| force_required || key.required)
        .map(|(name, _)| (*name).clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        return Err(RunnerError::task_invocation(
            "required container secrets need `[secrets].backend = \"effigy-vault\"`",
        ));
    }

    let vault_path = crate::runner::secret_vault::resolve_effigy_vault_path(
        repo_root,
        secrets,
        "container secret injection",
    )?;
    if !vault_path.exists() {
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        return Err(RunnerError::task_invocation(format!(
            "required container secrets are declared but the vault is missing at {}",
            vault_path.display()
        )));
    }

    let Some(passphrase) = crate::runner::secret_session::read_secret_passphrase(
        required_names.is_empty(),
        "Vault passphrase: ",
        "container secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
    )? else {
        return Ok(Vec::new());
    };
    let payload =
        crate::runner::secret_vault::read_effigy_vault_payload(&vault_path, passphrase.expose())?;
    let mut injected = Vec::new();
    let mut missing_required = Vec::new();
    for (name, key) in container_keys {
        match payload.records.get(name.as_str()) {
            Some(record) => injected.push((
                container_secret_env_name(name),
                SecretString::new(record.value.expose().to_owned()),
            )),
            None if force_required || key.required => missing_required.push(name.to_owned()),
            None => {}
        }
    }
    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required container secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }
    Ok(injected)
}

fn container_secret_env_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::resolve_container_secret_env;
    use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn container_secret_env_resolves_declared_container_target_values() {
        let root = temp_repo("container-secret-env");
        fs::write(
            root.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["containers"]
"#,
        )
        .expect("write manifest");
        write_test_vault(
            &root,
            "vault-passphrase",
            &[("database_url", "postgres://secret-value")],
        );
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let env = resolve_container_secret_env(&root, false).expect("resolve secrets");

        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "DATABASE_URL");
        assert_eq!(env[0].1.expose(), "postgres://secret-value");
    }

    #[test]
    fn container_secret_env_blocks_missing_required_before_startup() {
        let root = temp_repo("container-secret-missing");
        fs::write(
            root.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["containers"]
"#,
        )
        .expect("write manifest");
        write_test_vault(&root, "vault-passphrase", &[]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let error = resolve_container_secret_env(&root, false).expect_err("missing should fail");

        assert!(error
            .to_string()
            .contains("required container secret(s) missing from the vault"));
    }

    #[test]
    fn container_secret_env_force_required_loads_optional_container_values() {
        let root = temp_repo("container-secret-force-required");
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
required = false
targets = ["containers"]
"#,
        )
        .expect("write manifest");
        write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let env = resolve_container_secret_env(&root, true).expect("resolve secrets");

        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "API_TOKEN");
        assert_eq!(env[0].1.expose(), "tok_secret");
    }

    #[test]
    fn container_secret_env_force_required_blocks_missing_optional_container_values() {
        let root = temp_repo("container-secret-force-required-missing");
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
required = false
targets = ["containers"]
"#,
        )
        .expect("write manifest");
        write_test_vault(&root, "vault-passphrase", &[]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let error = resolve_container_secret_env(&root, true).expect_err("missing should fail");

        assert!(error
            .to_string()
            .contains("required container secret(s) missing from the vault: api_token"));
    }

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-secret-env-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    struct ScopedEnvVar {
        key: String,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key: key.to_owned(),
                previous,
            }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(&self.key, previous);
                } else {
                    std::env::remove_var(&self.key);
                }
            }
        }
    }

    fn write_test_vault(root: &Path, passphrase: &str, records: &[(&str, &str)]) {
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
}
