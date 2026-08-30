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
    // Prune skipped directories instead of descending and filtering per file:
    // a single installed `node_modules` tree otherwise dominates every walk,
    // and every graph query pays for one.
    walk.filter_entry(|entry| {
        !entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false)
            || entry
                .file_name()
                .to_str()
                .is_none_or(|name| !SKIPPED_DIR_SEGMENTS.contains(&name))
    });
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

/// Directory names the graph never indexes.
///
/// Two families, both matched on *any* path segment rather than only the repo
/// root — a monorepo installs `node_modules` and emits `dist` per package, so
/// a root-only prefix check left every nested copy in the index:
///
/// - Effigy/VCS internals (`.git`, `.effigy`)
/// - dependency and build output (installed packages, compiled artifacts,
///   framework output directories, coverage reports)
///
/// Ambiguous names that are commonly hand-written source (`build`, `out`,
/// `lib`) stay indexable on purpose.
const SKIPPED_DIR_SEGMENTS: &[&str] = &[
    ".effigy",
    ".git",
    ".next",
    ".nuxt",
    ".output",
    ".parcel-cache",
    ".svelte-kit",
    ".turbo",
    ".venv",
    "__pycache__",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

pub(crate) fn should_skip_path(relative_path: &str) -> bool {
    relative_path
        .split('/')
        .any(|segment| SKIPPED_DIR_SEGMENTS.contains(&segment))
}

#[cfg(test)]
mod tests {
    use super::should_skip_path;

    #[test]
    fn should_skip_path_skips_effigy_internal_tree() {
        assert!(should_skip_path(".effigy"));
        assert!(should_skip_path(".effigy/graph/graph.db"));
        assert!(should_skip_path(".effigy/runtime/session.json"));
        assert!(!should_skip_path("src/lib.rs"));
    }

    #[test]
    fn should_skip_path_skips_nested_installed_and_built_output() {
        assert!(should_skip_path("apps/web/node_modules/pkg/index.js"));
        assert!(should_skip_path("packages/ui/dist/index.js"));
        assert!(should_skip_path(
            "apps/web/.svelte-kit/output/server/index.js"
        ));
        assert!(should_skip_path("services/api/target/debug/build.rs"));
        assert!(should_skip_path("coverage/lcov-report/index.html"));
    }

    #[test]
    fn should_skip_path_keeps_ambiguous_source_directories() {
        assert!(!should_skip_path("src/build/pipeline.ts"));
        assert!(!should_skip_path("packages/core/out/index.ts"));
        assert!(!should_skip_path("crates/effigy-core/src/lib.rs"));
        assert!(!should_skip_path("docs/distribution.md"));
    }
}
