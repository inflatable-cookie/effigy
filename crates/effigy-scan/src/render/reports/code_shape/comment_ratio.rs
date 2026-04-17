use super::super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, TextReportSpec,
};
use crate::model::{format_ratio, CommentRatioScanResult, CommentRatioSeverity, TextRenderOptions};

pub fn render_comment_ratio_text(
    result: &CommentRatioScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Comment Ratio",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "thresholds: warn={} high={} critical={} min-code-lines={}",
                    format_ratio(result.thresholds.warn),
                    format_ratio(result.thresholds.high),
                    format_ratio(result.thresholds.critical),
                    result.thresholds.min_code_lines
                ),
                format!(
                    "scanned-files: {}  candidate-files: {}  findings: {}",
                    result.scanned_files,
                    result.candidate_files,
                    result.findings.len()
                ),
            ],
            empty_message: "No comment-heavy code files found.",
            filtered_message: "No high or critical comment-heavy files found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        CommentRatioSeverity::Warning,
        CommentRatioSeverity::High,
        CommentRatioSeverity::Critical,
        |finding| {
            format!(
                "{}  ratio={}  {} comment / {} code  {}",
                finding.severity.as_str(),
                format_ratio(finding.ratio),
                finding.comment_lines,
                finding.code_lines,
                finding.path
            )
        },
    )
}

pub fn render_comment_ratio_markdown(result: &CommentRatioScanResult) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Comment Ratio",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Thresholds: warn=`{}` high=`{}` critical=`{}` min-code-lines=`{}`",
                    format_ratio(result.thresholds.warn),
                    format_ratio(result.thresholds.high),
                    format_ratio(result.thresholds.critical),
                    result.thresholds.min_code_lines
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Candidate files: `{}`", result.candidate_files),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No comment-heavy code files found.",
            table_header: "| Severity | Ratio | Comment Lines | Code Lines | Path |",
            table_divider: "| --- | ---: | ---: | ---: | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | {} | {} | `{}` |",
                finding.severity.as_str(),
                format_ratio(finding.ratio),
                finding.comment_lines,
                finding.code_lines,
                finding.path
            )
        },
    )
}
