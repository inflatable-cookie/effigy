use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use super::{
    RunnerError, DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS,
    DEFAULT_LOCK_FILE_NAMES,
};

pub(in crate::runner) fn compile_glob_set(
    patterns: &[String],
    label: &str,
) -> Result<Option<GlobSet>, RunnerError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            RunnerError::task_invocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|error| {
        RunnerError::task_invocation(format!("failed to compile `{label}` glob set: {error}"))
    })?;
    Ok(Some(set))
}

pub(in crate::runner) fn build_scan_walk(root: &Path, respect_gitignore: bool) -> WalkBuilder {
    let mut walk = WalkBuilder::new(root);
    let has_git_dir = root.join(".git").exists();
    walk.hidden(false)
        .ignore(respect_gitignore)
        .git_ignore(respect_gitignore)
        .git_exclude(respect_gitignore && has_git_dir)
        .git_global(respect_gitignore)
        .require_git(has_git_dir)
        .parents(respect_gitignore && has_git_dir)
        .follow_links(false);
    if respect_gitignore && !has_git_dir {
        for ignore_name in [".ignore", ".gitignore"] {
            let ignore_path = root.join(ignore_name);
            if ignore_path.is_file() {
                let _ = walk.add_ignore(ignore_path);
            }
        }
    }
    walk
}

pub(in crate::runner) fn should_skip_path(
    rel: &Path,
    rel_str: &str,
    include: Option<&GlobSet>,
    exclude: Option<&GlobSet>,
) -> bool {
    if exclude.is_some_and(|set| set.is_match(rel_str)) {
        return true;
    }
    if rel.components().any(|component| {
        DEFAULT_EXCLUDED_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
    }) {
        return true;
    }
    if let Some(set) = include {
        return !set.is_match(rel_str);
    }
    if is_probable_documentation_path(rel)
        || is_probable_data_path(rel)
        || is_probable_lockfile(rel)
    {
        return true;
    }
    !is_probable_code_file(rel)
}

pub(in crate::runner) fn should_skip_generated_asset_path(
    rel: &Path,
    rel_str: &str,
    include: Option<&GlobSet>,
    exclude: Option<&GlobSet>,
) -> bool {
    if exclude.is_some_and(|set| set.is_match(rel_str)) {
        return true;
    }
    if rel
        .components()
        .any(|component| component.as_os_str() == OsStr::new(".git"))
    {
        return true;
    }
    if let Some(set) = include {
        return !set.is_match(rel_str);
    }
    false
}

pub(in crate::runner) fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(in crate::runner) fn normalized_scan_roots(
    target_root: &Path,
    scan_roots: &[PathBuf],
) -> Vec<PathBuf> {
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

pub(in crate::runner) fn workspace_scan_roots(
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

pub(in crate::runner) fn walk_scan_files<ShouldSkip, Visit>(
    root: &Path,
    skipped_roots: &[PathBuf],
    respect_gitignore: bool,
    include_patterns: &[String],
    exclude_patterns: &[String],
    should_skip: ShouldSkip,
    mut visit: Visit,
) -> Result<(), RunnerError>
where
    ShouldSkip: Fn(&Path, &str, Option<&GlobSet>, Option<&GlobSet>) -> bool,
    Visit: FnMut(&Path, &Path, &str) -> Result<(), RunnerError>,
{
    let include = compile_glob_set(include_patterns, "include")?;
    let exclude = compile_glob_set(exclude_patterns, "exclude")?;
    let walk = build_scan_walk(root, respect_gitignore);

    for entry in walk.build() {
        let entry = entry.map_err(|error| {
            RunnerError::task_invocation(format!(
                "scan walk failed under {}: {error}",
                root.display()
            ))
        })?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        if skipped_roots
            .iter()
            .any(|skip_root| path.starts_with(skip_root))
        {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(path);
        let rel_str = normalize_rel_path(rel);
        if rel_str.is_empty() || should_skip(rel, &rel_str, include.as_ref(), exclude.as_ref()) {
            continue;
        }

        visit(path, rel, &rel_str)?;
    }

    Ok(())
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

pub(in crate::runner) fn read_asset_sample(path: &Path) -> Result<String, RunnerError> {
    let mut file = File::open(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "scan sample read failed for {}: {error}",
            path.display()
        ))
    })?;
    let mut sample = vec![0u8; 16 * 1024];
    let read = file.read(&mut sample).map_err(|error| {
        RunnerError::task_invocation(format!(
            "scan sample read failed for {}: {error}",
            path.display()
        ))
    })?;
    sample.truncate(read);
    Ok(String::from_utf8_lossy(&sample).to_ascii_lowercase())
}

fn is_probable_code_file(path: &Path) -> bool {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    matches!(
        ext,
        "c" | "cc"
            | "cpp"
            | "cs"
            | "css"
            | "go"
            | "h"
            | "hpp"
            | "java"
            | "js"
            | "jsx"
            | "kt"
            | "kts"
            | "lua"
            | "m"
            | "mm"
            | "php"
            | "py"
            | "rb"
            | "rs"
            | "scala"
            | "sc"
            | "sh"
            | "sql"
            | "swift"
            | "ts"
            | "tsx"
            | "vue"
            | "zsh"
    )
}

fn is_probable_documentation_path(path: &Path) -> bool {
    path.components().any(|component| {
        DEFAULT_DOC_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
    })
}

fn is_probable_data_path(path: &Path) -> bool {
    path.components().any(|component| {
        DEFAULT_DATA_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
    })
}

fn is_probable_lockfile(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(|name| DEFAULT_LOCK_FILE_NAMES.contains(&name.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
