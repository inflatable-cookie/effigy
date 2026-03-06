use super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, TextReportSpec,
};
use crate::runner::scan::model::{
    format_bytes, format_ratio, CommentRatioScanResult, CommentRatioSeverity,
    DuplicateBlockScanResult, DuplicateBlockSeverity, GeneratedAssetScanResult,
    GeneratedAssetSeverity, GeneratedInSrcScanResult, GeneratedInSrcSeverity, GodFileScanResult,
    GodFileSeverity, TextRenderOptions,
};

pub(in crate::runner) fn render_god_file_text(
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

pub(in crate::runner) fn render_god_file_markdown(result: &GodFileScanResult) -> String {
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

pub(in crate::runner) fn render_duplicate_block_text(
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
        DuplicateBlockSeverity::Warning,
        DuplicateBlockSeverity::High,
        DuplicateBlockSeverity::Critical,
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

pub(in crate::runner) fn render_duplicate_block_markdown(
    result: &DuplicateBlockScanResult,
) -> String {
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

pub(in crate::runner) fn render_comment_ratio_text(
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

pub(in crate::runner) fn render_comment_ratio_markdown(result: &CommentRatioScanResult) -> String {
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
