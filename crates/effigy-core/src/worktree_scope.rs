//! Stable identity for one generation of a runtime-scoped checkout.
//!
//! Two checkout shapes receive an isolated runtime scope today:
//!
//! - a linked Git worktree, whose private worktree directory lives outside
//!   the checkout and is removed when that worktree is retired
//! - a full clone whose *local* Git config sets
//!   `effigy.runtimeScope = ephemeral` (Nucleus task workspaces mark every
//!   clone they create; Queue sibling clones may use the same marker). Its
//!   `.git` is a self-contained directory, so the token lives directly inside
//!   it and no shared-Git-directory mount is needed.
//!
//! The token lives in Git's own per-checkout state, so deleting and
//! recreating the checkout at the same path yields a new generation. An
//! unmarked primary checkout or full clone keeps primary-checkout behavior:
//! no scope, no token.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use crate::git_worktree::detect_linked_worktree;

pub const SCOPE_FILE: &str = "effigy-runtime-scope";

/// Which checkout shape produced a runtime scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Linked `git worktree` checkout; the token lives in Git's private
    /// worktree directory under the primary checkout's `.git`.
    LinkedWorktree,
    /// Full clone marked ephemeral in its local Git config; the token lives
    /// in the clone's own `.git` directory.
    EphemeralClone,
}

impl ScopeKind {
    /// Short tag baked into generated Compose project names so operators can
    /// tell scope shapes apart. Shared-identity checks parse this tag back
    /// out of a recorded project name.
    pub fn project_tag(self) -> &'static str {
        match self {
            ScopeKind::LinkedWorktree => "wt",
            ScopeKind::EphemeralClone => "ec",
        }
    }
}

/// Every project tag a scoped project name may carry, across scope shapes.
/// Consumers parse this list instead of hard-coding one shape.
pub const PROJECT_TAGS: [&str; 2] = ["wt", "ec"];

/// A resolved runtime scope: which checkout shape it is and where its token
/// file lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeScope {
    pub kind: ScopeKind,
    /// Directory holding the [`SCOPE_FILE`] token.
    pub token_dir: std::path::PathBuf,
}

/// Git config key that marks a clone ephemeral. Section and key names are
/// case-insensitive in Git; the value comparison stays case-sensitive
/// because tooling writes the exact marker.
const MARKER_SECTION: &str = "effigy";
const MARKER_KEY: &str = "runtimescope";
const MARKER_VALUE: &str = "ephemeral";

/// Resolve the runtime scope behind `repo_root`, or `None` when the checkout
/// keeps primary-checkout behavior.
///
/// A linked worktree always wins: its `.git` is a file pointing into the
/// primary checkout, so the clone marker (which only meaningful for a `.git`
/// directory) cannot apply. Reading the marker is a local-only operation —
/// see [`local_config_marks_ephemeral`].
pub fn resolve(repo_root: &Path) -> io::Result<Option<RuntimeScope>> {
    if let Some(layout) = detect_linked_worktree(repo_root) {
        return Ok(Some(RuntimeScope {
            kind: ScopeKind::LinkedWorktree,
            token_dir: layout.worktree_git_dir,
        }));
    }
    let git_dir = repo_root.join(".git");
    if git_dir.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "linked worktree metadata is invalid at {}",
                repo_root.display()
            ),
        ));
    }
    if !git_dir.is_dir() {
        return Ok(None);
    }
    if local_config_marks_ephemeral(&git_dir)? {
        Ok(Some(RuntimeScope {
            kind: ScopeKind::EphemeralClone,
            token_dir: git_dir,
        }))
    } else {
        Ok(None)
    }
}

/// Which checkout shape owns the runtime scope, if any.
pub fn scope_kind(repo_root: &Path) -> io::Result<Option<ScopeKind>> {
    Ok(resolve(repo_root)?.map(|scope| scope.kind))
}

/// Whether this checkout receives an isolated runtime scope: a linked
/// worktree or a clone marked `effigy.runtimeScope = ephemeral`.
pub fn is_runtime_scoped(repo_root: &Path) -> io::Result<bool> {
    Ok(resolve(repo_root)?.is_some())
}

