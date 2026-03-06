use std::path::{Path, PathBuf};

use globset::Glob;
use walkdir::WalkDir;

use crate::runner::error::RunnerError;

pub(super) fn resolve_declared_matches(
    catalog_root: &Path,
    declaration: &str,
) -> Result<Vec<PathBuf>, RunnerError> {
    if has_glob_magic(declaration) {
        return resolve_glob_matches(catalog_root, declaration);
    }
    Ok(vec![catalog_root.join(declaration)])
}

pub(super) fn render_relative_or_absolute(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn has_glob_magic(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[') || value.contains('{')
}

fn resolve_glob_matches(catalog_root: &Path, pattern: &str) -> Result<Vec<PathBuf>, RunnerError> {
    let glob = Glob::new(pattern).map_err(|error| {
        RunnerError::task_invocation(format!(
            "invalid cache declaration glob `{pattern}`: {error}"
        ))
    })?;
    let matcher = glob.compile_matcher();
    let mut matches = WalkDir::new(catalog_root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path == catalog_root {
                return None;
            }
            let relative = path.strip_prefix(catalog_root).ok()?;
            let relative_rendered = relative.to_string_lossy().replace('\\', "/");
            matcher
                .is_match(&relative_rendered)
                .then_some(path.to_path_buf())
        })
        .collect::<Vec<PathBuf>>();
    matches.sort();
    Ok(matches)
}
