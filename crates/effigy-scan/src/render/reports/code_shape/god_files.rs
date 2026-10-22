use super::super::super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, SeverityLevels, TextReportSpec,
};
use crate::model::{
    GodFileScanResult, GodFileSeverity, GodFileThresholdsSource, TextRenderOptions,
};

pub fn render_god_file_text(
    result: &GodFileScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "God Files",
            metadata_lines: vec![
                format!("root: {}", result.root),
                thresholds_line(result),
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
            format!(
                "{}  {} code lines ({} total) [{}]  {}",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.language,
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
                format!("- Thresholds: {}", thresholds_markdown(result)),
                format!("- Scanned files: `{}`", result.scanned_files),
                format!("- Skipped generated: `{}`", result.skipped_generated),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: "No oversized code files found.",
            table_header: "| Severity | Code Lines | Total Lines | Language | Path |",
            table_divider: "| --- | ---: | ---: | --- | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | {} | {} | `{}` |",
                finding.severity.as_str(),
                finding.code_lines,
                finding.total_lines,
                finding.language,
                finding.path
            )
        },
    )
}

fn thresholds_line(result: &GodFileScanResult) -> String {
    match result.thresholds_source {
        GodFileThresholdsSource::Configured => format!(
            "thresholds: warn={} high={} critical={}",
            result.thresholds.warn, result.thresholds.high, result.thresholds.critical
        ),
        GodFileThresholdsSource::LanguageDefaults => {
            "thresholds: language-defaults (see `effigy scan god-files --help`)".to_owned()
        }
    }
}

fn thresholds_markdown(result: &GodFileScanResult) -> String {
    match result.thresholds_source {
        GodFileThresholdsSource::Configured => format!(
            "warn=`{}` high=`{}` critical=`{}`",
            result.thresholds.warn, result.thresholds.high, result.thresholds.critical
        ),
        GodFileThresholdsSource::LanguageDefaults => {
            "per-language defaults (see `effigy scan god-files --help`)".to_owned()
        }
    }
}
