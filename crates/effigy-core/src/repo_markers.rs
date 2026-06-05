//! Stable Effigy repository marker names and pure filename predicates.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const TASK_MANIFEST_FILE: &str = "effigy.toml";
pub const LOCAL_OVERLAY_FILE: &str = "effigy.local.toml";
pub const LOCAL_OVERLAY_GITIGNORE_ALIASES: [&str; 2] = [LOCAL_OVERLAY_FILE, "/effigy.local.toml"];

pub const ROOT_MARKERS: [&str; 5] = [
    TASK_MANIFEST_FILE,
    "package.json",
    "composer.json",
    "Cargo.toml",
    ".git",
];

pub fn task_manifest_path(root: &Path) -> PathBuf {
    root.join(TASK_MANIFEST_FILE)
}

pub fn has_task_manifest(root: &Path) -> bool {
    task_manifest_path(root).is_file()
}

pub fn is_effigy_config_filename(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name == TASK_MANIFEST_FILE || name.starts_with("effigy."))
}
