use super::model::{
    format_bytes, format_ratio, AttentionMarkerScanResult, AttentionMarkerSeverity,
    CommentRatioScanResult, CommentRatioSeverity, DuplicateBlockScanResult, DuplicateBlockSeverity,
    GeneratedAssetScanResult, GeneratedAssetSeverity, GeneratedInSrcScanResult,
    GeneratedInSrcSeverity, GodFileScanResult, GodFileSeverity, StaleSuppressionScanResult,
    StaleSuppressionSeverity, TextRenderOptions,
};
use anstyle::{Ansi256Color, Color, Style};

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

pub(in crate::runner) fn render_attention_marker_text(
    result: &AttentionMarkerScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Attention Markers",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "markers: warning={} high={} critical={}",
                    result.patterns.warning.len(),
                    result.patterns.high.len(),
                    result.patterns.critical.len()
                ),
                format!(
                    "scanned-files: {}  matched-lines: {}  findings: {}",
                    result.scanned_files,
                    result.matched_lines,
                    result.findings.len()
                ),
            ],
            empty_message: "No attention markers found.",
            filtered_message: "No high or critical attention markers found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        AttentionMarkerSeverity::Warning,
        AttentionMarkerSeverity::High,
        AttentionMarkerSeverity::Critical,
        |finding| {
            format!(
                "{}  {}:{}  {}  [{}]  {}",
                finding.severity.as_str(),
                finding.path,
                finding.line,
                finding.category.as_str(),
                finding.marker,
                finding.snippet
            )
        },
    )
}

pub(in crate::runner) fn render_attention_marker_markdown(
    result: &AttentionMarkerScanResult,
) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Attention Markers",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Markers: warning=`{}` high=`{}` critical=`{}`",
                    result.patterns.warning.len(),
                    result.patterns.high.len(),
                    result.patterns.critical.len()
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Matched lines: `{}`", result.matched_lines),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No attention markers found.",
            table_header: "| Severity | Category | Marker | Path | Line | Snippet |",
            table_divider: "| --- | --- | --- | --- | ---: | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | `{}` | `{}` | {} | `{}` |",
                finding.severity.as_str(),
                finding.category.as_str(),
                finding.marker,
                finding.path,
                finding.line,
                finding.snippet.replace('`', "'"),
            )
        },
    )
}

pub(in crate::runner) fn render_stale_suppression_text(
    result: &StaleSuppressionScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Stale Suppressions",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!(
                    "markers: warning={} high={} critical={}",
                    result.patterns.warning.len(),
                    result.patterns.high.len(),
                    result.patterns.critical.len()
                ),
                format!(
                    "scanned-files: {}  matched-lines: {}  findings: {}",
                    result.scanned_files,
                    result.matched_lines,
                    result.findings.len()
                ),
            ],
            empty_message: "No stale suppressions found.",
            filtered_message: "No high or critical stale suppressions found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity,
        StaleSuppressionSeverity::Warning,
        StaleSuppressionSeverity::High,
        StaleSuppressionSeverity::Critical,
        |finding| {
            let header = format!(
                "{}  {}:{}  {}  [{}]",
                finding.severity.as_str(),
                finding.path,
                finding.line,
                finding.category.as_str(),
                finding.marker
            );
            let snippet = style_snippet_line(
                render_options.color_enabled,
                &format!("    {}", finding.snippet),
            );
            format!("{header}\n{snippet}")
        },
    )
}

pub(in crate::runner) fn render_stale_suppression_markdown(
    result: &StaleSuppressionScanResult,
) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Stale Suppressions",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!(
                    "- Markers: warning=`{}` high=`{}` critical=`{}`",
                    result.patterns.warning.len(),
                    result.patterns.high.len(),
                    result.patterns.critical.len()
                ),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Matched lines: `{}`", result.matched_lines),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No stale suppressions found.",
            table_header: "| Severity | Category | Marker | Path | Line | Snippet |",
            table_divider: "| --- | --- | --- | --- | ---: | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | `{}` | `{}` | {} | `{}` |",
                finding.severity.as_str(),
                finding.category.as_str(),
                finding.marker,
                finding.path,
                finding.line,
                finding.snippet.replace('`', "'"),
            )
        },
    )
}

