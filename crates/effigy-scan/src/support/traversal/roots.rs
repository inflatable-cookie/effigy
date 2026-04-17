use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn normalized_scan_roots(target_root: &Path, scan_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = BTreeSet::<PathBuf>::new();
    for root in scan_roots {
        if root == target_root || root.starts_with(target_root) {
            unique.insert(root.clone());
        }
    }
    if unique.is_empty() {
        unique.insert(target_root.to_path_buf());
    }
    unique.into_iter().collect()
}

pub fn workspace_scan_roots(
    target_root: &Path,
    scan_roots: &[PathBuf],
) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let unique_roots = normalized_scan_roots(target_root, scan_roots);
    unique_roots
        .iter()
        .map(|root| {
            let skipped_roots = unique_roots
                .iter()
                .filter(|candidate| *candidate != root && candidate.starts_with(root))
                .cloned()
                .collect::<Vec<PathBuf>>();
            (root.clone(), skipped_roots)
        })
        .collect()
}
