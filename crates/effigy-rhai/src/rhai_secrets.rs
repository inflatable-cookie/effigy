use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig, TASK_MANIFEST_FILE,
};
use effigy_secrets::{
    inspect_vault_permissions, local_dev_unlock_key_path, LocalDevUnlockKey, SecretValue,
    VaultEnvelope, VaultPermissionStatus, VaultPlaintextPayload, VaultSecretRecord,
};
use rhai::{EvalAltResult, ImmutableString, Map};

use crate::{RhaiHostError, RhaiSecretStore, RhaiSecretTarget};

pub(crate) fn with_rhai_secret_store<T>(store: RhaiSecretStore, run: impl FnOnce() -> T) -> T {
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let previous = active.replace(Some(store));
        let output = run();
        active.replace(previous);
        output
    })
}

pub(crate) fn resolve_rhai_secret_store(
    repo_root: &Path,
    secret_targets: &[RhaiSecretTarget],
) -> Result<RhaiSecretStore, RhaiHostError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(RhaiSecretStore::default());
    }
    let manifest = effigy_manifest::load_task_manifest(&manifest_path)
        .map_err(|error| RhaiHostError::new(error.to_string()))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(RhaiSecretStore::default());
    };
    let allowed_targets = secret_targets
        .iter()
        .copied()
        .map(RhaiSecretTarget::manifest_target)
        .collect::<Vec<_>>();
    let mut store = RhaiSecretStore::default();
    for (name, key) in &secrets.keys {
        if key
            .targets
            .iter()
            .any(|target| allowed_targets.contains(target))
        {
            store.declared_rhai.insert(name.clone());
        } else {
            store.declared_other_target.insert(name.clone());
        }
    }
    if store.declared_rhai.is_empty() {
        return Ok(store);
    }

    let required = secrets
        .keys
        .iter()
        .filter(|(_, key)| {
            key.required
                && key
                    .targets
                    .iter()
                    .any(|target| allowed_targets.contains(target))
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required.is_empty() {
            return Ok(store);
        }
        return Err(RhaiHostError::new(
            "required Rhai secrets need `[secrets].backend = \"effigy-vault\"`",
        ));
    }

    if !required.is_empty() {
        let vault_path = resolve_shared_rhai_secret_vault_path(repo_root, secrets)?;
        if !vault_path.exists() {
            return Err(RhaiHostError::new(format!(
                "required Rhai secrets are declared but the vault is missing at {}",
                vault_path.display()
            )));
        }

        let (payload, passphrase) = if local_dev_secret_access_active() {
            (read_rhai_local_dev_vault_payload(&vault_path)?, None)
        } else {
            let passphrase = read_rhai_secret_passphrase(false)?
                .ok_or_else(|| RhaiHostError::new("vault passphrase is required"))?;
            let payload = read_rhai_secret_vault_payload(&vault_path, passphrase.expose())?;
            (payload, Some(passphrase))
        };
        store.unlocked_passphrase = passphrase;
        store.vault_loaded = true;
        for name in &store.declared_rhai {
            if let Some(record) = payload.records.get(name) {
                store
                    .values
                    .insert(name.clone(), record.value.expose().to_owned());
            }
        }

        let missing_required = required
            .into_iter()
            .filter(|name| !store.values.contains_key(name))
            .collect::<Vec<_>>();
        if !missing_required.is_empty() {
            return Err(RhaiHostError::new(format!(
                "required Rhai secret(s) missing from the vault: {}",
                missing_required.join(", ")
            )));
        }
    }

    Ok(store)
}

fn resolve_rhai_secret_vault_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<PathBuf, RhaiHostError> {
    let declared = declared_rhai_secret_vault_path(secrets)?;
    if declared.is_absolute() {
        Ok(declared)
    } else {
        Ok(repo_root.join(declared))
    }
}

/// The vault every Rhai secret operation acts on, shared across worktrees.
///
/// The local vault is machine-local state outside version control, so a fresh
/// `git worktree` has none. Reads *and* mutations resolve through here so a
/// linked worktree operates on the one machine-local vault: resolving reads
/// and writes differently would let `secrets::set` fork a partial worktree
/// vault that then shadows every primary-only record.
///
/// Only vault *creation* uses [`resolve_rhai_secret_vault_path`] directly.
fn resolve_shared_rhai_secret_vault_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<PathBuf, RhaiHostError> {
    let declared = declared_rhai_secret_vault_path(secrets)?;
    let resolved = resolve_rhai_secret_vault_path(repo_root, secrets)?;
    if resolved.exists() || declared.is_absolute() {
        return Ok(resolved);
    }
    Ok(
        effigy_core::git_worktree::primary_checkout_fallback(repo_root, &declared)
            .unwrap_or(resolved),
    )
}

