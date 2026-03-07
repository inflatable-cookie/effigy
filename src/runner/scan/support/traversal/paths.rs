use std::path::Path;

pub(in crate::runner) fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(in crate::runner) fn rebase_finding_path(
    target_root: &Path,
    root: &Path,
    finding_path: &str,
) -> String {
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