/// Return the scoped checkout's persistent token, creating it once if needed.
pub fn load_or_create(repo_root: &Path) -> io::Result<Option<String>> {
    Ok(load_or_create_scoped(repo_root)?.map(|(_kind, token)| token))
}

/// Like [`load_or_create`], but also reports which checkout shape owns the
/// token.
pub fn load_or_create_scoped(repo_root: &Path) -> io::Result<Option<(ScopeKind, String)>> {
    let Some(scope) = resolve(repo_root)? else {
        return Ok(None);
    };
    load_or_create_token(&scope.token_dir).map(|token| Some((scope.kind, token)))
}

/// Check whether a recorded owner still names this checkout generation.
pub fn is_live(repo_root: &Path, token: &str) -> bool {
    let recorded = resolve(repo_root)
        .ok()
        .flatten()
        .and_then(|scope| fs::read_to_string(scope.token_dir.join(SCOPE_FILE)).ok());
    recorded
        .and_then(|value| valid_token(&value))
        .is_some_and(|value| value == token)
}

fn load_or_create_token(token_dir: &Path) -> io::Result<String> {
    let path = token_dir.join(SCOPE_FILE);
    match fs::read_to_string(&path) {
        Ok(value) => {
            return valid_token(&value).ok_or_else(|| {
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
            Ok(token)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // A concurrent activation won the create race. A short empty read
            // means its writer has not finished yet.
            for _ in 0..100 {
                if let Ok(value) = fs::read_to_string(&path) {
                    if let Some(value) = valid_token(&value) {
                        return Ok(value);
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

/// Whether the checkout's *local* Git config marks it
/// `effigy.runtimeScope = ephemeral`.
///
/// This deliberately reads only `<repo>/.git/config`: no `git` subprocess, no
/// `--system`/`--global` scope, no `GIT_CONFIG_*` environment, and no
/// `[include]`/`[includeIf]` expansion. Only a writer that puts the marker in
/// the checkout's own Git directory can scope it, so a global or inherited
/// setting can never mark an unmarked checkout.
fn local_config_marks_ephemeral(git_dir: &Path) -> io::Result<bool> {
    let path = git_dir.join("config");
    let raw = match fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(git_config_value(&raw, MARKER_SECTION, MARKER_KEY)
        .is_some_and(|value| value == MARKER_VALUE))
}

/// Minimal reader for one Git config file's own content, returning the value
/// of `section.key` when that exact bare section declares it.
///
/// Handles the constructs `git config` writes for a local key: section
/// headers (with or without a quoted subsection), comments, quoted values and
/// boolean bare keys. A subsectioned key (`[effigy "prod"]`) never matches
/// the bare `[effigy]` section, and `[include]` directives are treated as
/// data, never followed.
fn git_config_value(raw: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in raw.lines() {
        let line = strip_config_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_section = header.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            // Bare boolean key; it cannot equal the marker value.
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(unquote_config_value(value.trim()));
        }
    }
    None
}

/// Remove a trailing `#`/`;` comment that is not inside a quoted value.
fn strip_config_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' if !escaped => in_quotes = !in_quotes,
            '#' | ';' if !in_quotes => return &line[..index],
            _ => {}
        }
        escaped = in_quotes && ch == '\\' && !escaped;
    }
    line
}

/// Strip one pair of surrounding quotes and resolve the `\"` and `\\`
/// escapes Git writes inside quoted values.
fn unquote_config_value(value: &str) -> String {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return value.to_owned();
    };
    let mut unescaped = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(escaped) => unescaped.push(escaped),
                None => break,
            }
        } else {
            unescaped.push(ch);
        }
    }
    unescaped
}