fn declared_rhai_secret_vault_path(
    secrets: &ManifestSecretsConfig,
) -> Result<PathBuf, RhaiHostError> {
    let vault = secrets.vault.as_ref().ok_or_else(|| {
        RhaiHostError::new("`[secrets]` selects `effigy-vault` but `[secrets.vault]` is missing")
    })?;
    let path = vault
        .path
        .as_deref()
        .ok_or_else(|| RhaiHostError::new("`[secrets.vault].path` is required for Rhai secrets"))?;
    Ok(PathBuf::from(path))
}

fn read_rhai_secret_passphrase(optional_only: bool) -> Result<Option<SecretValue>, RhaiHostError> {
    if let Ok(value) = std::env::var("EFFIGY_TEST_SECRETS_PASSPHRASE") {
        return Ok(Some(SecretValue::new(value)));
    }
    if let Ok(value) = std::env::var("EFFIGY_INTERNAL_SECRET_PASSPHRASE") {
        return Ok(Some(SecretValue::new(value)));
    }
    if !std::io::stdin().is_terminal() {
        if optional_only {
            return Ok(None);
        }
        return Err(RhaiHostError::new(
            "Rhai secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
        ));
    }
    let value = rpassword::prompt_password("Vault passphrase: ")
        .map_err(|error| RhaiHostError::new(format!("failed to read secret input: {error}")))?;
    Ok(Some(SecretValue::new(value)))
}

fn read_rhai_secret_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RhaiHostError> {
    let raw = std::fs::read_to_string(vault_path).map_err(|error| {
        RhaiHostError::new(format!(
            "failed to read vault {}: {error}",
            vault_path.display()
        ))
    })?;
    let envelope =
        VaultEnvelope::from_json(&raw).map_err(|error| RhaiHostError::new(error.to_string()))?;
    envelope
        .decrypt_with_passphrase(passphrase)
        .map_err(|error| RhaiHostError::new(error.to_string()))
}

fn local_dev_secret_access_active() -> bool {
    std::env::var_os("EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS").is_some()
}

fn read_rhai_local_dev_vault_payload(
    vault_path: &Path,
) -> Result<VaultPlaintextPayload, RhaiHostError> {
    let raw = std::fs::read_to_string(vault_path).map_err(|error| {
        RhaiHostError::new(format!(
            "failed to read vault {}: {error}",
            vault_path.display()
        ))
    })?;
    let envelope =
        VaultEnvelope::from_json(&raw).map_err(|error| RhaiHostError::new(error.to_string()))?;
    let key_path = local_dev_unlock_key_path(vault_path);
    match inspect_vault_permissions(&key_path) {
        Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
        Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => {
            return Err(RhaiHostError::new(format!(
                "local-dev unlock key permissions are unsafe: mode {mode:o}, expected at most {max_mode:o}"
            )));
        }
        Err(error) => {
            return Err(RhaiHostError::new(format!(
                "failed to inspect local-dev unlock key {}: {error}",
                key_path.display()
            )));
        }
    }
    let key = std::fs::read(&key_path).map_err(|error| {
        RhaiHostError::new(format!(
            "failed to read local-dev unlock key {}: {error}",
            key_path.display()
        ))
    })?;
    let key = LocalDevUnlockKey::from_bytes(key)
        .map_err(|error| RhaiHostError::new(error.to_string()))?;
    envelope
        .decrypt_with_local_dev_key(&key)
        .map_err(|error| RhaiHostError::new(error.to_string()))
}

pub(crate) fn active_rhai_secret(
    repo_root: &Path,
    name: &str,
) -> Result<String, Box<EvalAltResult>> {
    active_rhai_validate_secret_name(name)?;
    active_rhai_load_vault_if_needed(repo_root, true)
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let store = active.borrow();
        let store = store
            .as_ref()
            .ok_or_else(|| crate::rhai_runtime_error("Rhai secret store is not active"))?;
        store.values.get(name).cloned().ok_or_else(|| {
            crate::rhai_runtime_error(format!("Rhai secret `{name}` is missing from the vault"))
        })
    })
}

