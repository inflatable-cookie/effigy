use std::ffi::OsStr;
use std::path::Path;

use globset::GlobSet;

use super::super::{
    DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS, DEFAULT_LOCK_FILE_NAMES,
    GENERATED_PATH_COMPONENTS,
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
        let name = component.as_os_str().to_string_lossy();
        DEFAULT_EXCLUDED_DIRS.contains(&name.as_ref())
            || GENERATED_PATH_COMPONENTS.contains(&name.as_ref())
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

/// Path filter for the shared doctor inventory walk. It keeps every file except
/// Effigy's own internal state so each enabled scanner applies its own filter.
pub fn should_skip_inventory_path(
    rel: &Path,
    rel_str: &str,
    include: Option<&GlobSet>,
    exclude: Option<&GlobSet>,
) -> bool {
    if exclude.is_some_and(|set| set.is_match(rel_str)) {
        return true;
    }
    if rel.components().any(|component| {
        let name = component.as_os_str();
        name == OsStr::new(".git") || name == OsStr::new(".effigy")
    }) {
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
