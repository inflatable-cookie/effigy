use std::ffi::OsStr;
use std::path::Path;

use super::{
    AttentionMarkerCategory, AttentionMarkerPatterns, AttentionMarkerSeverity,
    CommentRatioSeverity, CommentRatioThresholds, DuplicateBlockSeverity, DuplicateBlockThresholds,
    GeneratedAssetSeverity, GeneratedAssetThresholds, GodFileSeverity, GodFileThresholds,
    GENERATED_ASSET_DIRS, GENERATED_ASSET_NAME_MARKERS, GENERATED_MARKERS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct NormalizedCodeLine {
    pub(in crate::runner) line_number: usize,
    pub(in crate::runner) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct CommentRatioCounts {
    pub(in crate::runner) code_lines: usize,
    pub(in crate::runner) comment_lines: usize,
}

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
    GENERATED_MARKERS
        .iter()
        .any(|marker| sample.contains(marker))
}

pub(in crate::runner) fn classify_severity(
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

pub(in crate::runner) fn classify_generated_asset_severity(
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

pub(in crate::runner) fn classify_duplicate_block_severity(
    block_lines: usize,
    thresholds: &DuplicateBlockThresholds,
) -> Option<DuplicateBlockSeverity> {
    if block_lines >= thresholds.critical {
        return Some(DuplicateBlockSeverity::Critical);
    }
    if block_lines >= thresholds.high {
        return Some(DuplicateBlockSeverity::High);
    }
    if block_lines >= thresholds.warn {
        return Some(DuplicateBlockSeverity::Warning);
    }
    None
}

pub(in crate::runner) fn classify_comment_ratio_severity(
    ratio: f64,
    thresholds: &CommentRatioThresholds,
) -> Option<CommentRatioSeverity> {
    if ratio >= thresholds.critical {
        return Some(CommentRatioSeverity::Critical);
    }
    if ratio >= thresholds.high {
        return Some(CommentRatioSeverity::High);
    }
    if ratio >= thresholds.warn {
        return Some(CommentRatioSeverity::Warning);
    }
    None
}

pub(in crate::runner) fn severity_rank(severity: GodFileSeverity) -> usize {
    match severity {
        GodFileSeverity::Warning => 1,
        GodFileSeverity::High => 2,
        GodFileSeverity::Critical => 3,
    }
}

pub(in crate::runner) fn generated_asset_severity_rank(severity: GeneratedAssetSeverity) -> usize {
    match severity {
        GeneratedAssetSeverity::Warning => 1,
        GeneratedAssetSeverity::High => 2,
        GeneratedAssetSeverity::Critical => 3,
    }
}

pub(in crate::runner) fn attention_marker_severity_rank(
    severity: AttentionMarkerSeverity,
) -> usize {
    match severity {
        AttentionMarkerSeverity::Warning => 1,
        AttentionMarkerSeverity::High => 2,
        AttentionMarkerSeverity::Critical => 3,
    }
}

pub(in crate::runner) fn duplicate_block_severity_rank(severity: DuplicateBlockSeverity) -> usize {
    match severity {
        DuplicateBlockSeverity::Warning => 1,
        DuplicateBlockSeverity::High => 2,
        DuplicateBlockSeverity::Critical => 3,
    }
}

pub(in crate::runner) fn comment_ratio_severity_rank(severity: CommentRatioSeverity) -> usize {
    match severity {
        CommentRatioSeverity::Warning => 1,
        CommentRatioSeverity::High => 2,
        CommentRatioSeverity::Critical => 3,
    }
}

pub(in crate::runner) fn compile_attention_marker_patterns(
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

pub(in crate::runner) fn attention_marker_matches_line(raw_line: &str, marker_lower: &str) -> bool {
    let lower_line = raw_line.to_ascii_lowercase();
    if !lower_line.contains(marker_lower) {
        return false;
    }
    if marker_lower.contains("deprecat") {
        return true;
    }
    let trimmed = lower_line.trim_start();
    let comment_prefixes = ["//", "/*", "*", "<!--", "--"];
    if comment_prefixes
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
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
                && lower_line.as_bytes().get(comment_index + 1).copied() != Some(b'[');
        }
    }
    false
}

pub(in crate::runner) fn attention_marker_category(marker_lower: &str) -> AttentionMarkerCategory {
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

pub(in crate::runner) fn generated_asset_reason(rel: &Path, sample: &str) -> Option<String> {
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

pub(in crate::runner) fn count_code_lines(path: &Path, contents: &str) -> usize {
    normalized_code_lines(path, contents).len()
}

pub(in crate::runner) fn normalized_code_lines(
    path: &Path,
    contents: &str,
) -> Vec<NormalizedCodeLine> {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    match ext {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => {
            collect_comment_filtered_lines(contents, Some("#"), None)
        }
        "sql" => collect_comment_filtered_lines(contents, Some("--"), Some(("/*", "*/"))),
        "html" | "xml" | "vue" => {
            collect_comment_filtered_lines(contents, None, Some(("<!--", "-->")))
        }
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            collect_comment_filtered_lines(contents, Some("//"), Some(("/*", "*/")))
        }
        _ => contents
            .lines()
            .enumerate()
            .filter_map(|(line_number, line)| {
                let normalized = normalize_code_fragment(line);
                (!normalized.is_empty()).then(|| NormalizedCodeLine {
                    line_number: line_number + 1,
                    text: normalized,
                })
            })
            .collect(),
    }
}

pub(in crate::runner) fn comment_ratio_counts(path: &Path, contents: &str) -> CommentRatioCounts {
    let ext = path.extension().and_then(OsStr::to_str).unwrap_or_default();
    match ext {
        "py" | "rb" | "sh" | "toml" | "yaml" | "yml" | "zsh" => {
            count_code_and_comment_lines(contents, Some("#"), None)
        }
        "sql" => count_code_and_comment_lines(contents, Some("--"), Some(("/*", "*/"))),
        "html" | "xml" | "vue" => {
            count_code_and_comment_lines(contents, None, Some(("<!--", "-->")))
        }
        "c" | "cc" | "cpp" | "cs" | "css" | "go" | "h" | "hpp" | "java" | "js" | "jsx" | "kt"
        | "kts" | "m" | "mm" | "php" | "rs" | "scala" | "sc" | "swift" | "ts" | "tsx" => {
            count_code_and_comment_lines(contents, Some("//"), Some(("/*", "*/")))
        }
        _ => CommentRatioCounts {
            code_lines: contents
                .lines()
                .filter(|line| !normalize_code_fragment(line).is_empty())
                .count(),
            comment_lines: 0,
        },
    }
}

pub(in crate::runner) fn trim_snippet(line: &str, max_chars: usize) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let prefix = trimmed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    format!("{prefix}...")
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

fn collect_comment_filtered_lines(
    contents: &str,
    line_comment: Option<&str>,
    block_comment: Option<(&str, &str)>,
) -> Vec<NormalizedCodeLine> {
    let mut in_block_comment = false;
    let mut lines = Vec::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        let mut i = 0usize;
        let mut fragments = String::new();
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
            fragments.push(ch);
            i += ch.len_utf8();
        }
        let normalized = normalize_code_fragment(&fragments);
        if !normalized.is_empty() {
            lines.push(NormalizedCodeLine {
                line_number: line_number + 1,
                text: normalized,
            });
        }
    }
    lines
}

fn count_code_and_comment_lines(
    contents: &str,
    line_comment: Option<&str>,
    block_comment: Option<(&str, &str)>,
) -> CommentRatioCounts {
    let mut in_block_comment = false;
    let mut code_lines = 0usize;
    let mut comment_lines = 0usize;

    for raw_line in contents.lines() {
        let mut i = 0usize;
        let mut fragments = String::new();
        let mut saw_comment = false;
        while i < raw_line.len() {
            if in_block_comment {
                saw_comment = true;
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
                    saw_comment = true;
                    i += block_start.len();
                    continue;
                }
            }
            if let Some(line_prefix) = line_comment {
                if raw_line[i..].starts_with(line_prefix) {
                    saw_comment = true;
                    break;
                }
            }
            let ch = raw_line[i..]
                .chars()
                .next()
                .expect("unicode-safe line advance");
            fragments.push(ch);
            i += ch.len_utf8();
        }
        let normalized_code = normalize_code_fragment(&fragments);
        if !normalized_code.is_empty() {
            code_lines += 1;
        } else if saw_comment {
            comment_lines += 1;
        }
    }

    CommentRatioCounts {
        code_lines,
        comment_lines,
    }
}

fn normalize_code_fragment(fragment: &str) -> String {
    fragment.split_whitespace().collect::<Vec<&str>>().join(" ")
}
