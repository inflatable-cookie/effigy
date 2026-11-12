use std::path::Path;

use super::super::ScanError;

/// The validated text form of a walk-relative path, for glob matching and
/// finding paths. Non-UTF-8 paths are a typed `ScanError::NonUtf8Path`
/// instead of a lossy replacement, so matching never runs on `U+FFFD`.
pub fn rel_path_str(path: &Path) -> Result<String, ScanError> {
    path.to_str()
        .map(|text| text.replace('\\', "/"))
        .ok_or_else(|| ScanError::non_utf8_path(path.to_string_lossy()))
}

/// Lossy display form, for human-readable finding paths only. Never control
/// flow: matching uses [`rel_path_str`].
pub fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn rebase_finding_path(target_root: &Path, root: &Path, finding_path: &str) -> String {
    let root_rel = root
        .strip_prefix(target_root)
        .ok()
        .map(normalize_rel_path)
        .unwrap_or_default();
    if root_rel.is_empty() || root_rel == "." {
        return finding_path.to_owned();
    }
    format!("{root_rel}/{finding_path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn non_utf8_walk_path_is_a_typed_refusal() {
        use std::os::unix::ffi::OsStrExt;
        // The walker feeds every relative path through `rel_path_str`, so
        // a non-UTF-8 path fails closed as `NonUtf8Path` instead of
        // silently matching on a lossy `U+FFFD` replacement. (APFS rejects
        // non-UTF-8 names on disk, so this pins the in-memory refusal the
        // walk depends on.)
        let bad = Path::new(std::ffi::OsStr::from_bytes(b"src/\xff\xfe.rs"));
        let error = rel_path_str(bad).expect_err("non-UTF-8 path must fail");
        assert!(
            matches!(error, ScanError::NonUtf8Path { .. }),
            "got: {error}"
        );
        assert_eq!(
            rel_path_str(Path::new("src/demo.rs")).expect("utf-8"),
            "src/demo.rs"
        );
    }
}
