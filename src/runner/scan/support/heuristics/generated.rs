use std::ffi::OsStr;
use std::path::Path;

use crate::runner::scan::model::GeneratedInSrcCategory;

use super::super::{GENERATED_ASSET_DIRS, GENERATED_ASSET_NAME_MARKERS, GENERATED_MARKERS};
use super::strings::mask_string_literals;

pub(in crate::runner) fn is_generated_artifact(rel: &Path, contents: &str) -> bool {
    let lower_name = rel
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower_name.contains(".min.") || lower_name.ends_with(".generated.rs") {
        return true;
    }
    let sample = contents
        .lines()
        .take(24)
        .collect::<Vec<&str>>()
        .join("\n")
        .to_ascii_lowercase();
    has_generated_marker_header(&sample)
}

pub(in crate::runner) fn generated_asset_reason(rel: &Path, sample: &str) -> Option<String> {
    if is_generated_asset_vendor_path(rel) {
        return Some("vendor-or-build-path".to_owned());
    }
    if let Some(reason) = generated_asset_name_reason(rel) {
        return Some(reason);
    }
    if has_generated_marker_header(sample) {
        return Some("generated-marker".to_owned());
    }
    None
}

pub(in crate::runner) fn generated_in_src_reason(
    rel: &Path,
    sample: &str,
) -> Option<(GeneratedInSrcCategory, String)> {
    if has_generated_marker_header(sample) {
        return Some((
            GeneratedInSrcCategory::ContentMarker,
            "generated-marker".to_owned(),
        ));
    }
    let lower_name = rel
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower_name.ends_with(".map") {
        return Some((
            GeneratedInSrcCategory::BundledArtifact,
            "source-map".to_owned(),
        ));
    }
    if lower_name.contains(".min.") {
        return Some((
            GeneratedInSrcCategory::BundledArtifact,
            "minified-asset".to_owned(),
        ));
    }
    if GENERATED_ASSET_NAME_MARKERS
        .iter()
        .any(|marker| lower_name.contains(marker))
        || lower_name.ends_with(".generated.rs")
        || lower_name.contains(".pb.")
        || lower_name.contains(".designer.")
        || lower_name.contains(".g.")
    {
        return Some((
            GeneratedInSrcCategory::GeneratedFilename,
            "filename-marker".to_owned(),
        ));
    }
    if rel.components().any(|component| {
        matches!(
            component.as_os_str().to_string_lossy().as_ref(),
            "generated" | "gen"
        )
    }) {
        return Some((
            GeneratedInSrcCategory::GeneratedPath,
            "generated-path-component".to_owned(),
        ));
    }
    None
}

fn has_generated_marker_header(sample: &str) -> bool {
    sample.lines().take(24).any(generated_marker_header_line)
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

fn is_generated_asset_vendor_path(path: &Path) -> bool {
    path.components().any(|component| {
        GENERATED_ASSET_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
    })
}

fn generated_asset_name_reason(path: &Path) -> Option<String> {
    let lower_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lower_name.ends_with(".map") {
        return Some("source-map".to_owned());
    }
    if lower_name.contains(".min.") {
        return Some("minified-asset".to_owned());
    }
    if GENERATED_ASSET_NAME_MARKERS
        .iter()
        .any(|marker| lower_name.contains(marker))
    {
        return Some("bundled-asset".to_owned());
    }
    None
}
