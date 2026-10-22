use std::ffi::OsStr;
use std::path::Path;

use super::strings::mask_string_literals;
use crate::constants::{GENERATED_MARKERS, GENERATED_PATH_COMPONENTS};

const GENERATED_NAME_MARKERS: &[&str] = &[
    ".bundle.",
    ".chunk.",
    ".designer.",
    ".g.",
    ".generated.",
    ".min.",
    ".pb.",
    "_pb2.py",
];

/// True when a file is generated, derived or vendored output rather than
/// maintained source. Both content scanners skip these files.
pub fn is_generated_artifact(rel: &Path, contents: &str) -> bool {
    has_generated_path_component(rel)
        || has_generated_file_name(rel)
        || has_generated_header(contents)
}

fn has_generated_path_component(rel: &Path) -> bool {
    rel.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        GENERATED_PATH_COMPONENTS.contains(&name.as_ref())
    })
}

fn has_generated_file_name(rel: &Path) -> bool {
    let lower_name = rel
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    GENERATED_NAME_MARKERS
        .iter()
        .any(|marker| lower_name.contains(marker))
}

fn has_generated_header(contents: &str) -> bool {
    contents.lines().take(24).any(generated_marker_header_line)
}

fn generated_marker_header_line(raw_line: &str) -> bool {
    let lower_line = mask_string_literals(raw_line).to_ascii_lowercase();
    let trimmed = lower_line.trim_start();
    let comment_prefixes = ["//", "/*", "*", "<!--", "--", ";"];
    let is_comment_like = comment_prefixes
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
        || (trimmed.starts_with('#') && !trimmed.starts_with("#["));

    is_comment_like
        && GENERATED_MARKERS
            .iter()
            .any(|marker| lower_line.contains(marker))
}