fn render_text_report<T, S, FSeverity, FLine>(
    spec: TextReportSpec,
    findings: &[T],
    render_options: TextRenderOptions,
    severity_of: FSeverity,
    warning: S,
    high: S,
    critical: S,
    line_for: FLine,
) -> String
where
    S: Copy + PartialEq,
    FSeverity: Fn(&T) -> S,
    FLine: Fn(&T) -> String,
{
    let counts = severity_counts(findings, &severity_of, warning, high, critical);
    let visible_findings = visible_findings(
        findings,
        render_options.show_warnings,
        &severity_of,
        warning,
    );
    let mut lines = vec![spec.title.to_owned(), String::new()];
    lines.extend(spec.metadata_lines);
    lines.push(severity_counts_line(counts));
    push_hidden_warning_line(&mut lines, render_options, counts.warning);
    if findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.empty_message.to_owned());
        return lines.join("\n");
    }
    if visible_findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.filtered_message.to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("Findings".to_owned());
    lines.extend(visible_findings.into_iter().map(line_for));
    lines.join("\n")
}

fn render_markdown_report<T, FLine>(
    spec: MarkdownReportSpec,
    findings: &[T],
    line_for: FLine,
) -> String
where
    FLine: Fn(&T) -> String,
{
    let mut lines = vec![format!("# {}", spec.title), String::new()];
    lines.extend(spec.metadata_lines);
    if findings.is_empty() {
        lines.push(String::new());
        lines.push(spec.empty_message.to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push(spec.table_header.to_owned());
    lines.push(spec.table_divider.to_owned());
    lines.extend(findings.iter().map(line_for));
    lines.join("\n")
}

fn severity_counts<T, S, F>(
    findings: &[T],
    severity_of: &F,
    warning: S,
    high: S,
    critical: S,
) -> SeverityCounts
where
    S: Copy + PartialEq,
    F: Fn(&T) -> S,
{
    let mut counts = SeverityCounts::default();
    for finding in findings {
        let severity = severity_of(finding);
        if severity == critical {
            counts.critical += 1;
        } else if severity == high {
            counts.high += 1;
        } else if severity == warning {
            counts.warning += 1;
        }
    }
    counts
}

fn visible_findings<'a, T, S, F>(
    findings: &'a [T],
    show_warnings: bool,
    severity_of: &F,
    warning: S,
) -> Vec<&'a T>
where
    S: Copy + PartialEq,
    F: Fn(&T) -> S,
{
    findings
        .iter()
        .filter(|finding| show_warnings || severity_of(finding) != warning)
        .collect()
}

fn push_hidden_warning_line(
    lines: &mut Vec<String>,
    render_options: TextRenderOptions,
    warning_count: usize,
) {
    if !render_options.show_warnings && warning_count > 0 {
        lines.push(format!(
            "warning-rows-hidden: {}  use --show-warnings to list them",
            warning_count
        ));
    }
}

fn severity_counts_line(counts: SeverityCounts) -> String {
    format!(
        "severity-counts: critical={} high={} warning={}",
        counts.critical, counts.high, counts.warning
    )
}

struct TextReportSpec {
    title: &'static str,
    metadata_lines: Vec<String>,
    empty_message: &'static str,
    filtered_message: &'static str,
}

struct MarkdownReportSpec {
    title: &'static str,
    metadata_lines: Vec<String>,
    empty_message: &'static str,
    table_header: &'static str,
    table_divider: &'static str,
}

#[derive(Default, Clone, Copy)]
struct SeverityCounts {
    critical: usize,
    high: usize,
    warning: usize,
}

fn style_snippet_line(color_enabled: bool, text: &str) -> String {
    if !color_enabled {
        return text.to_owned();
    }
    let muted = Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(244))));
    format!("{}{}{}", muted.render(), text, muted.render_reset())
}