pub(crate) fn active_rhai_has_secret(
    repo_root: &Path,
    name: &str,
) -> Result<bool, Box<EvalAltResult>> {
    active_rhai_validate_secret_name(name)?;
    active_rhai_load_vault_if_needed(repo_root, false)
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let store = active.borrow();
        let store = store
            .as_ref()
            .ok_or_else(|| crate::rhai_runtime_error("Rhai secret store is not active"))?;
        Ok(store.values.contains_key(name))
    })
}

fn active_rhai_validate_secret_name(name: &str) -> Result<(), Box<EvalAltResult>> {
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let store = active.borrow();
        let store = store
            .as_ref()
            .ok_or_else(|| crate::rhai_runtime_error("Rhai secret store is not active"))?;
        if !store.declared_rhai.contains(name) {
            if store.declared_other_target.contains(name) {
                return Err(crate::rhai_runtime_error(format!(
                    "secret `{name}` is not declared for the `rhai` target"
                )));
            }
            return Err(crate::rhai_runtime_error(format!(
                "secret `{name}` is not declared under `[secrets.keys]`"
            )));
        }
        Ok(())
    })
}

fn active_rhai_load_vault_if_needed(
    repo_root: &Path,
    require_unlock: bool,
) -> Result<(), RhaiHostError> {
    let should_load = super::ACTIVE_RHAI_SECRETS.with(|active| {
        let store = active.borrow();
        let store = store
            .as_ref()
            .ok_or_else(|| RhaiHostError::new("Rhai secret store is not active"))?;
        Ok(!store.vault_loaded)
    })?;
    if !should_load {
        return Ok(());
    }

    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let manifest = effigy_manifest::load_task_manifest(&manifest_path)
        .map_err(|error| RhaiHostError::new(error.to_string()))?;
    let secrets = manifest
        .secrets
        .as_ref()
        .ok_or_else(|| RhaiHostError::new("`[secrets]` is not declared"))?;
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        return Err(RhaiHostError::new(
            "Rhai secrets require `[secrets].backend = \"effigy-vault\"`",
        ));
    }
    let vault_path = resolve_shared_rhai_secret_vault_path(repo_root, secrets)?;
    if !vault_path.exists() {
        if require_unlock {
            return Err(RhaiHostError::new(format!(
                "secrets vault is missing at {}",
                vault_path.display()
            )));
        }
        super::ACTIVE_RHAI_SECRETS.with(|active| {
            if let Some(store) = active.borrow_mut().as_mut() {
                store.vault_loaded = true;
            }
        });
        return Ok(());
    }

    let (payload, passphrase) = if local_dev_secret_access_active() {
        (read_rhai_local_dev_vault_payload(&vault_path)?, None)
    } else {
        let Some(passphrase) = read_rhai_secret_passphrase(!require_unlock)? else {
            super::ACTIVE_RHAI_SECRETS.with(|active| {
                if let Some(store) = active.borrow_mut().as_mut() {
                    store.vault_loaded = true;
                }
            });
            return Ok(());
        };
        let payload = read_rhai_secret_vault_payload(&vault_path, passphrase.expose())?;
        (payload, Some(passphrase))
    };
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        if let Some(store) = active.borrow_mut().as_mut() {
            for name in &store.declared_rhai {
                if let Some(record) = payload.records.get(name) {
                    store
                        .values
                        .insert(name.clone(), record.value.expose().to_owned());
                }
            }
            store.unlocked_passphrase = passphrase;
            store.vault_loaded = true;
        }
    });
    Ok(())
}

pub(crate) fn active_rhai_set_secret(
    repo_root: &Path,
    name: &str,
    value: &str,
) -> Result<(), Box<EvalAltResult>> {
    active_rhai_set_secret_records(repo_root, vec![(name.to_owned(), SecretValue::new(value))])
}

pub(crate) fn active_rhai_set_secrets(
    repo_root: &Path,
    values: Map,
) -> Result<(), Box<EvalAltResult>> {
    let records = values
        .into_iter()
        .map(|(name, value)| {
            let Some(value) = value.try_cast::<ImmutableString>() else {
                return Err(crate::rhai_runtime_error(format!(
                    "secret `{name}` value must be a string"
                )));
            };
            Ok((name.to_string(), SecretValue::new(value.as_str())))
        })
        .collect::<Result<Vec<_>, Box<EvalAltResult>>>()?;
    active_rhai_set_secret_records(repo_root, records)
}

