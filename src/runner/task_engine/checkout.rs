//! `CheckoutId` for a resolved Effigy root (contract 052 §5.1).
//!
//! Linked worktrees and ephemeral clones reuse their runtime-scope token, so
//! Compose names never change. A primary Git checkout keeps its own token
//! beside it in its `.git` directory. A root with no `.git` of its own has
//! no per-checkout Git state, so its identity derives from its canonical
//! path. Only Mutate mints a token; Observe reads and reports "no runtime
//! state yet".

use std::io;
use std::path::Path;

use effigy_core::worktree_scope::{self, ScopeKind};
use effigy_kernel::id::{CheckoutId, CheckoutKind, Digest};

use crate::runner::error::RunnerError;

/// The root's checkout identity, minting it on first use (Mutate only).
pub(in crate::runner) fn mint(root: &Path) -> Result<CheckoutId, RunnerError> {
    let io_error = |error: io::Error| {
        RunnerError::task_invocation(format!(
            "cannot record the checkout identity for {}: {error}",
            root.display()
        ))
    };
    if let Some((kind, token)) = worktree_scope::load_or_create_scoped(root).map_err(io_error)? {
        return parse(scoped_kind(kind), &token).map_err(io_error);
    }
    match worktree_scope::load_or_create_checkout_token(root).map_err(io_error)? {
        Some(token) => parse(CheckoutKind::Primary, &token).map_err(io_error),
        None => Ok(path_identity(root)),
    }
}

/// The root's checkout identity without creating anything (R2). `None`
/// means this checkout has never recorded runtime state.
pub(in crate::runner) fn read(root: &Path) -> io::Result<Option<CheckoutId>> {
    if worktree_scope::resolve(root)?.is_some() {
        return worktree_scope::load_scoped(root)?
            .map(|(kind, token)| parse(scoped_kind(kind), &token))
            .transpose();
    }
    if root.join(".git").is_dir() {
        return worktree_scope::load_checkout_token(root)?
            .map(|token| parse(CheckoutKind::Primary, &token))
            .transpose();
    }
    Ok(Some(path_identity(root)))
}

fn scoped_kind(kind: ScopeKind) -> CheckoutKind {
    match kind {
        ScopeKind::LinkedWorktree => CheckoutKind::LinkedWorktree,
        ScopeKind::EphemeralClone => CheckoutKind::EphemeralClone,
    }
}

fn path_identity(root: &Path) -> CheckoutId {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut bytes = b"effigy.checkout.path\0".to_vec();
    bytes.extend_from_slice(canonical.as_os_str().as_encoded_bytes());
    let digest = Digest::of(&bytes);
    let mut token = [0u8; 16];
    token.copy_from_slice(&digest.0[..16]);
    CheckoutId::new(CheckoutKind::Primary, token)
}

fn parse(kind: CheckoutKind, value: &str) -> io::Result<CheckoutId> {
    let value = value.trim();
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid checkout token");
    if value.len() != 32 || !value.is_ascii() {
        return Err(invalid());
    }
    let mut token = [0u8; 16];
    for (index, byte) in token.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(CheckoutId::new(kind, token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_checkout_token_is_read_only_until_minted_and_then_stable() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir(root.path().join(".git")).expect("git dir");
        assert_eq!(read(root.path()).expect("read"), None);
        let token = root
            .path()
            .join(".git")
            .join(worktree_scope::CHECKOUT_TOKEN_FILE);
        assert!(!token.exists(), "observe writes nothing");
        let minted = mint(root.path()).expect("mint");
        assert!(token.is_file());
        assert_eq!(read(root.path()).expect("read"), Some(minted));
        assert_eq!(mint(root.path()).expect("mint again"), minted);
        assert_eq!(minted.kind(), CheckoutKind::Primary);
    }

    #[test]
    fn root_without_git_state_has_a_path_identity_and_writes_nothing() {
        let root = tempfile::tempdir().expect("root");
        let minted = mint(root.path()).expect("mint");
        assert_eq!(read(root.path()).expect("read"), Some(minted));
        assert_eq!(
            std::fs::read_dir(root.path()).expect("list").count(),
            0,
            "no file is written into the checkout"
        );
        let other = tempfile::tempdir().expect("other");
        assert_ne!(mint(other.path()).expect("mint other"), minted);
    }
}
