use std::path::{Component, Path, PathBuf};

pub(super) fn split_catalog_env_reference(entry_ref: &str) -> Option<(&str, &str)> {
    let split_at = entry_ref.rfind(['/', '\\'])?;
    let (catalog_path, env_key_with_sep) = entry_ref.split_at(split_at);
    let env_key = env_key_with_sep
        .strip_prefix('/')
        .or_else(|| env_key_with_sep.strip_prefix('\\'))?;
    if catalog_path.is_empty() || env_key.is_empty() {
        return None;
    }
    Some((catalog_path, env_key))
}

pub(super) fn resolve_catalog_reference_root(catalog_path: &str, repo_root: &Path) -> PathBuf {
    let resolved = if Path::new(catalog_path).is_absolute() {
        PathBuf::from(catalog_path)
    } else {
        repo_root.join(catalog_path)
    };
    normalize_path(&resolved)
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
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
