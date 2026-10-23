//! Cwd-aware resolution of explicit `docs check` path arguments.
//!
//! An explicit path argument belongs to the task that invokes the check, so it
//! resolves against the invoking task's cwd (a nested catalog's `docs/`
//! directory, for example) instead of the repository root. The no-argument
//! default scope stays repository-root relative.
//!
//! Transition (one release): consumers that already spell root-relative
//! arguments from a nested cwd keep passing. When the cwd-relative candidate is
//! missing and the root-relative candidate exists, the root-relative path is
//! used and a warning names the cwd-proof `{repo}` spelling.

use std::path::{Component, Path, PathBuf};

/// One resolved explicit path argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedDocsPath {
    /// The path a report should show and the check should read:
    /// repository-relative when it stays inside the checkout, absolute
    /// otherwise. Docs-policy resolution passes an absolute path through.
    pub(super) display: PathBuf,
    /// The transition warning, when a root-relative spelling was accepted.
    pub(super) warning: Option<String>,
}

/// Resolve every explicit argument, preserving order.
pub(super) fn resolve_explicit_paths(
    repo_root: &Path,
    invocation_cwd: &Path,
    raw_paths: &[PathBuf],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut resolved = Vec::with_capacity(raw_paths.len());
    let mut warnings = Vec::new();
    for raw in raw_paths {
        let path = resolve_explicit_path(repo_root, invocation_cwd, raw);
        if let Some(warning) = path.warning {
            warnings.push(warning);
        }
        resolved.push(path.display);
    }
    (resolved, warnings)
}

/// Resolve one optional `--dir`/`--index`/`--file` style argument.
pub(super) fn resolve_explicit_optional(
    repo_root: &Path,
    invocation_cwd: &Path,
    raw: Option<&PathBuf>,
) -> (Option<PathBuf>, Vec<String>) {
    match raw {
        Some(path) => {
            let (paths, warnings) =
                resolve_explicit_paths(repo_root, invocation_cwd, std::slice::from_ref(path));
            (paths.into_iter().next(), warnings)
        }
        None => (None, Vec::new()),
    }
}

/// Resolve one explicit path argument against the invoking task's cwd.
pub(super) fn resolve_explicit_path(
    repo_root: &Path,
    invocation_cwd: &Path,
    raw: &Path,
) -> ResolvedDocsPath {
    if raw.is_absolute() {
        return ResolvedDocsPath {
            display: display_path(repo_root, raw),
            warning: None,
        };
    }

    let cwd_relative = normalize_components(&invocation_cwd.join(raw));
    if cwd_relative.exists() {
        return ResolvedDocsPath {
            display: display_path(repo_root, &cwd_relative),
            warning: None,
        };
    }

    let root_relative = normalize_components(&repo_root.join(raw));
    if invocation_cwd != repo_root && root_relative.exists() {
        let display = display_path(repo_root, &root_relative);
        let cwd_spelling = cwd_relative_spelling(repo_root, invocation_cwd, &display);
        return ResolvedDocsPath {
            display: display.clone(),
            warning: Some(format!(
                "`{}` does not exist under the task cwd `{}`; using the repository-root path `{}` for this release. Spell it `{{repo}}/{}` (or `{}` from the task cwd) to be cwd-proof.",
                raw.display(),
                invocation_cwd.display(),
                display.display(),
                cwd_spelling.display(),
                cwd_spelling.display(),
            )),
        };
    }

    ResolvedDocsPath {
        display: display_path(repo_root, &cwd_relative),
        warning: None,
    }
}

/// The path spelled relative to the invoking task's cwd.
///
/// `{repo}` is the task's catalog root, which is also the cwd the docs check
/// runs in, so `{repo}/<this spelling>` works under both path rules.
fn cwd_relative_spelling(repo_root: &Path, invocation_cwd: &Path, display: &Path) -> PathBuf {
    let cwd_prefix = invocation_cwd
        .strip_prefix(repo_root)
        .map(normalize_components)
        .unwrap_or_default();
    display
        .strip_prefix(&cwd_prefix)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| display.to_path_buf())
}

/// `repo_root`-relative spelling for a path inside the checkout, else absolute.
fn display_path(repo_root: &Path, path: &Path) -> PathBuf {
    let normalized = normalize_components(path);
    normalized
        .strip_prefix(repo_root)
        .map(Path::to_path_buf)
        .unwrap_or(normalized)
}

fn normalize_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::resolve_explicit_path;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-docs-check-paths-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs/knowledge")).expect("mkdir fixture");
        fs::write(root.join("README.md"), "# Root\n").expect("write root readme");
        fs::write(root.join("docs/README.md"), "# Docs\n").expect("write docs readme");
        fs::write(root.join("docs/knowledge/README.md"), "# Knowledge\n").expect("write knowledge");
        fs::canonicalize(root).expect("canonical fixture")
    }

    #[test]
    fn cwd_relative_argument_wins_over_the_repository_root() {
        let root = fixture("cwd-wins");
        let cwd = root.join("docs");
        let resolved = resolve_explicit_path(&root, &cwd, Path::new("README.md"));
        assert_eq!(resolved.display, Path::new("docs/README.md"));
        assert!(resolved.warning.is_none(), "{:?}", resolved.warning);
    }

    #[test]
    fn missing_cwd_relative_argument_falls_back_to_the_root_relative_path() {
        let root = fixture("transition");
        let cwd = root.join("docs");
        let resolved = resolve_explicit_path(&root, &cwd, Path::new("docs/README.md"));
        assert_eq!(resolved.display, Path::new("docs/README.md"));
        let warning = resolved.warning.expect("transition warning");
        assert!(warning.contains("`{repo}/README.md`"), "{warning}");
        assert!(warning.contains("docs/README.md"), "{warning}");
    }

    #[test]
    fn repository_root_cwd_keeps_every_argument_root_relative() {
        let root = fixture("root-cwd");
        let resolved = resolve_explicit_path(&root, &root, Path::new("docs/README.md"));
        assert_eq!(resolved.display, Path::new("docs/README.md"));
        assert!(resolved.warning.is_none(), "{:?}", resolved.warning);
    }

    #[test]
    fn absolute_argument_reports_its_repository_relative_spelling() {
        let root = fixture("absolute");
        let absolute = root.join("docs/knowledge/README.md");
        let resolved = resolve_explicit_path(&root, &root.join("docs"), &absolute);
        assert_eq!(resolved.display, Path::new("docs/knowledge/README.md"));
    }

    #[test]
    fn missing_argument_keeps_the_cwd_relative_spelling_for_the_failure() {
        let root = fixture("missing");
        let cwd = root.join("docs");
        let resolved = resolve_explicit_path(&root, &cwd, Path::new("gone.md"));
        assert_eq!(resolved.display, Path::new("docs/gone.md"));
        assert!(resolved.warning.is_none(), "{:?}", resolved.warning);
    }

    #[test]
    fn argument_outside_the_checkout_stays_absolute() {
        let root = fixture("outside");
        let outside = fixture("outside-other");
        let resolved = resolve_explicit_path(&root, &root, &outside.join("docs/README.md"));
        assert_eq!(resolved.display, outside.join("docs/README.md"));
    }
}