fn active_rhai_set_secret_records(
    repo_root: &Path,
    records: impl IntoIterator<Item = (String, SecretValue)>,
) -> Result<(), Box<EvalAltResult>> {
    let records = records.into_iter().collect::<Vec<_>>();
    if records.is_empty() {
        return Ok(());
    }

    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let manifest = effigy_manifest::load_task_manifest(&manifest_path)
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    let secrets = manifest
        .secrets
        .as_ref()
        .ok_or_else(|| crate::rhai_runtime_error("`[secrets]` is not declared"))?;
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        return Err(crate::rhai_runtime_error(
            "secret mutation requires `[secrets].backend = \"effigy-vault\"`",
        ));
    }
    for (name, _) in &records {
        let Some(key) = secrets.keys.get(name) else {
            return Err(crate::rhai_runtime_error(format!(
                "secret `{name}` is not declared under `[secrets.keys]`"
            )));
        };
        if !key.targets.contains(&ManifestSecretTarget::Rhai) {
            return Err(crate::rhai_runtime_error(format!(
                "secret `{name}` is not declared for the `rhai` target"
            )));
        }
    }

    let vault_path = resolve_shared_rhai_secret_vault_path(repo_root, secrets)
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
    let passphrase = match active_rhai_unlocked_passphrase()
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?
    {
        Some(passphrase) => passphrase,
        None => read_rhai_secret_passphrase(false)
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?
            .ok_or_else(|| crate::rhai_runtime_error("vault passphrase is required"))?,
    };
    let mut payload = if vault_path.exists() {
        read_rhai_secret_vault_payload(&vault_path, passphrase.expose())
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?
    } else {
        VaultPlaintextPayload::empty()
    };
    for (name, value) in &records {
        payload
            .records
            .insert(name.clone(), VaultSecretRecord::new(value.clone()));
    }
    let local_dev_key_path = local_dev_unlock_key_path(&vault_path);
    let envelope = match std::fs::read(&local_dev_key_path) {
        Ok(bytes) => {
            let key = LocalDevUnlockKey::from_bytes(bytes)
                .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;
            payload
                .encrypt_with_passphrase_and_local_dev_key(passphrase.expose(), &key)
                .map_err(|error| crate::rhai_runtime_error(error.to_string()))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => payload
            .encrypt_with_passphrase(passphrase.expose())
            .map_err(|error| crate::rhai_runtime_error(error.to_string()))?,
        Err(error) => {
            return Err(crate::rhai_runtime_error(format!(
                "failed to read local-dev unlock key {}: {error}",
                local_dev_key_path.display()
            )));
        }
    };
    write_rhai_secret_vault_file(&vault_path, &envelope)
        .map_err(|error| crate::rhai_runtime_error(error.to_string()))?;

    super::ACTIVE_RHAI_SECRETS.with(|active| {
        if let Some(store) = active.borrow_mut().as_mut() {
            store.unlocked_passphrase = Some(passphrase);
            for (name, value) in records {
                store.values.insert(name, value.expose().to_owned());
            }
        }
    });

    Ok(())
}

fn active_rhai_unlocked_passphrase() -> Result<Option<SecretValue>, RhaiHostError> {
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let store = active.borrow();
        let store = store
            .as_ref()
            .ok_or_else(|| RhaiHostError::new("Rhai secret store is not active"))?;
        Ok(store.unlocked_passphrase.clone())
    })
}

fn write_rhai_secret_vault_file(
    vault_path: &Path,
    envelope: &VaultEnvelope,
) -> Result<(), RhaiHostError> {
    if let Some(parent) = vault_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            RhaiHostError::new(format!(
                "failed to create vault directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let rendered = envelope
        .to_json_pretty()
        .map_err(|error| RhaiHostError::new(error.to_string()))?;
    std::fs::write(vault_path, rendered).map_err(|error| {
        RhaiHostError::new(format!(
            "failed to write vault {}: {error}",
            vault_path.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(vault_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub(crate) fn redact_active_rhai_secrets(input: &str) -> String {
    super::ACTIVE_RHAI_SECRETS.with(|active| {
        let Some(store) = active.borrow().as_ref().cloned() else {
            return input.to_owned();
        };
        let mut redacted = input.to_owned();
        for value in store.values.values() {
            if !value.is_empty() {
                redacted = redacted.replace(value, "[REDACTED]");
            }
        }
        redacted
    })
}
