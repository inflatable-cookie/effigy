use super::{
    format_bytes, AttentionMarkerFinding, AttentionMarkerScanResult, AttentionMarkerSeverity,
    GeneratedAssetFinding, GeneratedAssetScanResult, GeneratedAssetSeverity, GodFileFinding,
    GodFileScanResult, GodFileSeverity, TextRenderOptions,
};

pub(in crate::runner) fn render_god_file_text(
    result: &GodFileScanResult,
    render_options: TextRenderOptions,
) -> String {
    let critical_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GodFileSeverity::Critical)
        .count();
    let high_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GodFileSeverity::High)
        .count();
    let warning_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GodFileSeverity::Warning)
        .count();
    let visible_findings = result
        .findings
        .iter()
        .filter(|finding| {
            render_options.show_warnings || finding.severity != GodFileSeverity::Warning
        })
        .collect::<Vec<&GodFileFinding>>();
    let mut lines = vec![
        "God Files".to_owned(),
        String::new(),
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
        format!(
            "severity-counts: critical={} high={} warning={}",
            critical_count, high_count, warning_count
        ),
    ];
    if !render_options.show_warnings && warning_count > 0 {
        lines.push(format!(
            "warning-rows-hidden: {}  use --show-warnings to list them",
            warning_count
        ));
    }
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No oversized code files found.".to_owned());
        return lines.join("\n");
    }
    if visible_findings.is_empty() {
        lines.push(String::new());
        lines.push("No high or critical files found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("Findings".to_owned());
    for finding in visible_findings {
        lines.push(format!(
            "{}  {} code lines ({} total)  {}",
            finding.severity.as_str(),
            finding.code_lines,
            finding.total_lines,
            finding.path
        ));
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_god_file_markdown(result: &GodFileScanResult) -> String {
    let mut lines = vec![
        "# God Files".to_owned(),
        String::new(),
        format!("- Root: `{}`", result.root),
        format!(
            "- Thresholds: warn=`{}` high=`{}` critical=`{}`",
            result.thresholds.warn, result.thresholds.high, result.thresholds.critical
        ),
        format!("- Scanned files: `{}`", result.scanned_files),
        format!("- Skipped generated: `{}`", result.skipped_generated),
        format!("- Findings: `{}`", result.findings.len()),
    ];
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No oversized code files found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("| Severity | Code Lines | Total Lines | Path |".to_owned());
    lines.push("| --- | ---: | ---: | --- |".to_owned());
    for finding in &result.findings {
        lines.push(format!(
            "| {} | {} | {} | `{}` |",
            finding.severity.as_str(),
            finding.code_lines,
            finding.total_lines,
            finding.path
        ));
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_generated_asset_text(
    result: &GeneratedAssetScanResult,
    render_options: TextRenderOptions,
) -> String {
    let critical_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GeneratedAssetSeverity::Critical)
        .count();
    let high_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GeneratedAssetSeverity::High)
        .count();
    let warning_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == GeneratedAssetSeverity::Warning)
        .count();
    let visible_findings = result
        .findings
        .iter()
        .filter(|finding| {
            render_options.show_warnings || finding.severity != GeneratedAssetSeverity::Warning
        })
        .collect::<Vec<&GeneratedAssetFinding>>();
    let mut lines = vec![
        "Generated Assets".to_owned(),
        String::new(),
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
        format!(
            "severity-counts: critical={} high={} warning={}",
            critical_count, high_count, warning_count
        ),
    ];
    if !render_options.show_warnings && warning_count > 0 {
        lines.push(format!(
            "warning-rows-hidden: {}  use --show-warnings to list them",
            warning_count
        ));
    }
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No bulky generated or vendored assets found.".to_owned());
        return lines.join("\n");
    }
    if visible_findings.is_empty() {
        lines.push(String::new());
        lines.push("No high or critical generated assets found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("Findings".to_owned());
    for finding in visible_findings {
        lines.push(format!(
            "{}  {}  {}  [{}]",
            finding.severity.as_str(),
            format_bytes(finding.bytes),
            finding.path,
            finding.reason
        ));
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_generated_asset_markdown(
    result: &GeneratedAssetScanResult,
) -> String {
    let mut lines = vec![
        "# Generated Assets".to_owned(),
        String::new(),
        format!("- Root: `{}`", result.root),
        format!(
            "- Thresholds (bytes): warn=`{}` high=`{}` critical=`{}`",
            result.thresholds.warn, result.thresholds.high, result.thresholds.critical
        ),
        format!("- Scanned files: `{}`", result.scanned_files),
        format!("- Candidate files: `{}`", result.candidate_files),
        format!("- Findings: `{}`", result.findings.len()),
    ];
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No bulky generated or vendored assets found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("| Severity | Size | Path | Reason |".to_owned());
    lines.push("| --- | ---: | --- | --- |".to_owned());
    for finding in &result.findings {
        lines.push(format!(
            "| {} | {} | `{}` | `{}` |",
            finding.severity.as_str(),
            format_bytes(finding.bytes),
            finding.path,
            finding.reason
        ));
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_attention_marker_text(
    result: &AttentionMarkerScanResult,
    render_options: TextRenderOptions,
) -> String {
    let critical_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == AttentionMarkerSeverity::Critical)
        .count();
    let high_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == AttentionMarkerSeverity::High)
        .count();
    let warning_count = result
        .findings
        .iter()
        .filter(|finding| finding.severity == AttentionMarkerSeverity::Warning)
        .count();
    let visible_findings = result
        .findings
        .iter()
        .filter(|finding| {
            render_options.show_warnings || finding.severity != AttentionMarkerSeverity::Warning
        })
        .collect::<Vec<&AttentionMarkerFinding>>();
    let mut lines = vec![
        "Attention Markers".to_owned(),
        String::new(),
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
        format!(
            "severity-counts: critical={} high={} warning={}",
            critical_count, high_count, warning_count
        ),
    ];
    if !render_options.show_warnings && warning_count > 0 {
        lines.push(format!(
            "warning-rows-hidden: {}  use --show-warnings to list them",
            warning_count
        ));
    }
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No attention markers found.".to_owned());
        return lines.join("\n");
    }
    if visible_findings.is_empty() {
        lines.push(String::new());
        lines.push("No high or critical attention markers found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("Findings".to_owned());
    for finding in visible_findings {
        lines.push(format!(
            "{}  {}:{}  {}  [{}]  {}",
            finding.severity.as_str(),
            finding.path,
            finding.line,
            finding.category.as_str(),
            finding.marker,
            finding.snippet
        ));
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_attention_marker_markdown(
    result: &AttentionMarkerScanResult,
) -> String {
    let mut lines = vec![
        "# Attention Markers".to_owned(),
        String::new(),
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
    ];
    if result.findings.is_empty() {
        lines.push(String::new());
        lines.push("No attention markers found.".to_owned());
        return lines.join("\n");
    }
    lines.push(String::new());
    lines.push("| Severity | Category | Marker | Path | Line | Snippet |".to_owned());
    lines.push("| --- | --- | --- | --- | ---: | --- |".to_owned());
    for finding in &result.findings {
        lines.push(format!(
            "| {} | {} | `{}` | `{}` | {} | `{}` |",
            finding.severity.as_str(),
            finding.category.as_str(),
            finding.marker,
            finding.path,
            finding.line,
            finding.snippet.replace('`', "'"),
        ));
    }
    lines.join("\n")
}
