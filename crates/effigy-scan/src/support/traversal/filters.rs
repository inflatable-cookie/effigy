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
    // Component matching runs on `rel_str`, the validated UTF-8 form of the
    // walk path (the walk rejects non-UTF-8 paths as typed errors), never
    // on a lossy replacement.
    if rel_str.split('/').any(|component| {
        DEFAULT_EXCLUDED_DIRS.contains(&component)
            || GENERATED_PATH_COMPONENTS.contains(&component)
    }) {
        return true;
    }
    if let Some(set) = include {
        return !set.is_match(rel_str);
    }
    if is_probable_documentation_path(rel_str)
        || is_probable_data_path(rel_str)
        || is_probable_lockfile(rel)
    {
        return true;
    }
    !is_probable_code_file(rel)
}

/// Path filter for the shared doctor inventory walk. It keeps every file except
/// Effigy's own internal state so each enabled scanner applies its own filter.
pub fn should_skip_inventory_path(
    _rel: &Path,
    rel_str: &str,
    include: Option<&GlobSet>,
    exclude: Option<&GlobSet>,
) -> bool {
    if exclude.is_some_and(|set| set.is_match(rel_str)) {
        return true;
    }
    // Same validated-text rule as `should_skip_path`: match on `rel_str`.
    if rel_str
        .split('/')
        .any(|component| component == ".git" || component == ".effigy")
    {
        return true;
    }
    if let Some(set) = include {
        return !set.is_match(rel_str);
    }
    false
}

fn is_probable_code_file(path: &Path) -> bool {
    // Non-UTF-8 walk paths never reach here: the walk rejects them as typed
    // `ScanError::NonUtf8Path`. An absent extension is simply not a
    // probable code file, stated explicitly instead of defaulted.
    let Some(ext) = path.extension().and_then(OsStr::to_str) else {
        return false;
    };
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

fn is_probable_documentation_path(rel_str: &str) -> bool {
    rel_str
        .split('/')
        .any(|component| DEFAULT_DOC_DIRS.contains(&component))
}

fn is_probable_data_path(rel_str: &str) -> bool {
    rel_str
        .split('/')
        .any(|component| DEFAULT_DATA_DIRS.contains(&component))
}

fn is_probable_lockfile(path: &Path) -> bool {
    // Non-UTF-8 file names never reach here (typed walk rejection above);
    // an absent name is not a lockfile.
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    DEFAULT_LOCK_FILE_NAMES.contains(&name.to_ascii_lowercase().as_str())
}
