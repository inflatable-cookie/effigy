use std::ffi::OsStr;
use std::path::Path;

use globset::GlobSet;

use super::super::{
    DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS, DEFAULT_LOCK_FILE_NAMES,
};

pub fn should_skip_path(
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

pub fn should_skip_generated_asset_path(
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
