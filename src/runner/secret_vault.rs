use std::fs;
use std::path::{Path, PathBuf};

use effigy_manifest::config_sections::ManifestSecretsConfig;
use effigy_secrets::{
    inspect_vault_permissions, local_dev_unlock_key_path, LocalDevUnlockKey, VaultEnvelope,
    VaultPermissionStatus, VaultPlaintextPayload,
};

use crate::runner::error::RunnerError;

pub(in crate::runner) fn resolve_effigy_vault_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
    purpose: &str,
) -> Result<PathBuf, RunnerError> {
    let vault = secrets.vault.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(
            "`[secrets]` selects `effigy-vault` but `[secrets.vault]` is missing",
        )
    })?;
    let path = vault.path.as_deref().ok_or_else(|| {
        RunnerError::task_invocation(format!("`[secrets.vault].path` is required for {purpose}"))
    })?;
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(repo_root.join(path))
    }
}

pub(in crate::runner) fn read_effigy_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RunnerError> {
    let raw = fs::read_to_string(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read vault {}: {error}",
            vault_path.display()
        ))
    })?;
    let envelope = VaultEnvelope::from_json(&raw)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    envelope
        .decrypt_with_passphrase(passphrase)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn read_effigy_vault_payload_for_local_dev(
    vault_path: &Path,
) -> Result<VaultPlaintextPayload, RunnerError> {
    let raw = fs::read_to_string(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read vault {}: {error}",
            vault_path.display()
        ))
    })?;
    let envelope = VaultEnvelope::from_json(&raw)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let key_path = local_dev_unlock_key_path(vault_path);
    match inspect_vault_permissions(&key_path) {
        Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
        Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => {
            return Err(RunnerError::task_invocation(format!(
                "local-dev unlock key permissions are unsafe: mode {mode:o}, expected at most {max_mode:o}"
            )));
        }
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "failed to inspect local-dev unlock key {}: {error}",
                key_path.display()
            )));
        }
    }
    let key = fs::read(&key_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read local-dev unlock key {}: {error}",
            key_path.display()
        ))
    })?;
    let key = LocalDevUnlockKey::from_bytes(key)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    envelope
        .decrypt_with_local_dev_key(&key)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn write_effigy_vault_payload(
    vault_path: &Path,
    payload: &VaultPlaintextPayload,
    passphrase: &str,
) -> Result<(), RunnerError> {
    let key_path = local_dev_unlock_key_path(vault_path);
    let key = match fs::read(&key_path) {
        Ok(bytes) => LocalDevUnlockKey::from_bytes(bytes)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LocalDevUnlockKey::generate()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "failed to read local-dev unlock key {}: {error}",
                key_path.display()
            )));
        }
    };
    let envelope = payload
        .encrypt_with_passphrase_and_local_dev_key(passphrase, &key)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_private_file(&key_path, key.expose())?;
    let rendered = envelope
        .to_json_pretty()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_private_file(vault_path, rendered.as_bytes())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create vault directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    write_private_file_inner(path, bytes)
}

#[cfg(unix)]
fn write_private_file_inner(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write private vault state {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write private vault state {}: {error}",
            path.display()
        ))
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to secure private vault state {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn write_private_file_inner(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    fs::write(path, bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write private vault state {}: {error}",
            path.display()
        ))
    })
}
