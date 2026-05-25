use super::common::{
    render_markdown_report, render_text_report, MarkdownReportSpec, TextReportSpec,
};
use crate::model::{
    BoundaryViolationScanResult, BoundaryViolationSeverity, DeadCodeConfidence, DeadCodeFinding,
    DeadCodeFindingKind, DeadCodeScanResult, DeadCodeSeverity, TextRenderOptions,
    ValidationGapConfidence, ValidationGapFindingKind, ValidationGapScanResult,
    ValidationGapSeverity, ValidationGapTestTarget,
};

pub fn render_boundary_violation_text(
    result: &BoundaryViolationScanResult,
    render_options: TextRenderOptions,
) -> String {
    render_text_report(
        TextReportSpec {
            title: "Boundary Violations",
            metadata_lines: vec![
                format!("root: {}", result.root),
                format!("configured-layers: {}", result.configured_layers),
                format!(
                    "checked-edges: {}  findings: {}",
                    result.checked_edges,
                    result.findings.len()
                ),
            ],
            empty_message: if result.configured_layers == 0 {
                "No boundary rules configured."
            } else {
                "No boundary violations found."
            },
            filtered_message: "No high or critical boundary violations found.",
        },
        &result.findings,
        render_options,
        |finding| finding.severity.clone(),
        BoundaryViolationSeverity::Warning,
        BoundaryViolationSeverity::High,
        BoundaryViolationSeverity::Critical,
        |finding| {
            format!(
                "{}  {} -> {}  {}  {}:{} -> {}:{}  {} -> {}  [{}]",
                finding.severity.as_str(),
                finding.source_layer,
                finding.target_layer,
                finding.edge_kind,
                finding.source_path,
                finding.source_line,
                finding.target_path,
                finding.target_line,
                finding.source_symbol,
                finding.target_symbol,
                finding.confidence
            )
        },
    )
}

pub fn render_boundary_violation_markdown(result: &BoundaryViolationScanResult) -> String {
    render_markdown_report(
        MarkdownReportSpec {
            title: "Boundary Violations",
            metadata_lines: vec![
                format!("- Root: `{}`", result.root),
                format!("- Configured layers: `{}`", result.configured_layers),
                format!("- Checked edges: `{}`", result.checked_edges),
                format!("- Findings: `{}`", result.findings.len()),
            ],
            empty_message: if result.configured_layers == 0 {
                "No boundary rules configured."
            } else {
                "No boundary violations found."
            },
            table_header:
                "| Severity | Source Layer | Target Layer | Edge | Source | Target | Confidence |",
            table_divider: "| --- | --- | --- | --- | --- | --- | --- |",
        },
        &result.findings,
        |finding| {
            format!(
                "| {} | {} | {} | `{}` | `{}`:{} `{}` | `{}`:{} `{}` | {} |",
                finding.severity.as_str(),
                finding.source_layer,
                finding.target_layer,
                finding.edge_kind,
                finding.source_path,
                finding.source_line,
                finding.source_symbol.replace('`', "'"),
                finding.target_path,
                finding.target_line,
                finding.target_symbol.replace('`', "'"),
                finding.confidence
            )
        },
    )
}

pub fn render_dead_code_text(
    result: &DeadCodeScanResult,
    _render_options: TextRenderOptions,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Dead Code\nroot: {}\nchecked-files: {}  checked-symbols: {}  findings: {}\n",
        result.root,
        result.checked_files,
        result.checked_symbols,
        result.findings.len()
    ));
    out.push_str(&format!(
        "skipped: {} non-implementation, {} unsupported, {} allowlisted path(s), {} allowlisted symbol(s)\n",
        result.skipped_non_implementation_files,
        result.skipped_unsupported_language_files,
        result.skipped_allowlisted_paths,
        result.skipped_allowlisted_symbols
    ));
    if result.findings.is_empty() {
        out.push_str("No dead-code candidates found.\n");
        return out;
    }
    for finding in &result.findings {
        out.push_str(&format!(
            "\n{}  {} [{} / {}]  {}:{}\n",
            dead_code_severity_label(finding.severity),
            dead_code_kind_label(finding.kind),
            dead_code_confidence_label(finding.confidence),
            finding.language_id,
            finding.path,
            finding.line
        ));
        if let Some(symbol) = &finding.symbol {
            if let Some(kind) = &finding.symbol_kind {
                out.push_str(&format!("symbol: {} ({})\n", symbol, kind));
            } else {
                out.push_str(&format!("symbol: {}\n", symbol));
            }
        }
        out.push_str(&format!("reason: {}\n", finding.reason));
        out.push_str(&format!(
            "graph: in {} edge(s), out {} edge(s), in {} reference(s), out {} reference(s)\n",
            finding.inbound_edges,
            finding.outbound_edges,
            finding.inbound_references,
            finding.outbound_references
        ));
    }
    out
}

