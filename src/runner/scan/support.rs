use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use super::{
    AttentionMarkerCategory, AttentionMarkerPatterns, AttentionMarkerSeverity,
    GeneratedAssetSeverity, GeneratedAssetThresholds, GodFileSeverity, GodFileThresholds,
    RunnerError, DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS,
    DEFAULT_LOCK_FILE_NAMES, GENERATED_ASSET_DIRS, GENERATED_ASSET_NAME_MARKERS, GENERATED_MARKERS,
};

pub(super) fn compile_glob_set(
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

pub(super) fn build_scan_walk(root: &Path, respect_gitignore: bool) -> WalkBuilder {
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

pub(super) fn should_skip_path(
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

pub(super) fn should_skip_generated_asset_path(
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

pub(super) fn is_generated_artifact(rel: &Path, contents: &str) -> bool {
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
    GENERATED_MARKERS
        .iter()
        .any(|marker| sample.contains(marker))
}

pub(super) fn classify_severity(
    code_lines: usize,
    thresholds: &GodFileThresholds,
) -> Option<GodFileSeverity> {
    if code_lines >= thresholds.critical {
        return Some(GodFileSeverity::Critical);
    }
    if code_lines >= thresholds.high {
        return Some(GodFileSeverity::High);
    }
    if code_lines >= thresholds.warn {
        return Some(GodFileSeverity::Warning);
    }
    None
}

pub(super) fn classify_generated_asset_severity(
    bytes: usize,
    thresholds: &GeneratedAssetThresholds,
) -> Option<GeneratedAssetSeverity> {
    if bytes >= thresholds.critical {
        return Some(GeneratedAssetSeverity::Critical);
    }
    if bytes >= thresholds.high {
        return Some(GeneratedAssetSeverity::High);
    }
    if bytes >= thresholds.warn {
        return Some(GeneratedAssetSeverity::Warning);
    }
    None
}

pub(super) fn severity_rank(severity: GodFileSeverity) -> usize {
    match severity {
        GodFileSeverity::Warning => 1,
        GodFileSeverity::High => 2,
        GodFileSeverity::Critical => 3,
    }
}

pub(super) fn generated_asset_severity_rank(severity: GeneratedAssetSeverity) -> usize {
    match severity {
        GeneratedAssetSeverity::Warning => 1,
        GeneratedAssetSeverity::High => 2,
        GeneratedAssetSeverity::Critical => 3,
    }
}

pub(super) fn attention_marker_severity_rank(severity: AttentionMarkerSeverity) -> usize {
    match severity {
        AttentionMarkerSeverity::Warning => 1,
        AttentionMarkerSeverity::High => 2,
        AttentionMarkerSeverity::Critical => 3,
    }
}

pub(super) fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn normalized_scan_roots(target_root: &Path, scan_roots: &[PathBuf]) -> Vec<PathBuf> {
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

pub(super) fn rebase_finding_path(target_root: &Path, root: &Path, finding_path: &str) -> String {
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

pub(super) fn read_asset_sample(path: &Path) -> Result<String, RunnerError> {
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

pub(super) fn compile_attention_marker_patterns(
    patterns: &AttentionMarkerPatterns,
) -> Vec<(AttentionMarkerSeverity, String, String)> {
    let mut compiled = Vec::<(AttentionMarkerSeverity, String, String)>::new();
    for marker in &patterns.critical {
        compiled.push((
            AttentionMarkerSeverity::Critical,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    for marker in &patterns.high {
        compiled.push((
            AttentionMarkerSeverity::High,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    for marker in &patterns.warning {
        compiled.push((
            AttentionMarkerSeverity::Warning,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    compiled
}

pub(super) fn attention_marker_matches_line(raw_line: &str, marker_lower: &str) -> bool {
    let lower_line = raw_line.to_ascii_lowercase();
    if !lower_line.contains(marker_lower) {
        return false;
    }
    if marker_lower.contains("deprecat") {
        return true;
    }
    let trimmed = lower_line.trim_start();
    let comment_prefixes = ["//", "/*", "*", "<!--", "--"];
    if comment_prefixes.iter().any(|prefix| trimmed.starts_with(prefix))
        || (trimmed.starts_with('#') && !trimmed.starts_with("#["))
    {
        return true;
    }
    if let Some(marker_index) = lower_line.find(marker_lower) {
        if comment_prefixes.iter().any(|prefix| {
            lower_line
                .find(prefix)
                .is_some_and(|comment_index| comment_index < marker_index)
        }) {
            return true;
        }
        if let Some(comment_index) = lower_line.find('#') {
            return comment_index < marker_index
                && lower_line
                    .as_bytes()
                    .get(comment_index + 1)
                    .copied()
                    != Some(b'[');
        }
    }
    false
}

pub(super) fn attention_marker_category(marker_lower: &str) -> AttentionMarkerCategory {
    if marker_lower.contains("deprecat") {
        return AttentionMarkerCategory::Deprecation;
    }
    if marker_lower.contains("temporary")
        || marker_lower.contains("workaround")
        || marker_lower.contains("remove before")
        || marker_lower.contains("placeholder")
        || marker_lower.contains("stub")
        || marker_lower.contains("later")
        || marker_lower.contains("tech debt")
    {
        return AttentionMarkerCategory::TemporaryArtifact;
    }
    AttentionMarkerCategory::DeferredWork
}

pub(super) fn generated_asset_reason(rel: &Path, sample: &str) -> Option<String> {
    if is_generated_asset_vendor_path(rel) {
        return Some("vendor-or-build-path".to_owned());
    }
    if let Some(reason) = generated_asset_name_reason(rel) {
        return Some(reason);
    }
    if GENERATED_MARKERS
        .iter()
        .any(|marker| sample.contains(marker))
    {
        return Some("generated-marker".to_owned());
    }
    None
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

pub(super) fn count_code_lines(path: &Path, contents: &str) -> usize {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    match ext {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => {
            count_comment_filtered_lines(contents, Some("#"), None)
        }
        "sql" => count_comment_filtered_lines(contents, Some("--"), Some(("/*", "*/"))),
        "html" | "xml" | "vue" => {
            count_comment_filtered_lines(contents, None, Some(("<!--", "-->")))
        }
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            count_comment_filtered_lines(contents, Some("//"), Some(("/*", "*/")))
        }
        _ => contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
    }
}

fn count_comment_filtered_lines(
    contents: &str,
    line_comment: Option<&str>,
    block_comment: Option<(&str, &str)>,
) -> usize {
    let mut in_block_comment = false;
    let mut count = 0usize;
    for raw_line in contents.lines() {
        let mut i = 0usize;
        let mut has_code = false;
        while i < raw_line.len() {
            if in_block_comment {
                if let Some((_, block_end)) = block_comment {
                    if raw_line[i..].starts_with(block_end) {
                        in_block_comment = false;
                        i += block_end.len();
                        continue;
                    }
                }
                let ch = raw_line[i..]
                    .chars()
                    .next()
                    .expect("unicode-safe block comment advance");
                i += ch.len_utf8();
                continue;
            }
            if let Some((block_start, _)) = block_comment {
                if raw_line[i..].starts_with(block_start) {
                    in_block_comment = true;
                    i += block_start.len();
                    continue;
                }
            }
            if let Some(line_prefix) = line_comment {
                if raw_line[i..].starts_with(line_prefix) {
                    break;
                }
            }
            let ch = raw_line[i..]
                .chars()
                .next()
                .expect("unicode-safe line advance");
            if !ch.is_whitespace() {
                has_code = true;
            }
            i += ch.len_utf8();
        }
        if has_code {
            count += 1;
        }
    }
    count
}

pub(super) fn trim_snippet(line: &str, max_chars: usize) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let prefix = trimmed.chars().take(max_chars.saturating_sub(3)).collect::<String>();
    format!("{prefix}...")
}
