//! Stable identity for one generation of a linked Git worktree.
//!
//! Git's private worktree directory lives outside the checkout and is removed
//! when that worktree is retired. A new worktree at the same path receives a
//! new private directory and therefore a new token.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use crate::git_worktree::detect_linked_worktree;

const SCOPE_FILE: &str = "effigy-runtime-scope";

/// Return the linked worktree's persistent token, creating it once if needed.
pub fn load_or_create(repo_root: &Path) -> io::Result<Option<String>> {
    let Some(layout) = detect_linked_worktree(repo_root) else {
        if repo_root.join(".git").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "linked worktree metadata is invalid at {}",
                    repo_root.display()
                ),
            ));
        }
        return Ok(None);
    };
    let path = layout.worktree_git_dir.join(SCOPE_FILE);
    match fs::read_to_string(&path) {
        Ok(value) => {
            return valid_token(&value).map(Some).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid worktree scope at {}", path.display()),
                )
            });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let mut random = [0u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            Ok(Some(token))
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // A concurrent activation won the create race. A short empty read
            // means its writer has not finished yet.
            for _ in 0..100 {
                if let Ok(value) = fs::read_to_string(&path) {
                    if let Some(value) = valid_token(&value) {
                        return Ok(Some(value));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("worktree scope at {} was not completed", path.display()),
            ))
        }
        Err(error) => Err(error),
    }
}

/// Check whether a recorded owner still names this worktree generation.
pub fn is_live(repo_root: &Path, token: &str) -> bool {
    detect_linked_worktree(repo_root)
        .and_then(|layout| fs::read_to_string(layout.worktree_git_dir.join(SCOPE_FILE)).ok())
        .and_then(|value| valid_token(&value))
        .is_some_and(|value| value == token)
}

fn valid_token(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_stable_and_changes_when_worktree_is_recreated_at_same_path() {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("worker");
        let private = root.path().join("primary/.git/worktrees/worker");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&private).unwrap();
        fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", private.display()),
        )
        .unwrap();
        fs::write(private.join("commondir"), "../..\n").unwrap();
        let first = load_or_create(&checkout).unwrap().unwrap();
        assert_eq!(load_or_create(&checkout).unwrap(), Some(first.clone()));
        assert!(is_live(&checkout, &first));
        fs::remove_dir_all(&private).unwrap();
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("commondir"), "../..\n").unwrap();
        assert!(!is_live(&checkout, &first));
        let next = load_or_create(&checkout).unwrap().unwrap();
        assert_ne!(first, next);
        assert!(is_live(&checkout, &next));
    }
}
