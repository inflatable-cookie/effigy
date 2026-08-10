use super::common::{
    render_markdown_report, render_text_report, style_snippet_line, MarkdownReportSpec,
    SeverityLevels, TextReportSpec,
};
use crate::model::{
    AttentionMarkerScanResult, AttentionMarkerSeverity, ScanGraphFileContext,
    StaleSuppressionScanResult, StaleSuppressionSeverity, TextRenderOptions,
};

pub fn render_attention_marker_text(
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
        SeverityLevels {
            warning: AttentionMarkerSeverity::Warning,
            high: AttentionMarkerSeverity::High,
            critical: AttentionMarkerSeverity::Critical,
        },
        |finding| {
            let base = format!(
                "{}  {}:{}  {}  [{}]  {}",
                finding.severity.as_str(),
                finding.path,
                finding.line,
                finding.category.as_str(),
                finding.marker,
                finding.snippet
            );
            match &finding.graph {
                Some(graph) => format!("{base}\n    graph: {}", format_graph_context(graph)),
                None => base,
            }
        },
    )
}

pub fn render_attention_marker_markdown(result: &AttentionMarkerScanResult) -> String {
    let include_graph = result
        .findings
        .iter()
        .any(|finding| finding.graph.is_some());
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
            table_header: if include_graph {
                "| Severity | Category | Marker | Path | Line | Snippet | Graph |"
            } else {
                "| Severity | Category | Marker | Path | Line | Snippet |"
            },
            table_divider: if include_graph {
                "| --- | --- | --- | --- | ---: | --- | --- |"
            } else {
                "| --- | --- | --- | --- | ---: | --- |"
            },
        },
        &result.findings,
        |finding| match &finding.graph {
            Some(graph) => format!(
                "| {} | {} | `{}` | `{}` | {} | `{}` | {} |",
                finding.severity.as_str(),
                finding.category.as_str(),
                finding.marker,
                finding.path,
                finding.line,
                finding.snippet.replace('`', "'"),
                format_graph_context(graph).replace('|', "/")
            ),
            None => format!(
                "| {} | {} | `{}` | `{}` | {} | `{}` |",
                finding.severity.as_str(),
                finding.category.as_str(),
                finding.marker,
                finding.path,
                finding.line,
                finding.snippet.replace('`', "'"),
            ),
        },
    )
}

pub fn render_stale_suppression_text(
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
        SeverityLevels {
            warning: StaleSuppressionSeverity::Warning,
            high: StaleSuppressionSeverity::High,
            critical: StaleSuppressionSeverity::Critical,
        },
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

fn format_graph_context(graph: &ScanGraphFileContext) -> String {
    format!(
        "{}  symbols={}  in={}  out={}  refs={}  {}",
        graph.language_id,
        graph.symbol_count,
        graph.inbound_edges,
        graph.outbound_edges,
        graph.reference_count,
        graph.connectivity
    )
}

pub fn render_stale_suppression_markdown(result: &StaleSuppressionScanResult) -> String {
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
