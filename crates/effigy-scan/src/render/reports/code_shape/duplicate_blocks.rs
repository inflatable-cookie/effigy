use super::super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, SeverityLevels, TextReportSpec,
};
use crate::model::{DuplicateBlockScanResult, DuplicateBlockSeverity, TextRenderOptions};

pub fn render_duplicate_block_text(
    result: &DuplicateBlockScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Duplicate Blocks",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "thresholds: warn={} high={} critical={} min-occurrences={}",
                    result.thresholds.warn,
                    result.thresholds.high,
                    result.thresholds.critical,
                    result.thresholds.min_occurrences
                ),
                format!(
                    "scanned-files: {}  candidate-blocks: {}  findings: {}",
                    result.scanned_files,
                    result.candidate_blocks,
                    result.findings.len()
                ),
            ],
            empty_message: "No duplicate code blocks found.",
            filtered_message: "No high or critical duplicate blocks found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        SeverityLevels {
            warning: DuplicateBlockSeverity::Warning,
            high: DuplicateBlockSeverity::High,
            critical: DuplicateBlockSeverity::Critical,
        },
        |finding| {
            let locations = finding
                .locations
                .iter()
                .take(3)
                .map(|location| {
                    format!(
                        "{}:{}-{}",
                        location.path, location.start_line, location.end_line
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}  {} lines  {} occurrences  {}  [{}]",
                finding.severity.as_str(),
                finding.block_lines,
                finding.occurrences,
                finding.snippet,
                locations
            )
        },
    )
}

pub fn render_duplicate_block_markdown(result: &DuplicateBlockScanResult) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Duplicate Blocks",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Thresholds: warn=`{}` high=`{}` critical=`{}` min-occurrences=`{}`",
                    result.thresholds.warn,
                    result.thresholds.high,
                    result.thresholds.critical,
                    result.thresholds.min_occurrences
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Candidate blocks: `{}`", result.candidate_blocks),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No duplicate code blocks found.",
            table_header: "| Severity | Lines | Occurrences | Fingerprint | Snippet | Locations |",
            table_divider: "| --- | ---: | ---: | --- | --- | --- |",
        },
        &result.findings,
        |finding| {
            let locations = finding
                .locations
                .iter()
                .map(|location| {
                    format!(
                        "{}:{}-{}",
                        location.path, location.start_line, location.end_line
                    )
                })
                .collect::<Vec<_>>()
                .join("<br>");
            format!(
                "| {} | {} | {} | `{}` | `{}` | {} |",
                finding.severity.as_str(),
                finding.block_lines,
                finding.occurrences,
                finding.fingerprint,
                finding.snippet.replace('`', "'"),
                locations
            )
        },
    )
}
