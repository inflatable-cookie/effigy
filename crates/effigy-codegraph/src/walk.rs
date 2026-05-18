use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ignore::WalkBuilder;

use crate::error::CodeGraphError;
use crate::support::{language_id_for_path, normalize_rel_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    pub path: PathBuf,
    pub relative_path: String,
    pub language_id: String,
    pub modified_unix_ms: u128,
    pub byte_size: u64,
}

pub fn scan_repo_files(repo_root: &Path) -> Result<Vec<ScanEntry>, CodeGraphError> {
    let mut entries = Vec::new();
    let has_git_dir = repo_root.join(".git").exists();
    let mut walk = WalkBuilder::new(repo_root);
    walk.hidden(false)
        .ignore(true)
        .git_ignore(true)
        .git_exclude(has_git_dir)
        .git_global(true)
        .require_git(has_git_dir)
        .parents(has_git_dir)
        .follow_links(false);
    for entry in walk.build() {
        let entry = entry.map_err(|error| {
            CodeGraphError::validation(format!(
                "graph walk failed under {}: {error}",
                repo_root.display()
            ))
        })?;
        if !entry
            .file_type()
            .map(|file_type| file_type.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(repo_root).unwrap_or(path);
        let relative_path = normalize_rel_path(rel);
        if should_skip_path(&relative_path) {
            continue;
        }
        let Some(language_id) = language_id_for_path(&relative_path) else {
            continue;
        };
        let metadata = fs::metadata(path)?;
        let modified_unix_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        entries.push(ScanEntry {
            path: path.to_path_buf(),
            relative_path,
            language_id: language_id.to_owned(),
            modified_unix_ms,
            byte_size: metadata.len(),
        });
    }
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(entries)
}

pub(crate) fn should_skip_path(relative_path: &str) -> bool {
    relative_path == ".git"
        || relative_path.starts_with(".git/")
        || relative_path == "target"
        || relative_path.starts_with("target/")
        || relative_path == "node_modules"
        || relative_path.starts_with("node_modules/")
        || relative_path == "vendor"
        || relative_path.starts_with("vendor/")
        || relative_path == ".effigy/graph"
        || relative_path.starts_with(".effigy/graph/")
        || relative_path.starts_with(".effigy/runtime/")
        || relative_path == ".effigy/runtime"
}
