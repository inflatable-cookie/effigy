use std::fs;
use std::path::{Path, PathBuf};

use effigy_manifest::config_sections::ManifestSecretsConfig;
use effigy_secrets::{VaultEnvelope, VaultPlaintextPayload};

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
