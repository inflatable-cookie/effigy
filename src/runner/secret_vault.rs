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
    let declared = declared_vault_path(secrets, purpose)?;
    if declared.is_absolute() {
        Ok(declared)
    } else {
        Ok(repo_root.join(declared))
    }
}

/// Vault path for *reading* secrets, shared across linked worktrees.
///
/// The local vault deliberately lives outside version control, so a freshly
/// created `git worktree` starts without one and every secret-backed task in
/// it fails — even though the same machine already holds an unlocked vault in
/// the primary checkout. Reads fall back to that vault rather than inventing a
/// second secrets backend. Writes keep using
/// [`resolve_effigy_vault_path`]: authoring a vault stays where the caller
/// asked for it.
pub(in crate::runner) fn resolve_effigy_vault_read_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
    purpose: &str,
) -> Result<PathBuf, RunnerError> {
    let declared = declared_vault_path(secrets, purpose)?;
    let resolved = resolve_effigy_vault_path(repo_root, secrets, purpose)?;
    if resolved.exists() || declared.is_absolute() {
        return Ok(resolved);
    }
    if let Some(shared) = effigy_core::git_worktree::primary_checkout_fallback(repo_root, &declared)
    {
        return Ok(shared);
    }
    // Still worth saying where a shared vault would have to live: the caller's
    // own "vault is missing" error only names this worktree.
    if let Some(primary) = effigy_core::git_worktree::detect_linked_worktree(repo_root)
        .and_then(|layout| layout.primary_checkout_root)
    {
        eprintln!(
            "[warn] no local vault for {purpose} at {} and none in the primary checkout {}; run `effigy secrets init` there so registered worktrees share one vault",
            resolved.display(),
            primary.display()
        );
    }
    Ok(resolved)
}

fn declared_vault_path(
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
    Ok(PathBuf::from(path))
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

#[cfg(test)]
mod tests {
    use std::fs;

    use effigy_manifest::config_sections::{ManifestSecretsConfig, ManifestSecretsVaultConfig};
    use tempfile::TempDir;

    use super::{resolve_effigy_vault_path, resolve_effigy_vault_read_path};

    const VAULT_RELATIVE: &str = ".effigy/secrets/local.vault";

    fn secrets() -> ManifestSecretsConfig {
        ManifestSecretsConfig {
            vault: Some(ManifestSecretsVaultConfig {
                path: Some(VAULT_RELATIVE.to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Primary checkout with a vault, plus a linked worktree that has none.
    fn worktree_fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let root = TempDir::new().expect("tempdir");
        let primary = root.path().join("primary");
        let worktree = root.path().join("wt/feature");
        let worktree_git_dir = primary.join(".git/worktrees/feature");
        fs::create_dir_all(&worktree_git_dir).expect("worktree git dir");
        fs::create_dir_all(&worktree).expect("worktree root");
        fs::write(worktree_git_dir.join("commondir"), "../..\n").expect("commondir");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_git_dir.display()),
        )
        .expect("gitdir pointer");
        (root, primary, worktree)
    }

    #[test]
    fn worktree_read_falls_back_to_the_primary_checkout_vault() {
        let (_root, primary, worktree) = worktree_fixture();
        let primary_vault = primary.join(VAULT_RELATIVE);
        fs::create_dir_all(primary_vault.parent().expect("vault parent")).expect("vault dir");
        fs::write(&primary_vault, "{}").expect("vault");

        let resolved =
            resolve_effigy_vault_read_path(&worktree, &secrets(), "test").expect("read path");

        assert_eq!(resolved, primary_vault);
    }

    #[test]
    fn worktree_read_prefers_the_worktrees_own_vault() {
        let (_root, primary, worktree) = worktree_fixture();
        for root in [&primary, &worktree] {
            let vault = root.join(VAULT_RELATIVE);
            fs::create_dir_all(vault.parent().expect("vault parent")).expect("vault dir");
            fs::write(&vault, "{}").expect("vault");
        }

        let resolved =
            resolve_effigy_vault_read_path(&worktree, &secrets(), "test").expect("read path");

        assert_eq!(resolved, worktree.join(VAULT_RELATIVE));
    }

    #[test]
    fn write_path_never_redirects_to_the_primary_checkout() {
        let (_root, primary, worktree) = worktree_fixture();
        let primary_vault = primary.join(VAULT_RELATIVE);
        fs::create_dir_all(primary_vault.parent().expect("vault parent")).expect("vault dir");
        fs::write(&primary_vault, "{}").expect("vault");

        let resolved = resolve_effigy_vault_path(&worktree, &secrets(), "test").expect("path");

        assert_eq!(resolved, worktree.join(VAULT_RELATIVE));
    }

    #[test]
    fn ordinary_checkout_read_path_is_unchanged() {
        let root = TempDir::new().expect("tempdir");
        fs::create_dir_all(root.path().join(".git")).expect("git dir");

        let resolved =
            resolve_effigy_vault_read_path(root.path(), &secrets(), "test").expect("read path");

        assert_eq!(resolved, root.path().join(VAULT_RELATIVE));
    }
}
