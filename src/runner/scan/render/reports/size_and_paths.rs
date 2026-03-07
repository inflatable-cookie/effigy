use super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, TextReportSpec,
};
use crate::runner::scan::model::{
    format_bytes, GeneratedAssetScanResult, GeneratedAssetSeverity, GeneratedInSrcScanResult,
    GeneratedInSrcSeverity, TextRenderOptions,
};

pub(in crate::runner) fn render_generated_asset_text(
    result: &GeneratedAssetScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Generated Assets",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "thresholds-bytes: warn={} high={} critical={}",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!(
                    "scanned-files: {}  candidate-files: {}  findings: {}",
                    result.scanned_files,
                    result.candidate_files,
                    result.findings.len()
                ),
            ],
            empty_message: "No bulky generated or vendored assets found.",
            filtered_message: "No high or critical generated assets found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        GeneratedAssetSeverity::Warning,
        GeneratedAssetSeverity::High,
        GeneratedAssetSeverity::Critical,
        |finding| {
            format!(
                "{}  {}  {}  [{}]",
                finding.severity.as_str(),
                format_bytes(finding.bytes),
                finding.path,
                finding.reason
            )
        },
    )
}

pub(in crate::runner) fn render_generated_asset_markdown(
    result: &GeneratedAssetScanResult,
) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Generated Assets",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Thresholds (bytes): warn=`{}` high=`{}` critical=`{}`",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Candidate files: `{}`", result.candidate_files),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No bulky generated or vendored assets found.",
            table_header: "| Severity | Size | Path | Reason |",
            table_divider: "| --- | ---: | --- | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | `{}` | `{}` |",
                finding.severity.as_str(),
                format_bytes(finding.bytes),
                finding.path,
                finding.reason
            )
        },
    )
}

pub(in crate::runner) fn render_generated_in_src_text(
    result: &GeneratedInSrcScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Generated In Src",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "thresholds-bytes: warn={} high={} critical={}",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!("source-roots: {}", result.source_roots.join(", ")),
                format!(
                    "scanned-files: {}  candidate-files: {}  findings: {}",
                    result.scanned_files,
                    result.candidate_files,
                    result.findings.len()
                ),
            ],
            empty_message: "No generated files found inside source trees.",
            filtered_message: "No high or critical generated-in-src files found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        GeneratedInSrcSeverity::Warning,
        GeneratedInSrcSeverity::High,
        GeneratedInSrcSeverity::Critical,
        |finding| {
            format!(
                "{}  {}  {}  [{}] [{}]",
                finding.severity.as_str(),
                format_bytes(finding.size_bytes),
                finding.path,
                finding.category.as_str(),
                finding.reason
            )
        },
    )
}

pub(in crate::runner) fn render_generated_in_src_markdown(
    result: &GeneratedInSrcScanResult,
) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Generated In Src",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Thresholds (bytes): warn=`{}` high=`{}` critical=`{}`",
                    result.thresholds.warn, result.thresholds.high, result.thresholds.critical
                ),
                format!(
                    "- Source roots: {}",
                    result
                        .source_roots
                        .iter()
                        .map(|value| format!("`{value}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Candidate files: `{}`", result.candidate_files),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No generated files found inside source trees.",
            table_header: "| Severity | Size | Category | Reason | Path |",
            table_divider: "| --- | ---: | --- | --- | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | {} | `{}` | `{}` |",
                finding.severity.as_str(),
                format_bytes(finding.size_bytes),
                finding.category.as_str(),
                finding.reason,
                finding.path
            )
        },
    )
}
