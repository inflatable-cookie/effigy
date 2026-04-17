use super::super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, TextReportSpec,
};
use crate::model::{GodFileScanResult, GodFileSeverity, TextRenderOptions};

pub fn render_god_file_text(
    result: &GodFileScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "God Files",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "thresholds: warn={} high={} critical={}",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!(
                    "scanned-files: {}  skipped-generated: {}  findings: {}",
                    result.scanned_files,
                    result.skipped_generated,
                    result.findings.len()
                ),
            ],
            empty_message: "No oversized code files found.",
            filtered_message: "No high or critical files found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        GodFileSeverity::Warning,
        GodFileSeverity::High,
        GodFileSeverity::Critical,
        |finding| {
            format!(
                "{}  {} code lines ({} total)  {}",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.path
            )
        },
    )
}

pub fn render_god_file_markdown(result: &GodFileScanResult) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "God Files",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Thresholds: warn=`{}` high=`{}` critical=`{}`",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Skipped generated: `{}`", result.skipped_generated),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No oversized code files found.",
            table_header: "| Severity | Code Lines | Total Lines | Path |",
            table_divider: "| --- | ---: | ---: | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | {} | `{}` |",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.path
            )
        },
    )
}
