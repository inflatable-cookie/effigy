use crate::runner::scan::model::{
    AttentionMarkerCategory, AttentionMarkerPatterns, AttentionMarkerSeverity,
    StaleSuppressionCategory, StaleSuppressionPatterns, StaleSuppressionSeverity,
};

use super::strings::mask_string_literals;

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

pub(in crate::runner) fn compile_stale_suppression_patterns(
    patterns: &StaleSuppressionPatterns,
) -> Vec<(StaleSuppressionSeverity, String, String)> {
    let mut compiled = Vec::<(StaleSuppressionSeverity, String, String)>::new();
    for marker in &patterns.critical {
        compiled.push((
            StaleSuppressionSeverity::Critical,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    for marker in &patterns.high {
        compiled.push((
            StaleSuppressionSeverity::High,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    for marker in &patterns.warning {
        compiled.push((
            StaleSuppressionSeverity::Warning,
            marker.clone(),
            marker.to_ascii_lowercase(),
        ));
    }
    compiled
}

pub(in crate::runner) fn attention_marker_matches_line(raw_line: &str, marker_lower: &str) -> bool {
    let lower_line = mask_string_literals(raw_line).to_ascii_lowercase();
    if !lower_line.contains(marker_lower) {
        return false;
    }
    let comment_match = attention_marker_matches_comment_segment(&lower_line, marker_lower);
    if marker_lower.starts_with('@') {
        return comment_match || lower_line.contains("#[deprecated");
    }
    if marker_lower.contains("deprecat") {
        return lower_line.contains("#[deprecated") || comment_match;
    }
    comment_match
}

fn attention_marker_matches_comment_segment(lower_line: &str, marker_lower: &str) -> bool {
    let trimmed = lower_line.trim_start();
    strip_comment_prefix(trimmed).is_some_and(|body| marker_starts_comment_segment(body, marker_lower))
        || inline_comment_body(lower_line)
            .is_some_and(|body| marker_starts_comment_segment(body, marker_lower))
}

fn inline_comment_body(lower_line: &str) -> Option<&str> {
    for prefix in ["//", "/*", "<!--"] {
        if let Some(index) = lower_line.find(prefix) {
            let candidate = &lower_line[index + prefix.len()..];
            if !candidate.is_empty() {
                return Some(candidate);
            }
        }
    }
    None
}

fn strip_comment_prefix(trimmed: &str) -> Option<&str> {
    for prefix in ["//", "/*", "*", "<!--", "--"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    if trimmed.starts_with("#[") || trimmed.starts_with("##") {
        return None;
    }
    trimmed.strip_prefix('#')
}

fn marker_starts_comment_segment(body: &str, marker_lower: &str) -> bool {
    let body = body.trim_start();
    let body = body.trim_start_matches(['-', '*', '[', '(']);
    let body = body.trim_start();
    let Some(rest) = body.strip_prefix(marker_lower) else {
        return false;
    };
    rest.is_empty()
        || rest.starts_with(':')
        || rest.starts_with(' ')
        || rest.starts_with(']')
        || rest.starts_with(')')
        || rest.starts_with('-')
}

pub(in crate::runner) fn stale_suppression_matches_line(
    raw_line: &str,
    marker_lower: &str,
) -> bool {
    let lower_line = mask_string_literals(raw_line).to_ascii_lowercase();
    match marker_lower {
        "eslint-disable" => {
            lower_line.contains("eslint-disable")
                && !lower_line.contains("eslint-disable-next-line")
                && !lower_line.contains("eslint-disable-line")
        }
        "#[allow(" => lower_line.contains("#[allow("),
        "#[expect(" => lower_line.contains("#[expect("),
        "#[allow(warnings)]" => lower_line.contains("#[allow(warnings)]"),
        "shellcheck disable=" => lower_line.contains("shellcheck disable="),
        "fmt: off" => lower_line.contains("fmt: off"),
        other => lower_line.contains(other),
    }
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

pub(in crate::runner) fn stale_suppression_category(
    marker_lower: &str,
) -> StaleSuppressionCategory {
    if marker_lower.contains("ts-")
        || marker_lower.contains("type: ignore")
        || marker_lower.starts_with("#[allow")
        || marker_lower.starts_with("#[expect")
    {
        return StaleSuppressionCategory::TypeIgnore;
    }
    if marker_lower.contains("prettier-ignore")
        || marker_lower.contains("fmt: off")
        || marker_lower.contains("shellcheck disable=")
    {
        return StaleSuppressionCategory::ToolBypass;
    }
    StaleSuppressionCategory::LintDisable
}
