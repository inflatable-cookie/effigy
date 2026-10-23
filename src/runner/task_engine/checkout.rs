//! `CheckoutId` for a resolved Effigy root (contract 052 §5.1).
//!
//! Linked worktrees and ephemeral clones reuse their runtime-scope token, so
//! Compose names never change. A primary Git checkout keeps its own token
//! beside it in its `.git` directory. A root with no `.git` of its own has
//! no per-checkout Git state, so it persists its identity in `.effigy`. Only Mutate mints a token; Observe reads and reports "no runtime
//! state yet".

use std::io;
use std::path::Path;

use effigy_core::worktree_scope::{self, ScopeKind};
use effigy_kernel::id::{CheckoutId, CheckoutKind};

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
    if let Some(token) = worktree_scope::load_checkout_token(root)
        .map_err(io_error)?
        .or(worktree_scope::checkout_token(root).map_err(io_error)?)
    {
        return parse(CheckoutKind::Primary, &token).map_err(io_error);
    }
    let token = worktree_scope::prepare_checkout_token(root).map_err(io_error)?;
    parse(CheckoutKind::Primary, &token).map_err(io_error)
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
            .or(worktree_scope::checkout_token(root)?)
            .map(|token| parse(CheckoutKind::Primary, &token))
            .transpose();
    }
    worktree_scope::checkout_token(root)?
        .map(|token| parse(CheckoutKind::Primary, &token))
        .transpose()
}

fn scoped_kind(kind: ScopeKind) -> CheckoutKind {
    match kind {
        ScopeKind::LinkedWorktree => CheckoutKind::LinkedWorktree,
        ScopeKind::EphemeralClone => CheckoutKind::EphemeralClone,
    }
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
        let token = root.path().join(".git").join(worktree_scope::SCOPE_FILE);
        assert!(!token.exists(), "observe writes nothing");
        let minted = mint(root.path()).expect("mint");
        assert!(token.is_file());
        assert_eq!(read(root.path()).expect("read"), Some(minted));
        assert_eq!(mint(root.path()).expect("mint again"), minted);
        assert_eq!(minted.kind(), CheckoutKind::Primary);
    }

    #[test]
    fn root_without_git_state_mints_a_persistent_identity_only_on_mutate() {
        let root = tempfile::tempdir().expect("root");
        assert_eq!(read(root.path()).expect("read"), None);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        let minted = mint(root.path()).expect("mint");
        assert_eq!(read(root.path()).expect("read"), Some(minted));
        assert!(root
            .path()
            .join(".effigy")
            .join(worktree_scope::SCOPE_FILE)
            .is_file());
        let other = tempfile::tempdir().expect("other");
        assert_ne!(mint(other.path()).expect("mint other"), minted);
    }
}

/// Every persisted identity this checkout actually carries. Older primary task
/// and environment engines used different token files; their coordinates must
/// survive migration as aliases. A path hash never verifies a generation.
pub(super) fn identities(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &Path,
) -> io::Result<Vec<CheckoutId>> {
    if let Some(scope) = worktree_scope::resolve(root)? {
        return Ok(
            read_token(cx, &scope.token_dir, worktree_scope::SCOPE_FILE)?
                .map(|token| parse(scoped_kind(scope.kind), &token))
                .transpose()?
                .into_iter()
                .collect(),
        );
    }
    let git = root.join(".git");
    let mut ids = Vec::new();
    if git.is_dir() {
        for name in [
            worktree_scope::CHECKOUT_TOKEN_FILE,
            worktree_scope::SCOPE_FILE,
        ] {
            if let Some(token) = read_token(cx, &git, name)? {
                let id = parse(CheckoutKind::Primary, &token)?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    } else {
        let dir = root.join(".effigy");
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() => {
                if let Some(token) = read_token(cx, &dir, worktree_scope::SCOPE_FILE)? {
                    ids.push(parse(CheckoutKind::Primary, &token)?);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "checkout identity directory is not a real directory",
                ))
            }
        }
    }
    Ok(ids)
}
#[derive(Debug)]
struct Token(String);
impl effigy_kernel::record::Record for Token {
    const MAX_BYTES: u64 = 128;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value = std::str::from_utf8(bytes)
            .map_err(|error| error.to_string())?
            .trim();
        parse(CheckoutKind::Primary, value).map_err(|error| error.to_string())?;
        Ok(Self(value.into()))
    }
}
fn read_token(
    cx: &effigy_kernel::ctx::ObserveContext,
    dir: &Path,
    name: &str,
) -> io::Result<Option<String>> {
    let dir = effigy_kernel::fs::DirFd::open(dir).map_err(io::Error::other)?;
    let name = effigy_kernel::fs::FileName::new(name).map_err(io::Error::other)?;
    match effigy_kernel::record::read_record::<Token>(cx, &dir, &name) {
        effigy_kernel::observe::Observed::Verified(Token(value), _) => Ok(Some(value)),
        effigy_kernel::observe::Observed::Absent(_) => Ok(None),
        effigy_kernel::observe::Observed::Unknown(reason) => Err(io::Error::other(format!(
            "checkout token is unknown: {reason:?}"
        ))),
    }
}
