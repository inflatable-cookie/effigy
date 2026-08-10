use super::WorkspaceScan;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::model::{
    AttentionMarkerFinding, AttentionMarkerScanOptions, AttentionMarkerScanResult,
    StaleSuppressionFinding, StaleSuppressionScanOptions, StaleSuppressionScanResult,
};
use super::super::support::{
    attention_marker_category, attention_marker_matches_line, attention_marker_severity_rank,
    compile_attention_marker_patterns, compile_stale_suppression_patterns, is_generated_artifact,
    rebase_finding_path, should_skip_path, stale_suppression_category,
    stale_suppression_matches_line, stale_suppression_severity_rank, trim_snippet,
};
use super::{run_workspace_scan, walk_text_scan_files, ScanError, ScanWorkspaceCounts};

pub fn run_attention_marker_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &AttentionMarkerScanOptions,
) -> Result<AttentionMarkerScanResult, ScanError> {
    run_workspace_scan(
        WorkspaceScan {
            target_root,
            scan_roots,
            stats: ScanWorkspaceCounts::default(),
        },
        |root, skipped_roots| run_attention_marker_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.matched_lines;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_attention_marker_findings,
        |root, counts, findings| AttentionMarkerScanResult {
            root,
            scanned_files: counts.scanned_files,
            matched_lines: counts.secondary,
            findings,
            patterns: options.patterns.clone(),
        },
    )
}

pub fn run_stale_suppression_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &StaleSuppressionScanOptions,
) -> Result<StaleSuppressionScanResult, ScanError> {
    run_workspace_scan(
        WorkspaceScan {
            target_root,
            scan_roots,
            stats: ScanWorkspaceCounts::default(),
        },
        |root, skipped_roots| run_stale_suppression_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.matched_lines;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_stale_suppression_findings,
        |root, counts, findings| StaleSuppressionScanResult {
            root,
            scanned_files: counts.scanned_files,
            matched_lines: counts.secondary,
            findings,
            patterns: options.patterns.clone(),
        },
    )
}

fn run_attention_marker_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &AttentionMarkerScanOptions,
) -> Result<AttentionMarkerScanResult, ScanError> {
    options.validate()?;
    let patterns = compile_attention_marker_patterns(&options.patterns);
    let mut findings = Vec::<AttentionMarkerFinding>::new();
    let mut scanned_files = 0usize;
    let mut matched_lines = 0usize;

    walk_text_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |_path, rel, rel_str, contents| {
            if is_generated_artifact(rel, &contents) {
                return Ok(());
            }

            scanned_files += 1;
            for (line_number, raw_line) in contents.lines().enumerate() {
                let line = raw_line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut line_has_match = false;
                let mut matched_keys = BTreeSet::<String>::new();
                for (severity, marker, marker_lower) in &patterns {
                    if !attention_marker_matches_line(raw_line, marker_lower) {
                        continue;
                    }
                    let dedupe_key = marker_lower.trim_start_matches('@').to_owned();
                    if !matched_keys.insert(dedupe_key) {
                        continue;
                    }
                    line_has_match = true;
                    findings.push(AttentionMarkerFinding {
                        path: rel_str.to_owned(),
                        line: line_number + 1,
                        category: attention_marker_category(marker_lower),
                        severity: *severity,
                        marker: marker.clone(),
                        snippet: trim_snippet(line, 120),
                        graph: None,
                    });
                }
                if line_has_match {
                    matched_lines += 1;
                }
            }
            Ok(())
        },
    )?;

    sort_attention_marker_findings(&mut findings);

    Ok(AttentionMarkerScanResult {
        root: root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}

fn run_stale_suppression_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &StaleSuppressionScanOptions,
) -> Result<StaleSuppressionScanResult, ScanError> {
    options.validate()?;
    let patterns = compile_stale_suppression_patterns(&options.patterns);
    let mut findings = Vec::<StaleSuppressionFinding>::new();
    let mut scanned_files = 0usize;
    let mut matched_lines = 0usize;

    walk_text_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |_path, rel, rel_str, contents| {
            if is_generated_artifact(rel, &contents) {
                return Ok(());
            }

            scanned_files += 1;
            for (line_number, raw_line) in contents.lines().enumerate() {
                let line = raw_line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut line_has_match = false;
                let mut matched_keys = BTreeSet::<String>::new();
                for (severity, marker, marker_lower) in &patterns {
                    if !stale_suppression_matches_line(raw_line, marker_lower) {
                        continue;
                    }
                    let dedupe_key = stale_suppression_dedupe_key(marker_lower);
                    if !matched_keys.insert(dedupe_key) {
                        continue;
                    }
                    line_has_match = true;
                    findings.push(StaleSuppressionFinding {
                        path: rel_str.to_owned(),
                        line: line_number + 1,
                        category: stale_suppression_category(marker_lower),
                        severity: *severity,
                        marker: marker.clone(),
                        snippet: trim_snippet(line, 120),
                    });
                }
                if line_has_match {
                    matched_lines += 1;
                }
            }
            Ok(())
        },
    )?;

    sort_stale_suppression_findings(&mut findings);

    Ok(StaleSuppressionScanResult {
        root: root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}

fn sort_attention_marker_findings(findings: &mut [AttentionMarkerFinding]) {
    findings.sort_by(|left, right| {
        attention_marker_severity_rank(right.severity)
            .cmp(&attention_marker_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });
}

fn sort_stale_suppression_findings(findings: &mut [StaleSuppressionFinding]) {
    findings.sort_by(|left, right| {
        stale_suppression_severity_rank(right.severity)
            .cmp(&stale_suppression_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });
}

fn stale_suppression_dedupe_key(marker_lower: &str) -> String {
    if marker_lower.starts_with("#[allow(") {
        return "#[allow(".to_owned();
    }
    marker_lower.to_owned()
}
