use std::path::{Component, Path, PathBuf};

use super::super::super::LoadedCatalog;

pub(super) fn resolve_catalog_by_prefix<'a>(
    prefix_value: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .find(|catalog| catalog.alias == prefix_value)
        .or_else(|| resolve_catalog_by_relative_prefix(prefix_value, catalogs, cwd))
}

pub(super) fn selection_evidence_for_prefix(prefix_value: &str, catalog: &LoadedCatalog) -> String {
    if catalog.alias == prefix_value {
        format!("selected catalog via explicit prefix `{prefix_value}`")
    } else {
        format!(
            "selected catalog via relative prefix `{prefix_value}` -> `{}`",
            catalog.alias
        )
    }
}

fn resolve_catalog_by_relative_prefix<'a>(
    prefix_value: &str,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Option<&'a LoadedCatalog> {
    if !is_relative_path_prefix(prefix_value) {
        return None;
    }

    let resolved = normalize_path(if Path::new(prefix_value).is_absolute() {
        PathBuf::from(prefix_value)
    } else {
        cwd.join(prefix_value)
    });

    catalogs
        .iter()
        .find(|catalog| normalize_path(catalog.catalog_root.clone()) == resolved)
}

fn is_relative_path_prefix(prefix_value: &str) -> bool {
    prefix_value.starts_with('.')
        || prefix_value.starts_with('/')
        || prefix_value.contains('/')
        || prefix_value.contains('\\')
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
