//! Path identity for comparisons.
//!
//! Every `--repo` comparison and every invocation cwd resolves through here, so
//! one implementation owns "which path is this" and a path that could not be
//! resolved stays visible instead of being folded into the uncanonicalised one.

use std::io;
use std::path::{Path, PathBuf};

/// A path resolved to the identity comparisons use.
#[derive(Debug)]
pub enum ResolvedPath {
    /// The filesystem resolved the path.
    Canonical(PathBuf),
    /// The path could not be resolved (it may not exist yet). The original path
    /// is kept with the reason, so a caller can refuse when the difference
    /// matters instead of silently comparing raw text.
    Unresolved { path: PathBuf, error: io::Error },
}

impl ResolvedPath {
    /// The path comparisons use: the resolved one, or the original when it
    /// could not be resolved.
    pub fn path(&self) -> &Path {
        match self {
            Self::Canonical(path) => path,
            Self::Unresolved { path, .. } => path,
        }
    }

    /// Take the path comparisons use.
    pub fn into_path(self) -> PathBuf {
        match self {
            Self::Canonical(path) => path,
            Self::Unresolved { path, .. } => path,
        }
    }
}

/// Resolve `path`, keeping the reason when the filesystem cannot.
pub fn canonicalize_or_original(path: &Path) -> ResolvedPath {
    match std::fs::canonicalize(path) {
        Ok(resolved) => ResolvedPath::Canonical(resolved),
        Err(error) => ResolvedPath::Unresolved {
            path: path.to_path_buf(),
            error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{canonicalize_or_original, ResolvedPath};
    use std::io::ErrorKind;

    /// A resolvable path reports its canonical identity, including the `..`
    /// segments `canonicalize` removes.
    #[test]
    fn resolvable_path_reports_its_canonical_identity() {
        let root = tempfile::tempdir().expect("temp dir");
        let nested = root.path().join("nested");
        std::fs::create_dir(&nested).expect("nested dir");
        let requested = nested.join("..");

        let resolved = canonicalize_or_original(&requested);

        assert!(matches!(resolved, ResolvedPath::Canonical(_)));
        assert_eq!(resolved.path(), root.path().canonicalize().expect("root"));
    }

    /// The defect this replaces returned the uncanonicalised path with no way
    /// to tell why; the reason now travels with the path.
    #[test]
    fn unresolvable_path_keeps_the_reason_instead_of_only_the_original() {
        let root = tempfile::tempdir().expect("temp dir");
        let missing = root.path().join("does-not-exist");

        let resolved = canonicalize_or_original(&missing);

        assert_eq!(resolved.path(), missing);
        let ResolvedPath::Unresolved { error, .. } = &resolved else {
            panic!("a missing path must stay unresolved");
        };
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }
}