fn valid_token(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a full-clone-shaped checkout whose local config carries
    /// `config_body` (or none when empty).
    fn clone_checkout(root: &Path, config_body: &str) -> std::path::PathBuf {
        let checkout = root.join("clone");
        fs::create_dir_all(checkout.join(".git")).unwrap();
        if !config_body.is_empty() {
            fs::write(checkout.join(".git/config"), config_body).unwrap();
        }
        checkout
    }

    const MARKED: &str =
        "[core]\n\trepositoryformatversion = 0\n[effigy]\n\truntimeScope = ephemeral\n";

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

    #[test]
    fn marked_clone_reuses_token_and_recreated_clone_gets_new_generation() {
        let root = tempfile::tempdir().unwrap();
        let checkout = clone_checkout(root.path(), MARKED);
        assert_eq!(
            scope_kind(&checkout).unwrap(),
            Some(ScopeKind::EphemeralClone)
        );
        let first = load_or_create(&checkout).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(checkout.join(".git").join(SCOPE_FILE)).unwrap(),
            first
        );
        assert_eq!(load_or_create(&checkout).unwrap(), Some(first.clone()));
        assert_eq!(
            load_or_create_scoped(&checkout).unwrap(),
            Some((ScopeKind::EphemeralClone, first.clone()))
        );
        assert!(is_live(&checkout, &first));

        // Deleting the clone and recreating it at the same path must yield a
        // new generation token that invalidates the old owner.
        fs::remove_dir_all(checkout.join(".git")).unwrap();
        clone_checkout(root.path(), MARKED);
        assert!(!is_live(&checkout, &first));
        let next = load_or_create(&checkout).unwrap().unwrap();
        assert_ne!(first, next);
        assert!(is_live(&checkout, &next));
    }

    #[test]
    fn unmarked_clone_and_primary_checkout_have_no_scope() {
        let root = tempfile::tempdir().unwrap();
        let plain = clone_checkout(root.path(), "[core]\n\trepositoryformatversion = 0\n");
        let marker_elsewhere = clone_checkout(
            root.path().join("other").as_path(),
            "[effigy \"prod\"]\n\truntimeScope = ephemeral\n",
        );
        let bare = root.path().join("bare");
        fs::create_dir_all(&bare).unwrap();
        for checkout in [plain, marker_elsewhere, bare] {
            assert_eq!(scope_kind(checkout.as_path()).unwrap(), None);
            assert!(!is_runtime_scoped(checkout.as_path()).unwrap());
            assert_eq!(load_or_create(checkout.as_path()).unwrap(), None);
        }
    }

    #[test]
    fn marker_forms_written_by_git_config_are_recognized() {
        let root = tempfile::tempdir().unwrap();
        let marked = clone_checkout(root.path(), MARKED);
        assert!(is_runtime_scoped(&marked).unwrap());
        let spaced = clone_checkout(
            root.path().join("spaced").as_path(),
            "[effigy] ; comment\n runtimeScope = \"ephemeral\" # trailing\n",
        );
        assert!(is_runtime_scoped(&spaced).unwrap());
        let upper = clone_checkout(
            root.path().join("upper").as_path(),
            "[EFFIGY]\n\tRUNTIMESCOPE = ephemeral\n",
        );
        assert!(is_runtime_scoped(&upper).unwrap());
    }

    #[test]
    fn near_miss_values_and_inherited_config_do_not_mark() {
        let root = tempfile::tempdir().unwrap();
        let cases = [
            // Boolean bare key or wrong value: not the marker.
            "[effigy]\n\truntimeScope\n",
            "[effigy]\n\truntimeScope = true\n",
            "[effigy]\n\truntimeScope = Ephemeral\n",
            // Commented out, other sections, subsectioned keys.
            "[effigy]\n\t# runtimeScope = ephemeral\n",
            "[core]\n\truntimeScope = ephemeral\n",
            "[include]\n\tpath = ~/gitconfig-ephemeral-marker\n",
        ];
        for (index, body) in cases.iter().enumerate() {
            let path = root.path().join(format!("case-{index}"));
            let checkout = clone_checkout(&path, body);
            assert!(
                !is_runtime_scoped(&checkout).unwrap(),
                "case {index} unexpectedly marked: {body}"
            );
        }
    }

    #[test]
    fn invalid_recorded_token_is_an_error_not_a_recreate() {
        let root = tempfile::tempdir().unwrap();
        let checkout = clone_checkout(root.path(), MARKED);
        fs::write(checkout.join(".git").join(SCOPE_FILE), "not-a-token").unwrap();
        let error = load_or_create(&checkout).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn project_tags_cover_every_scope_kind() {
        assert_eq!(
            ScopeKind::LinkedWorktree.project_tag(),
            PROJECT_TAGS[0] // "wt"
        );
        assert_eq!(
            ScopeKind::EphemeralClone.project_tag(),
            PROJECT_TAGS[1] // "ec"
        );
    }
}
