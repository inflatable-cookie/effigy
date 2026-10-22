//! Pinned extra sources for `effigy bootstrap` (Q-009, contract 020).
//!
//! A source is one git checkout pinned to one commit and placed at one path,
//! which may sit outside the project (a sibling). The pin is committed in the
//! manifest as `rev` or `rev_file`, so there is no second lock: bootstrap and
//! the first `dev` only acquire a missing checkout, and nothing ever advances a
//! pin. An existing working copy is never reset, fetched forward or cleaned.
//!
//! Inspection is pure: it reads the checkout and reports what it finds.
//! Preparation acquires only a missing destination.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config_sections::ManifestPinnedSourceConfig;
use crate::{load_task_manifest, ManifestError, TASK_MANIFEST_FILE};

/// What bootstrap or `effigy doctor` observed for one pinned source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinnedSourceState {
    /// The checkout was acquired by this run and checked out at the pin.
    Cloned,
    /// An existing checkout already sits exactly at the pin with no local changes.
    Present,
    /// An existing checkout differs from the pin, or has local changes. Left untouched.
    Drift,
    /// No checkout exists at the path yet.
    Missing,
    /// The path holds something that is not a git checkout.
    NotCheckout,
    /// The checkout's `origin` is not the pinned remote.
    RemoteMismatch,
}

impl PinnedSourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloned => "cloned",
            Self::Present => "present",
            Self::Drift => "drift",
            Self::Missing => "missing",
            Self::NotCheckout => "not-checkout",
            Self::RemoteMismatch => "remote-mismatch",
        }
    }

    /// States an operator must look at. Nothing is repaired automatically.
    pub fn needs_attention(self) -> bool {
        matches!(
            self,
            Self::Drift | Self::NotCheckout | Self::RemoteMismatch
        )
    }
}

/// One resolved `[sources.<name>]` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedSourceSpec {
    pub name: String,
    pub git: String,
    /// The commit-ish the pin names, from `rev` or read out of `rev_file`.
    pub revision: String,
    /// The file the revision was read from, when the pin uses `rev_file`.
    pub revision_file: Option<PathBuf>,
    /// Absolute destination of the working copy.
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedSourceReport {
    pub name: String,
    pub git: String,
    pub revision: String,
    pub path: PathBuf,
    pub state: PinnedSourceState,
    /// Observed HEAD commit, when the path holds a git checkout.
    pub head: Option<String>,
    pub dirty: bool,
    pub detail: Option<String>,
}

impl PinnedSourceReport {
    pub fn drift_message(&self) -> Option<String> {
        if !self.state.needs_attention() {
            return None;
        }
        let mut message = format!(
            "pinned source `{}` at {} is {}: {}",
            self.name,
            self.path.display(),
            self.state.as_str(),
            self.detail.as_deref().unwrap_or("left untouched")
        );
        message.push_str(
            " (nothing was reset, fetched or cleaned; move the pin by editing `rev` or `rev_file`)",
        );
        Some(message)
    }
}

/// Resolve the manifest's declared pinned sources without touching any checkout.
pub fn resolve_pinned_sources(
    manifest_path: &Path,
) -> Result<Vec<PinnedSourceSpec>, ManifestError> {
    let manifest = load_task_manifest(manifest_path)?;
    let base = manifest_path.parent().unwrap_or(Path::new("."));
    let mut specs = Vec::with_capacity(manifest.sources.len());
    for (name, source) in &manifest.sources {
        specs.push(resolve_one(base, name, source)?);
    }
    Ok(specs)
}

fn resolve_one(
    base: &Path,
    name: &str,
    source: &ManifestPinnedSourceConfig,
) -> Result<PinnedSourceSpec, ManifestError> {
    let git = source.git.trim();
    if git.is_empty() {
        return Err(invalid(
            base,
            format!("pinned source `{name}` needs a non-empty `git` remote"),
        ));
    }
    let inline = source
        .rev
        .as_deref()
        .map(str::trim)
        .filter(|rev| !rev.is_empty());
    let from_file = source
        .rev_file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty());
    let (revision, revision_file) = match (inline, from_file) {
        (Some(_), Some(_)) => {
            return Err(invalid(
                base,
                format!(
                    "pinned source `{name}` declares both `rev` and `rev_file`; keep exactly one"
                ),
            ))
        }
        (None, None) => {
            return Err(invalid(
                base,
                format!(
                    "pinned source `{name}` needs `rev` or `rev_file`; a source is never advanced silently"
                ),
            ))
        }
        (Some(rev), None) => (rev.to_owned(), None),
        (None, Some(file)) => {
            let path = resolve_path(base, file);
            let contents = std::fs::read_to_string(&path).map_err(|error| ManifestError::Read {
                path: path.clone(),
                error,
            })?;
            let rev = contents.trim();
            if rev.is_empty() || rev.lines().count() != 1 {
                return Err(invalid(
                    base,
                    format!(
                        "pinned source `{name}` reads `rev_file` {} which must hold one non-empty commit",
                        path.display()
                    ),
                ));
            }
            (rev.to_owned(), Some(path))
        }
    };
    let path = source.path.trim();
    if path.is_empty() {
        return Err(invalid(
            base,
            format!("pinned source `{name}` needs a non-empty `path`"),
        ));
    }
    Ok(PinnedSourceSpec {
        name: name.to_owned(),
        git: git.to_owned(),
        revision,
        revision_file,
        path: resolve_path(base, path),
    })
}

fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        return normalize(path);
    }
    normalize(&base.join(path))
}

/// Lexically normalise `.` and `..` so sibling paths stay comparable.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn invalid(base: &Path, detail: String) -> ManifestError {
    ManifestError::Compose {
        path: base.join(TASK_MANIFEST_FILE),
        detail,
    }
}

/// Report what each pinned source looks like. Never mutates a checkout.
pub fn inspect_pinned_sources(
    manifest_path: &Path,
) -> Result<Vec<PinnedSourceReport>, ManifestError> {
    resolve_pinned_sources(manifest_path)?
        .iter()
        .map(inspect_one)
        .collect()
}

/// Acquire every missing pinned source and report the rest.
///
/// Only a missing destination is created. An existing checkout is inspected and
/// reported; it is never reset, fetched forward or cleaned.
pub fn prepare_pinned_sources(
    manifest_path: &Path,
) -> Result<Vec<PinnedSourceReport>, ManifestError> {
    resolve_pinned_sources(manifest_path)?
        .iter()
        .map(prepare_one)
        .collect()
}

fn prepare_one(spec: &PinnedSourceSpec) -> Result<PinnedSourceReport, ManifestError> {
    if spec.path.exists() {
        return inspect_one(spec);
    }
    if let Some(parent) = spec.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    git(
        None,
        &[
            "clone",
            "--quiet",
            spec.git.as_str(),
            spec.path.to_str().unwrap_or_default(),
        ],
    )?;
    git(
        Some(&spec.path),
        &["checkout", "--quiet", "--detach", spec.revision.as_str()],
    )?;
    let head = git(Some(&spec.path), &["rev-parse", "HEAD"]).ok();
    Ok(PinnedSourceReport {
        name: spec.name.clone(),
        git: spec.git.clone(),
        revision: spec.revision.clone(),
        path: spec.path.clone(),
        state: PinnedSourceState::Cloned,
        head,
        dirty: false,
        detail: None,
    })
}

fn inspect_one(spec: &PinnedSourceSpec) -> Result<PinnedSourceReport, ManifestError> {
    let mut report = PinnedSourceReport {
        name: spec.name.clone(),
        git: spec.git.clone(),
        revision: spec.revision.clone(),
        path: spec.path.clone(),
        state: PinnedSourceState::Missing,
        head: None,
        dirty: false,
        detail: None,
    };
    if !spec.path.exists() {
        report.detail = Some(format!(
            "no checkout at {}; `effigy bootstrap` clones it at the pin",
            spec.path.display()
        ));
        return Ok(report);
    }
    if !spec.path.join(".git").exists() {
        report.state = PinnedSourceState::NotCheckout;
        report.detail = Some(format!(
            "{} exists but is not a git checkout",
            spec.path.display()
        ));
        return Ok(report);
    }
    let remote = git(Some(&spec.path), &["remote", "get-url", "origin"]).unwrap_or_default();
    if canonical_identity(&remote) != canonical_identity(&spec.git) {
        report.state = PinnedSourceState::RemoteMismatch;
        report.detail = Some(format!("origin is `{remote}`, expected `{}`", spec.git));
        return Ok(report);
    }
    let head = git(Some(&spec.path), &["rev-parse", "HEAD"]).ok();
    let dirty = git(Some(&spec.path), &["status", "--porcelain"])
        .map(|status| !status.trim().is_empty())
        .unwrap_or(true);
    let pinned = git(
        Some(&spec.path),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{}^{{commit}}", spec.revision),
        ],
    )
    .ok();
    let at_pin = match (head.as_deref(), pinned.as_deref()) {
        (Some(head), Some(pinned)) => head == pinned,
        (Some(head), None) => commit_matches(head, &spec.revision),
        (None, _) => false,
    };
    report.head = head;
    report.dirty = dirty;
    if !at_pin || dirty {
        report.state = PinnedSourceState::Drift;
        let head = report.head.as_deref().unwrap_or("<unknown>");
        report.detail = Some(if dirty {
            format!("HEAD {head} is pinned to {} but the checkout has local changes", spec.revision)
        } else {
            format!("HEAD {head} does not match the pin {}", spec.revision)
        });
        return Ok(report);
    }
    report.state = PinnedSourceState::Present;
    Ok(report)
}