pub fn render_dead_code_markdown(result: &DeadCodeScanResult) -> String {
    if result.findings.is_empty() {
        return [
            "# Dead Code".to_owned(),
            format!("- Root: `{}`", result.root),
            format!("- Checked files: `{}`", result.checked_files),
            format!("- Checked symbols: `{}`", result.checked_symbols),
            "- Findings: `0`".to_owned(),
            String::new(),
            "No dead-code candidates found.".to_owned(),
            String::new(),
        ]
        .join("\n");
    }

    let mut out = vec![
        "# Dead Code".to_owned(),
        format!("- Root: `{}`", result.root),
        format!("- Checked files: `{}`", result.checked_files),
        format!("- Checked symbols: `{}`", result.checked_symbols),
        format!(
            "- Skipped: `{}` non-implementation, `{}` unsupported, `{}` allowlisted paths, `{}` allowlisted symbols",
            result.skipped_non_implementation_files,
            result.skipped_unsupported_language_files,
            result.skipped_allowlisted_paths,
            result.skipped_allowlisted_symbols
        ),
        format!("- Findings: `{}`", result.findings.len()),
        String::new(),
        "| Severity | Kind | Confidence | Path | Symbol | Reason |".to_owned(),
        "| --- | --- | --- | --- | --- | --- |".to_owned(),
    ];
    for finding in &result.findings {
        out.push(format!(
            "| {} | {} | {} | `{}`:{} | {} | {} |",
            dead_code_severity_label(finding.severity),
            dead_code_kind_label(finding.kind),
            dead_code_confidence_label(finding.confidence),
            finding.path,
            finding.line,
            markdown_symbol_cell(finding),
            finding.reason.replace('|', "\\|")
        ));
    }
    out.push(String::new());
    out.join("\n")
}

fn markdown_symbol_cell(finding: &DeadCodeFinding) -> String {
    match (&finding.symbol, &finding.symbol_kind) {
        (Some(symbol), Some(kind)) => format!("`{}` ({})", symbol.replace('`', "'"), kind),
        (Some(symbol), None) => format!("`{}`", symbol.replace('`', "'")),
        (None, _) => "-".to_owned(),
    }
}

fn dead_code_kind_label(kind: DeadCodeFindingKind) -> &'static str {
    match kind {
        DeadCodeFindingKind::IsolatedFile => "isolated-file",
        DeadCodeFindingKind::UnreferencedSymbol => "unreferenced-symbol",
    }
}

fn dead_code_severity_label(severity: DeadCodeSeverity) -> &'static str {
    match severity {
        DeadCodeSeverity::Warning => "warning",
        DeadCodeSeverity::High => "high",
    }
}

fn dead_code_confidence_label(confidence: DeadCodeConfidence) -> &'static str {
    match confidence {
        DeadCodeConfidence::Medium => "medium",
        DeadCodeConfidence::High => "high",
    }
}

pub fn render_validation_gap_text(
    result: &ValidationGapScanResult,
    _render_options: TextRenderOptions,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Validation Gaps\nroot: {}\nmode: {}  hotspot-threshold: {}  affected-depth: {}\n",
        result.root, result.mode, result.hotspot_threshold, result.affected_depth
    ));
    out.push_str(&format!(
        "checked-files: {}  findings: {}\n",
        result.checked_files,
        result.findings.len()
    ));
    out.push_str(&format!(
        "skipped: {} non-implementation, {} unsupported, {} allowlisted path(s)\n",
        result.skipped_non_implementation_files,
        result.skipped_unsupported_language_files,
        result.skipped_allowlisted_paths
    ));
    if !result.changed_paths.is_empty() {
        out.push_str(&format!(
            "changed-paths: {}\n",
            result.changed_paths.join(", ")
        ));
    }
    if !result.likely_test_files.is_empty() || !result.likely_test_tasks.is_empty() {
        out.push_str("likely-tests:\n");
        for target in &result.likely_test_files {
            out.push_str(&format!(
                "- file {} [{}] {} because {}\n",
                target.path,
                target.kind,
                target.confidence,
                target.reasons.join("; ")
            ));
        }
        for target in &result.likely_test_tasks {
            out.push_str(&format!(
                "- task {} [{}] {} because {}\n",
                target.name,
                target.kind,
                target.confidence,
                target.reasons.join("; ")
            ));
        }
    }
    if result.findings.is_empty() {
        out.push_str("No validation-gap findings found.\n");
        return out;
    }
    for finding in &result.findings {
        out.push_str(&format!(
            "\n{}  {} [{} / {}]  {}:{}\n",
            validation_gap_severity_label(finding.severity),
            validation_gap_kind_label(finding.kind),
            validation_gap_confidence_label(finding.confidence),
            finding.language_id,
            finding.path,
            finding.line
        ));
        out.push_str(&format!("reason: {}\n", finding.reason));
        out.push_str(&format!(
            "graph: connectivity {}, in {} edge(s), out {} edge(s), in {} reference(s), out {} reference(s)\n",
            finding.connectivity,
            finding.inbound_edges,
            finding.outbound_edges,
            finding.inbound_references,
            finding.outbound_references
        ));
    }
    out
}

