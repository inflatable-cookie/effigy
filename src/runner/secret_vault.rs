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

/// The vault file, with the failure every reader reports the same way.
fn read_vault_envelope(vault_path: &Path) -> Result<effigy_secrets::VaultEnvelope, RunnerError> {
    effigy_secrets::storage::read_envelope(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not read the vault at {}: {error}\nnext: run `effigy secrets unlock` (or `effigy secrets init` when there is no vault yet)",
            vault_path.display()
        ))
    })
}

pub(in crate::runner) fn read_effigy_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RunnerError> {
    let envelope = read_vault_envelope(vault_path)?;
    envelope.decrypt_with_passphrase(passphrase).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not decrypt the vault at {}: {error}\nnext: check the passphrase with `effigy secrets unlock`, or `effigy secrets change-passphrase` if it was rotated",
            vault_path.display()
        ))
    })
}

/// Read the vault with the local-dev key, or `None` when this machine has not
/// minted one yet. The vault is read first, so a corrupt vault fails whether or
/// not a key exists; only an absent key path (`NotFound` from its own symlink
/// metadata) is `None`, and an unusable one keeps its error.
pub(in crate::runner) fn read_effigy_vault_payload_with_local_dev_key(
    vault_path: &Path,
) -> Result<Option<VaultPlaintextPayload>, RunnerError> {
    let envelope = read_vault_envelope(vault_path)?;
    let key_path = local_dev_unlock_key_path(vault_path);
    match fs::symlink_metadata(&key_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(local_dev_key_error(&key_path, error)),
    }
    match inspect_vault_permissions(&key_path) {
        Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
        Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => {
            return Err(RunnerError::task_invocation(format!(
                "the local-dev unlock key {} is readable by other users: mode {mode:o}, expected at most {max_mode:o}\nnext: run `effigy secrets lock` then `effigy secrets unlock` to mint a private key",
                key_path.display()
            )));
        }
        Err(error) => return Err(local_dev_key_error(&key_path, error)),
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
    let payload = envelope.decrypt_with_local_dev_key(&key).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not decrypt the vault at {} with the local-dev key: {error}\nnext: run `effigy secrets unlock` again; if the vault's passphrase changed, run `effigy secrets lock` first",
            vault_path.display()
        ))
    })?;
    Ok(Some(payload))
}

fn local_dev_key_error(key_path: &Path, error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(format!(
        "could not inspect the local-dev unlock key {}: {error}\nnext: check the vault directory's ownership and permissions, or run `effigy secrets lock` then `effigy secrets unlock`",
        key_path.display()
    ))
}