fn commit_matches(head: &str, revision: &str) -> bool {
    if revision.len() < 7 {
        return false;
    }
    head.starts_with(revision) || revision.starts_with(head)
}

fn canonical_identity(remote: &str) -> String {
    effigy_core::git_source::canonical_git_cache_identity(remote)
}

fn git(cwd: Option<&Path>, args: &[&str]) -> Result<String, ManifestError> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.arg("-C").arg(cwd);
    }
    let output = command.args(args).output().map_err(|error| ManifestError::Read {
        path: cwd.map(Path::to_path_buf).unwrap_or_default(),
        error,
    })?;
    if !output.status.success() {
        return Err(ManifestError::Compose {
            path: cwd.map(Path::to_path_buf).unwrap_or_default(),
            detail: format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join(TASK_MANIFEST_FILE);
        std::fs::write(&path, body).expect("write manifest");
        path
    }

    #[test]
    fn a_source_needs_exactly_one_pin_spelling() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manifest = write_manifest(
            dir.path(),
            r#"
[sources.book]
git = "git@example.test:book.git"
path = "../book"
"#,
        );
        let error = resolve_pinned_sources(&manifest)
            .expect_err("missing pin")
            .to_string();
        assert!(error.contains("needs `rev` or `rev_file`"), "{error}");

        let manifest = write_manifest(
            dir.path(),
            r#"
[sources.book]
git = "git@example.test:book.git"
rev = "abc1234"
rev_file = "book.rev"
path = "../book"
"#,
        );
        let error = resolve_pinned_sources(&manifest)
            .expect_err("two pins")
            .to_string();
        assert!(error.contains("both `rev` and `rev_file`"), "{error}");
    }

    #[test]
    fn rev_file_supplies_the_pin_and_must_hold_one_commit() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("infra")).expect("mkdir");
        std::fs::write(dir.path().join("infra/book.rev"), "deadbeef\n").expect("write rev file");
        let manifest = write_manifest(
            dir.path(),
            r#"
[sources.book]
git = "git@example.test:book.git"
rev_file = "infra/book.rev"
path = "../book"
"#,
        );
        let specs = resolve_pinned_sources(&manifest).expect("resolve sources");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].revision, "deadbeef");
        assert_eq!(
            specs[0].revision_file.as_deref(),
            Some(dir.path().join("infra/book.rev").as_path())
        );
        assert_eq!(
            specs[0].path,
            normalize(&dir.path().join("../book"))
        );

        std::fs::write(dir.path().join("infra/book.rev"), "one\ntwo\n").expect("write bad rev");
        let error = resolve_pinned_sources(&manifest)
            .expect_err("multi-line pin")
            .to_string();
        assert!(error.contains("must hold one non-empty commit"), "{error}");
    }

    #[test]
    fn inspection_reports_a_missing_checkout_without_creating_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manifest = write_manifest(
            dir.path(),
            r#"
[sources.book]
git = "git@example.test:book.git"
rev = "0123456789abcdef0123456789abcdef01234567"
path = "../book"
"#,
        );
        let reports = inspect_pinned_sources(&manifest).expect("inspect");
        assert_eq!(reports[0].state, PinnedSourceState::Missing);
        assert!(!dir.path().join("../book").exists());
    }

    #[test]
    fn inspection_refuses_a_path_that_is_not_a_checkout() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("book")).expect("mkdir book");
        let manifest = write_manifest(
            dir.path(),
            r#"
[sources.book]
git = "git@example.test:book.git"
rev = "0123456789abcdef0123456789abcdef01234567"
path = "book"
"#,
        );
        let reports = inspect_pinned_sources(&manifest).expect("inspect");
        assert_eq!(reports[0].state, PinnedSourceState::NotCheckout);
        assert!(reports[0].drift_message().is_some());
    }
}