pub fn render_validation_gap_markdown(result: &ValidationGapScanResult) -> String {
    let mut out = vec![
        "# Validation Gaps".to_owned(),
        format!("- Root: `{}`", result.root),
        format!("- Mode: `{}`", result.mode),
        format!("- Hotspot threshold: `{}`", result.hotspot_threshold),
        format!("- Affected depth: `{}`", result.affected_depth),
        format!("- Checked files: `{}`", result.checked_files),
        format!(
            "- Skipped: `{}` non-implementation, `{}` unsupported, `{}` allowlisted paths",
            result.skipped_non_implementation_files,
            result.skipped_unsupported_language_files,
            result.skipped_allowlisted_paths
        ),
        format!("- Findings: `{}`", result.findings.len()),
    ];
    if !result.changed_paths.is_empty() {
        out.push(format!(
            "- Changed paths: `{}`",
            result.changed_paths.join("`, `")
        ));
    }
    out.push(String::new());

    if !result.likely_test_files.is_empty() || !result.likely_test_tasks.is_empty() {
        out.push("## Likely Tests".to_owned());
        out.push(String::new());
        out.push("| Kind | Target | Confidence | Reasons |".to_owned());
        out.push("| --- | --- | --- | --- |".to_owned());
        for target in &result.likely_test_files {
            out.push(render_validation_gap_target_row("file", target));
        }
        for target in &result.likely_test_tasks {
            out.push(render_validation_gap_target_row("task", target));
        }
        out.push(String::new());
    }

    if result.findings.is_empty() {
        out.push("No validation-gap findings found.".to_owned());
        out.push(String::new());
        return out.join("\n");
    }

    out.push("| Severity | Kind | Confidence | Path | Connectivity | Reason |".to_owned());
    out.push("| --- | --- | --- | --- | --- | --- |".to_owned());
    for finding in &result.findings {
        out.push(format!(
            "| {} | {} | {} | `{}`:{} | {} | {} |",
            validation_gap_severity_label(finding.severity),
            validation_gap_kind_label(finding.kind),
            validation_gap_confidence_label(finding.confidence),
            finding.path,
            finding.line,
            finding.connectivity,
            finding.reason.replace('|', "\\|")
        ));
    }
    out.push(String::new());
    out.join("\n")
}

fn render_validation_gap_target_row(kind: &str, target: &ValidationGapTestTarget) -> String {
    let target_name = if kind == "file" {
        format!("`{}`", target.path.replace('`', "'"))
    } else {
        format!(
            "`{}` in `{}`",
            target.name.replace('`', "'"),
            target.path.replace('`', "'")
        )
    };
    format!(
        "| {} | {} | {} | {} |",
        kind,
        target_name,
        target.confidence,
        target.reasons.join("; ").replace('|', "\\|")
    )
}

fn validation_gap_kind_label(kind: ValidationGapFindingKind) -> &'static str {
    match kind {
        ValidationGapFindingKind::HotspotWithoutNearbyTests => "hotspot-without-nearby-tests",
        ValidationGapFindingKind::ChangedOwnerWithoutTestTarget => {
            "changed-owner-without-test-target"
        }
    }
}

fn validation_gap_severity_label(severity: ValidationGapSeverity) -> &'static str {
    match severity {
        ValidationGapSeverity::Warning => "warning",
        ValidationGapSeverity::High => "high",
    }
}

fn validation_gap_confidence_label(confidence: ValidationGapConfidence) -> &'static str {
    match confidence {
        ValidationGapConfidence::Medium => "medium",
        ValidationGapConfidence::High => "high",
    }
}