pub(in crate::runner) fn read_effigy_vault_payload_for_local_dev(
    vault_path: &Path,
) -> Result<VaultPlaintextPayload, RunnerError> {
    read_effigy_vault_payload_with_local_dev_key(vault_path)?.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "this vault has no local-dev unlock key at {}\nnext: run `effigy secrets unlock` with the passphrase to mint one",
            local_dev_unlock_key_path(vault_path).display()
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

    /// A vault this checkout could open with its passphrase, written where the
    /// task-secret reader looks. Its local-dev key is what each test adds.
    fn write_valid_vault(root: &std::path::Path) -> std::path::PathBuf {
        let vault = root.join(VAULT_RELATIVE);
        fs::create_dir_all(vault.parent().expect("vault parent")).expect("vault dir");
        let envelope = effigy_secrets::VaultPlaintextPayload::empty()
            .encrypt_with_passphrase("vault-passphrase")
            .expect("encrypt fixture vault");
        fs::write(
            &vault,
            envelope.to_json_pretty().expect("serialize fixture"),
        )
        .expect("write fixture vault");
        vault
    }

    /// A machine that has not minted a local-dev key is a distinct, expected
    /// state: it is the one case an interactive upgrade answers.
    #[test]
    fn a_vault_without_a_local_dev_key_is_a_distinct_outcome() {
        let root = TempDir::new().expect("tempdir");
        let vault = write_valid_vault(root.path());

        let read = super::read_effigy_vault_payload_with_local_dev_key(&vault)
            .expect("a missing local-dev key is not a read failure");
        assert!(
            read.is_none(),
            "the absence of a key is None, not a payload"
        );

        let message = super::read_effigy_vault_payload_for_local_dev(&vault)
            .expect_err("the plain read still refuses")
            .to_string();
        assert!(
            message.contains("no local-dev unlock key"),
            "the error names the missing key: {message}"
        );
    }

    /// A vault this machine cannot read is a failure even when no local-dev key
    /// exists: hiding it behind "no key yet" would turn a corrupt vault into an
    /// interactive upgrade prompt, or silently omit an optional secret.
    #[test]
    fn a_corrupt_vault_fails_even_without_a_local_dev_key() {
        let root = TempDir::new().expect("tempdir");
        let vault = root.path().join(VAULT_RELATIVE);
        fs::create_dir_all(vault.parent().expect("vault parent")).expect("vault dir");
        fs::write(&vault, "not a vault").expect("vault");

        let message = super::read_effigy_vault_payload_with_local_dev_key(&vault)
            .expect_err("a vault that cannot be read is this machine's failure")
            .to_string();
        assert!(
            message.contains(&vault.display().to_string()),
            "the error names the vault it could not read: {message}"
        );
        assert!(
            message.contains("next: run `effigy secrets unlock`"),
            "the error says what to do next: {message}"
        );
    }

    /// A key that exists but cannot be used is a real failure. Treating it as
    /// "no key yet" would hide an unsafe or unreadable key behind an
    /// interactive upgrade.
    #[test]
    #[cfg(unix)]
    fn an_unusable_local_dev_key_is_an_error_not_a_missing_key() {
        use std::os::unix::fs::PermissionsExt;

        let root = TempDir::new().expect("tempdir");
        let vault = write_valid_vault(root.path());
        let key_path = effigy_secrets::local_dev_unlock_key_path(&vault);
        fs::write(&key_path, "not a key").expect("local-dev key");
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644))
            .expect("make the key readable beyond its owner");

        let message = super::read_effigy_vault_payload_with_local_dev_key(&vault)
            .expect_err("an unsafe key is a failure")
            .to_string();
        assert!(
            message.contains("readable by other users"),
            "the error names the key's permissions: {message}"
        );
        assert!(
            message.contains("next: run `effigy secrets lock`"),
            "the error says what to do next: {message}"
        );
    }

    /// A key path that cannot even be inspected is an error, not the absence of
    /// a key. `Path::exists` answers `false` for a failed lookup, so a
    /// self-referential key path is exactly the case it would hide.
    #[test]
    #[cfg(unix)]
    fn a_key_path_that_cannot_be_inspected_is_not_a_missing_key() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().expect("tempdir");
        let vault = write_valid_vault(root.path());
        let key_path = effigy_secrets::local_dev_unlock_key_path(&vault);
        symlink(&key_path, &key_path).expect("self-referential key path");

        let message = super::read_effigy_vault_payload_with_local_dev_key(&vault)
            .expect_err("an uninspectable key is a failure")
            .to_string();
        assert!(
            message.contains("could not inspect the local-dev unlock key"),
            "the error names the key it could not inspect: {message}"
        );
    }

    /// The key path itself is absent only when nothing exists at it. A link
    /// whose target is gone is a key this machine cannot use, and its error
    /// must not read as "no key yet" (which would start an upgrade).
    #[test]
    #[cfg(unix)]
    fn a_dangling_local_dev_key_link_is_not_a_missing_key() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().expect("tempdir");
        let vault = write_valid_vault(root.path());
        let key_path = effigy_secrets::local_dev_unlock_key_path(&vault);
        let missing = root.path().join("no-such-key");
        symlink(&missing, &key_path).expect("dangling key link");

        let message = super::read_effigy_vault_payload_with_local_dev_key(&vault)
            .expect_err("a key link with no target is a failure")
            .to_string();
        assert!(
            message.contains("could not inspect the local-dev unlock key"),
            "the error names the key it could not use: {message}"
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
