use super::super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, SeverityLevels, TextReportSpec,
};
use crate::model::{GodFileScanResult, GodFileSeverity, ScanGraphFileContext, TextRenderOptions};

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
        SeverityLevels {
            warning: GodFileSeverity::Warning,
            high: GodFileSeverity::High,
            critical: GodFileSeverity::Critical,
        },
        |finding| {
            let base = format!(
                "{}  {} code lines ({} total)  {}",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.path
            );
            match &finding.graph {
                Some(graph) => format!("{base}\n    graph: {}", format_graph_context(graph)),
                None => base,
            }
        },
    )
}

pub fn render_god_file_markdown(result: &GodFileScanResult) -> String {
    let include_graph = result
        .findings
        .iter()
        .any(|finding| finding.graph.is_some());
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
            table_header: if include_graph {
                "| Severity | Code Lines | Total Lines | Path | Graph |"
            } else {
                "| Severity | Code Lines | Total Lines | Path |"
            },
            table_divider: if include_graph {
                "| --- | ---: | ---: | --- | --- |"
            } else {
                "| --- | ---: | ---: | --- |"
            },
        },
        &result.findings,
        |finding| match &finding.graph {
            Some(graph) => format!(
                "| {} | {} | {} | `{}` | {} |",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.path,
                format_graph_context(graph).replace('|', "/")
            ),
            None => format!(
                "| {} | {} | {} | `{}` |",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.path
            ),
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
