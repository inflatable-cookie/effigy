use std::fs;
use std::path::{Path, PathBuf};

use effigy_manifest::config_sections::ManifestSecretsConfig;
use effigy_secrets::{
    inspect_vault_permissions, local_dev_unlock_key_path, LocalDevUnlockKey, VaultPermissionStatus,
    VaultPlaintextPayload,
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

/// The vault every command acts on, shared across linked worktrees.
///
/// The local vault deliberately lives outside version control, so a freshly
/// created `git worktree` starts without one and every secret-backed task in
/// it fails — even though the same machine already holds an unlocked vault in
/// the primary checkout. Reads *and* read-modify-writes resolve through here
/// so a worktree operates on the one machine-local vault: resolving reads and
/// writes differently would let a mutation fork a partial worktree vault that
/// then shadows every primary-only record.
///
/// Only vault *creation* (`secrets init`) uses [`resolve_effigy_vault_path`]
/// directly, so authoring a new vault stays where the caller asked for it.
pub(in crate::runner) fn resolve_shared_effigy_vault_path(
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
    let envelope = effigy_secrets::storage::read_envelope(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not read the vault at {}: {error}\nnext: run `effigy secrets unlock` (or `effigy secrets init` when there is no vault yet)",
            vault_path.display()
        ))
    })?;
    envelope.decrypt_with_passphrase(passphrase).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not decrypt the vault at {}: {error}\nnext: check the passphrase with `effigy secrets unlock`, or `effigy secrets change-passphrase` if it was rotated",
            vault_path.display()
        ))
    })
}

pub(in crate::runner) fn read_effigy_vault_payload_for_local_dev(
    vault_path: &Path,
) -> Result<VaultPlaintextPayload, RunnerError> {
    let envelope = effigy_secrets::storage::read_envelope(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not read the vault at {}: {error}\nnext: run `effigy secrets unlock` (or `effigy secrets init` when there is no vault yet)",
            vault_path.display()
        ))
    })?;
    let key_path = local_dev_unlock_key_path(vault_path);
    match inspect_vault_permissions(&key_path) {
        Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
        Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => {
            return Err(RunnerError::task_invocation(format!(
                "the local-dev unlock key {} is readable by other users: mode {mode:o}, expected at most {max_mode:o}\nnext: run `effigy secrets lock` then `effigy secrets unlock` to mint a private key",
                key_path.display()
            )));
        }
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "could not inspect the local-dev unlock key {}: {error}\nnext: check the vault directory's ownership and permissions, or run `effigy secrets lock` then `effigy secrets unlock`",
                key_path.display()
            )));
        }
    }
    let key = fs::read(&key_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not read the local-dev unlock key {}: {error}\nnext: run `effigy secrets unlock` with the passphrase to mint a new key",
            key_path.display()
        ))
    })?;
    let key = LocalDevUnlockKey::from_bytes(key).map_err(|error| {
        RunnerError::task_invocation(format!(
            "the local-dev unlock key {} is not valid: {error}\nnext: run `effigy secrets lock` then `effigy secrets unlock` to mint a new key",
            key_path.display()
        ))
    })?;
    envelope.decrypt_with_local_dev_key(&key).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not decrypt the vault at {} with the local-dev key: {error}\nnext: run `effigy secrets unlock` again; if the vault's passphrase changed, run `effigy secrets lock` first",
            vault_path.display()
        ))
    })
}

pub(in crate::runner) fn begin_vault_write(
    vault_path: &Path,
) -> Result<effigy_secrets::storage::VaultWrite, RunnerError> {
    effigy_secrets::storage::VaultWrite::acquire(vault_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn write_effigy_vault_payload(
    write: &effigy_secrets::storage::VaultWrite,
    payload: &VaultPlaintextPayload,
    passphrase: &str,
) -> Result<(), RunnerError> {
    write
        .write_payload(payload, passphrase)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn write_private_file(
    write: &effigy_secrets::storage::VaultWrite,
    path: &Path,
    bytes: &[u8],
) -> Result<(), RunnerError> {
    write
        .write_private(path, bytes)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use effigy_manifest::config_sections::{ManifestSecretsConfig, ManifestSecretsVaultConfig};
    use tempfile::TempDir;

    use super::{resolve_effigy_vault_path, resolve_shared_effigy_vault_path};

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
            resolve_shared_effigy_vault_path(&worktree, &secrets(), "test").expect("read path");

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
            resolve_shared_effigy_vault_path(&worktree, &secrets(), "test").expect("read path");

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
    fn a_vault_that_is_not_a_vault_names_the_path_and_the_next_action() {
        let root = TempDir::new().expect("tempdir");
        let vault = root.path().join(VAULT_RELATIVE);
        fs::create_dir_all(vault.parent().expect("vault parent")).expect("vault dir");
        fs::write(&vault, "not a vault").expect("vault");

        let error = super::read_effigy_vault_payload(&vault, "passphrase")
            .expect_err("refuse a malformed vault");
        let message = error.to_string();

        assert!(
            message.contains(&vault.display().to_string()),
            "the error names the vault it could not read: {message}"
        );
        assert!(
            message.contains("next: run `effigy secrets unlock`"),
            "the error says what to do next: {message}"
        );
    }

    #[test]
    fn ordinary_checkout_read_path_is_unchanged() {
        let root = TempDir::new().expect("tempdir");
        fs::create_dir_all(root.path().join(".git")).expect("git dir");

        let resolved =
            resolve_shared_effigy_vault_path(root.path(), &secrets(), "test").expect("read path");

        assert_eq!(resolved, root.path().join(VAULT_RELATIVE));
    }
}
