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
