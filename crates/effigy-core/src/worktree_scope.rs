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

/// The checkout that owns `path`'s runtime identity and durable state.
///
/// Git discovery is lexical and upward: the nearest ancestor (or `path`
/// itself) carrying `.git` — a directory for a primary checkout or clone, a
/// file for a linked worktree or submodule — owns everything below it. A
/// nested catalog directory with no Git metadata of its own therefore belongs
/// to the enclosing checkout (contract 052 §7.1: durable checkout state
/// belongs to the resolved invocation root, not a nested catalog's cwd), while
/// a nested repository or submodule keeps its own identity. A path with no Git
/// metadata anywhere above it is its own checkout.
pub fn checkout_root(path: &Path) -> std::path::PathBuf {
    path.ancestors()
        .find(|candidate| {
            !candidate.as_os_str().is_empty() && candidate.join(".git").symlink_metadata().is_ok()
        })
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

/// Resolve the runtime scope behind `repo_root`, or `None` when the checkout
/// keeps primary-checkout behavior. A nested catalog directory resolves the
/// enclosing checkout's scope; see [`checkout_root`].
///
/// A linked worktree always wins: its `.git` is a file pointing into the
/// primary checkout, so the clone marker (which only meaningful for a `.git`
/// directory) cannot apply. Reading the marker is a local-only operation —
/// see [`local_config_marks_ephemeral`].
pub fn resolve(repo_root: &Path) -> io::Result<Option<RuntimeScope>> {
    let repo_root = checkout_root(repo_root);
    let repo_root = repo_root.as_path();
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

/// Git metadata belonging to this checkout, never a linked worktree's common
/// Git directory. Non-Git roots have no Git generation to bind.
pub fn git_directory(repo_root: &Path) -> io::Result<Option<std::path::PathBuf>> {
    if let Some(scope) = resolve(repo_root)? {
        return Ok(Some(scope.token_dir));
    }
    let git = checkout_root(repo_root).join(".git");
    Ok(git.is_dir().then_some(git))
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

/// Read an existing scope without allocating identity or writing Git state.
/// A scoped checkout without a token has no runtime state yet.
pub fn load(repo_root: &Path) -> io::Result<Option<String>> {
    Ok(load_scoped(repo_root)?.map(|(_, token)| token))
}

fn read_token(dir: &Path) -> io::Result<Option<String>> {
    let path = dir.join(SCOPE_FILE);
    match fs::read_to_string(&path) {
        Ok(raw) => valid_token(&raw).map(Some).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid checkout identity at {}", path.display()),
            )
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// All checkout shapes need an identity for kernel resources. Primary Git
/// checkouts keep their token beside Git metadata; non-Git manifests keep it
/// in their private Effigy directory. Both answer for the enclosing checkout,
/// so a nested catalog directory shares its checkout's identity rather than
/// minting a second one (see [`checkout_root`]).
fn identity_dir(repo_root: &Path) -> io::Result<std::path::PathBuf> {
    if let Some(scope) = resolve(repo_root)? {
        return Ok(scope.token_dir);
    }
    let root = checkout_root(repo_root);
    let git = root.join(".git");
    Ok(if git.is_dir() {
        git
    } else {
        root.join(".effigy")
    })
}

/// The checkout's identity token, read in one order for every caller: the
/// runtime-scope file ([`SCOPE_FILE`]) in the checkout's identity directory
/// first, then a primary checkout's own [`CHECKOUT_TOKEN_FILE`]. A checkout
/// carrying both resolves to the scope file's token, so the kernel invocation,
/// containers and environments agree on one identity.
pub fn checkout_token(repo_root: &Path) -> io::Result<Option<String>> {
    let scope = read_token(&identity_dir(repo_root)?)?;
    if scope.is_some() || resolve(repo_root)?.is_some() {
        return Ok(scope);
    }
    load_checkout_token(repo_root)
}

/// Prepare-only identity allocation; callers must resolve again afterward.
pub fn prepare_checkout_token(repo_root: &Path) -> io::Result<String> {
    if let Some(token) = checkout_token(repo_root)? {
        return Ok(token);
    }
    let dir = identity_dir(repo_root)?;
    fs::create_dir_all(&dir)?;
    load_or_create_token(&dir)
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

/// The scoped checkout's recorded token without creating one, for readers
/// that must write nothing.
pub fn load_scoped(repo_root: &Path) -> io::Result<Option<(ScopeKind, String)>> {
    let Some(scope) = resolve(repo_root)? else {
        return Ok(None);
    };
    match fs::read_to_string(scope.token_dir.join(SCOPE_FILE)) {
        Ok(value) => valid_token(&value)
            .map(|token| Some((scope.kind, token)))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid worktree scope in {}", scope.token_dir.display()),
                )
            }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// File holding a primary checkout's own identity token, beside the
/// runtime-scope token in Git's per-checkout directory.
pub const CHECKOUT_TOKEN_FILE: &str = "effigy-checkout-id";

/// The recorded primary checkout token without creating one.
pub fn load_checkout_token(repo_root: &Path) -> io::Result<Option<String>> {
    let path = checkout_root(repo_root)
        .join(".git")
        .join(CHECKOUT_TOKEN_FILE);
    match fs::read_to_string(&path) {
        Ok(value) => valid_token(&value).map(Some).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid checkout token at {}", path.display()),
            )
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn load_or_create_token(token_dir: &Path) -> io::Result<String> {
    load_or_create_token_file(&token_dir.join(SCOPE_FILE))
}

fn load_or_create_token_file(path: &Path) -> io::Result<String> {
    if let Some(token) = read_published_token(path)? {
        return Ok(token);
    }
    let mut random = [0u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    publish_token(path, &token)
}

/// The token published at `path`, or `None` when no creator has published one.
///
/// A published file is always whole (see [`publish_token`]), so a malformed
/// file here is a corrupt persisted scope, never an in-flight write.
fn read_published_token(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(value) => valid_token(&value).map(Some).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid worktree scope at {}", path.display()),
            )
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Publish `token` at `path` so that the path only ever names a complete,
/// synced token.
///
/// The value is staged in a private sibling file and hard-linked into place.
/// `link(2)` fails when `path` exists, so exactly one creator wins and every
/// other caller adopts the winner's token. A crash before the link leaves only
/// the staged sibling, which no reader consults.
fn publish_token(path: &Path, token: &str) -> io::Result<String> {
    let staged = staged_path(path, token);
    let linked = stage_token(&staged, token).and_then(|()| fs::hard_link(&staged, path));
    let _ = fs::remove_file(&staged);
    match linked {
        Ok(()) => {
            if let Some(dir) = path.parent() {
                sync_directory(dir)?;
            }
            Ok(token.to_owned())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read_published_token(path)?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("worktree scope at {} vanished mid-publish", path.display()),
                )
            }),
        Err(error) => Err(error),
    }
}

fn staged_path(path: &Path, token: &str) -> std::path::PathBuf {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    path.with_file_name(format!(".{name}.{token}.tmp"))
}

fn stage_token(staged: &Path, token: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staged)?;
    file.write_all(token.as_bytes())?;
    file.sync_all()
}

#[cfg(unix)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_dir: &Path) -> io::Result<()> {
    Ok(())
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

    /// The token this checkout records now, or `None` when it records none.
    fn recorded_checkout_token(checkout: &Path) -> Option<String> {
        load_or_create(checkout).ok().flatten()
    }

    /// A primary checkout token written the way v0.14.1 left it, so an
    /// existing `effigy-checkout-id` stays readable.
    pub(super) fn legacy_checkout_token(checkout: &Path, token: &str) -> String {
        fs::write(checkout.join(".git").join(CHECKOUT_TOKEN_FILE), token).unwrap();
        token.to_owned()
    }

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
        assert_eq!(recorded_checkout_token(&checkout), Some(first.clone()));
        fs::remove_dir_all(&private).unwrap();
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("commondir"), "../..\n").unwrap();
        assert_ne!(recorded_checkout_token(&checkout), Some(first.clone()));
        let next = load_or_create(&checkout).unwrap().unwrap();
        assert_ne!(first, next);
        assert_eq!(recorded_checkout_token(&checkout), Some(next.clone()));
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
        assert_eq!(recorded_checkout_token(&checkout), Some(first.clone()));

        // Deleting the clone and recreating it at the same path must yield a
        // new generation token that invalidates the old owner.
        fs::remove_dir_all(checkout.join(".git")).unwrap();
        clone_checkout(root.path(), MARKED);
        assert_ne!(recorded_checkout_token(&checkout), Some(first.clone()));
        let next = load_or_create(&checkout).unwrap().unwrap();
        assert_ne!(first, next);
        assert_eq!(recorded_checkout_token(&checkout), Some(next.clone()));
    }

    #[test]
    fn nested_catalog_directory_belongs_to_the_enclosing_checkout() {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("repo");
        fs::create_dir_all(checkout.join(".git")).unwrap();
        // A nested catalog directory with its own manifest and no Git
        // metadata of its own (contract 052 §7.1).
        let nested = checkout.join("apps/site");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("effigy.toml"), "[catalog]\n").unwrap();

        assert_eq!(checkout_root(&nested), checkout);
        let token = legacy_checkout_token(&checkout, "7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a");
        assert_eq!(checkout_token(&nested).unwrap(), Some(token.clone()));
        assert_eq!(checkout_token(&checkout).unwrap(), Some(token));
        assert_eq!(git_directory(&nested).unwrap(), Some(checkout.join(".git")));
        // It never mints a second checkout, so it writes no journal of its own.
        assert!(!nested.join(".effigy").exists());
    }

    #[test]
    fn nested_scoped_checkout_scope_reaches_its_catalog_directories() {
        let root = tempfile::tempdir().unwrap();
        let checkout = clone_checkout(root.path(), MARKED);
        let nested = checkout.join("apps/site");
        fs::create_dir_all(&nested).unwrap();
        let token = load_or_create(&checkout).unwrap().unwrap();
        assert_eq!(load(&nested).unwrap(), Some(token));
        assert_eq!(
            load_or_create_scoped(&nested).unwrap(),
            Some((
                ScopeKind::EphemeralClone,
                load_or_create(&checkout).unwrap().unwrap()
            ))
        );
    }

    #[test]
    fn nested_repository_keeps_its_own_identity() {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("repo");
        fs::create_dir_all(checkout.join(".git")).unwrap();
        // A nested repository carries its own Git metadata, so it is a
        // checkout in its own right rather than part of its parent.
        let inner = checkout.join("vendor/library");
        fs::create_dir_all(inner.join(".git")).unwrap();
        assert_eq!(checkout_root(&inner), inner);
        let outer = legacy_checkout_token(&checkout, "11111111111111111111111111111111");
        let own = legacy_checkout_token(&inner, "22222222222222222222222222222222");
        assert_ne!(outer, own);
        assert_eq!(checkout_token(&inner).unwrap(), Some(own));
        // A submodule is a file-shaped `.git`; it is still its own checkout.
        let submodule = checkout.join("vendor/submodule");
        fs::create_dir_all(&submodule).unwrap();
        fs::write(submodule.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
        assert_eq!(checkout_root(&submodule), submodule);
    }

    #[test]
    fn root_checkout_linked_worktree_and_plain_directory_keep_their_roots() {
        let root = tempfile::tempdir().unwrap();
        let git_root = root.path().join("repo");
        fs::create_dir_all(git_root.join(".git")).unwrap();
        assert_eq!(checkout_root(&git_root), git_root);

        let worktree = root.path().join("worktree");
        let private = root.path().join("repo/.git/worktrees/worktree");
        fs::create_dir_all(&worktree).unwrap();
        fs::create_dir_all(&private).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", private.display()),
        )
        .unwrap();
        fs::write(private.join("commondir"), "../..\n").unwrap();
        assert_eq!(checkout_root(&worktree), worktree);

        // No Git metadata anywhere above it: the directory is its own checkout.
        let plain = root.path().join("plain/nested");
        fs::create_dir_all(&plain).unwrap();
        assert_eq!(checkout_root(&plain), plain);
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
    fn an_empty_persisted_scope_is_refused_as_corrupt() {
        // Publishing never leaves a partial file at the scope path, so an
        // empty one is corruption: refuse it, do not wait for a writer.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SCOPE_FILE);
        fs::write(&path, "").unwrap();
        let error = load_or_create_token_file(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn a_staged_but_unpublished_token_is_invisible_to_readers() {
        // A creator died after writing part of its staged token, before the
        // link. The scope path must read as absent and the next caller must
        // publish a complete token of its own.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SCOPE_FILE);
        let abandoned = "ab".repeat(16);
        fs::write(staged_path(&path, &abandoned), &abandoned[..16]).unwrap();
        assert_eq!(read_published_token(&path).unwrap(), None);
        let token = load_or_create_token_file(&path).unwrap();
        assert_ne!(token, abandoned);
        assert_eq!(fs::read_to_string(&path).unwrap(), token);
    }

    #[test]
    fn a_creator_that_loses_the_publish_race_adopts_the_winners_token() {
        // Deterministic interleave: the winner publishes, then a second creator
        // that already drew its own token publishes against the same path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SCOPE_FILE);
        let winner = load_or_create_token_file(&path).unwrap();
        let loser = "01".repeat(16);
        assert_eq!(publish_token(&path, &loser).unwrap(), winner);
        assert_eq!(fs::read_to_string(&path).unwrap(), winner);
    }

    #[test]
    fn racing_creators_all_read_one_whole_token() {
        let fixture = tempfile::tempdir().unwrap();
        let checkout = clone_checkout(fixture.path(), MARKED);
        let barrier = std::sync::Barrier::new(8);
        let results = std::thread::scope(|scope| {
            let racers = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        load_or_create(&checkout)
                    })
                })
                .collect::<Vec<_>>();
            racers
                .into_iter()
                .map(|racer| racer.join().unwrap())
                .collect::<Vec<_>>()
        });
        let first = results[0].as_ref().unwrap().clone().unwrap();
        for result in results {
            assert_eq!(result.unwrap(), Some(first.clone()));
        }
        assert_eq!(recorded_checkout_token(&checkout), Some(first));
        let leftovers = fs::read_dir(checkout.join(".git"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0, "every racer cleans up its staged token");
    }

    #[test]
    fn scope_reads_leave_a_fresh_ephemeral_checkout_uninitialized() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("checkout");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(
            root.join(".git/config"),
            "[effigy]\nruntimeScope=ephemeral\n",
        )
        .unwrap();
        assert_eq!(load(&root).unwrap(), None);
        assert_eq!(load_scoped(&root).unwrap(), None);
        assert!(!root.join(".git").join(SCOPE_FILE).exists());
        let token = prepare_checkout_token(&root).unwrap();
        assert_eq!(load(&root).unwrap(), Some(token.clone()));
        assert_eq!(checkout_token(&root).unwrap(), Some(token));
    }
    #[test]
    fn primary_task_identity_is_reused_without_minting_a_second_token() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        fs::create_dir(root.join(".git")).unwrap();
        let token = super::tests::legacy_checkout_token(root, "33333333333333333333333333333333");
        assert_eq!(checkout_token(root).unwrap(), Some(token.clone()));
        assert_eq!(prepare_checkout_token(root).unwrap(), token);
        assert!(!root.join(".git").join(SCOPE_FILE).exists());
        // A checkout carrying both files resolves to the scope file's token,
        // and the legacy token stays readable for migration aliases.
        let other = "01010101010101010101010101010101";
        fs::write(root.join(".git").join(SCOPE_FILE), other).unwrap();
        assert_eq!(checkout_token(root).unwrap().as_deref(), Some(other));
        assert_eq!(prepare_checkout_token(root).unwrap(), other);
        assert_eq!(load_checkout_token(root).unwrap(), Some(token));
    }

    #[test]
    fn primary_checkout_identity_never_changes_scope_naming() {
        let fixture = tempfile::tempdir().unwrap();
        fs::create_dir(fixture.path().join(".git")).unwrap();
        assert_eq!(checkout_token(fixture.path()).unwrap(), None);
        let token = prepare_checkout_token(fixture.path()).unwrap();
        assert_eq!(prepare_checkout_token(fixture.path()).unwrap(), token);
        assert_eq!(load(fixture.path()).unwrap(), None);
        assert_eq!(checkout_token(fixture.path()).unwrap(), Some(token));
    }
}
