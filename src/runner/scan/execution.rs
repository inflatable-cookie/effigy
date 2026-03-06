use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::support::{
    attention_marker_category, attention_marker_matches_line, attention_marker_severity_rank,
    classify_generated_asset_severity, classify_severity, compile_attention_marker_patterns,
    count_code_lines, generated_asset_reason, generated_asset_severity_rank, is_generated_artifact,
    read_asset_sample, rebase_finding_path, severity_rank, should_skip_generated_asset_path,
    should_skip_path, trim_snippet, walk_scan_files, workspace_scan_roots,
};
use super::{
    AttentionMarkerFinding, AttentionMarkerScanOptions, AttentionMarkerScanResult,
    GeneratedAssetFinding, GeneratedAssetScanOptions, GeneratedAssetScanResult, GodFileFinding,
    GodFileScanOptions, GodFileScanResult, RunnerError,
};

pub(in crate::runner) fn run_god_file_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, RunnerError> {
    let mut findings = Vec::<GodFileFinding>::new();
    let mut scanned_files = 0usize;
    let mut skipped_generated = 0usize;

    for (root, skipped_roots) in workspace_scan_roots(target_root, scan_roots) {
        let result = run_god_file_scan_single(&root, &skipped_roots, options)?;
        scanned_files += result.scanned_files;
        skipped_generated += result.skipped_generated;
        findings.extend(result.findings.into_iter().map(|mut finding| {
            finding.path = rebase_finding_path(target_root, &root, &finding.path);
            finding
        }));
    }

    findings.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.code_lines.cmp(&left.code_lines))
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(GodFileScanResult {
        root: target_root.display().to_string(),
        scanned_files,
        skipped_generated,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn run_god_file_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, RunnerError> {
    options.validate()?;
    let mut findings = Vec::<GodFileFinding>::new();
    let mut scanned_files = 0usize;
    let mut skipped_generated = 0usize;
    walk_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |path, rel, rel_str| {
            let contents = std::fs::read_to_string(path).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "scan read failed for {}: {error}",
                    path.display()
                ))
            })?;
            if is_generated_artifact(rel, &contents) {
                skipped_generated += 1;
                return Ok(());
            }

            let code_lines = count_code_lines(rel, &contents);
            scanned_files += 1;
            let severity = classify_severity(code_lines, &options.thresholds);
            if let Some(severity) = severity {
                findings.push(GodFileFinding {
                    path: rel_str.to_owned(),
                    code_lines,
                    total_lines: contents.lines().count(),
                    severity,
                });
            }
            Ok(())
        },
    )?;

    findings.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.code_lines.cmp(&left.code_lines))
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(GodFileScanResult {
        root: root.display().to_string(),
        scanned_files,
        skipped_generated,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

pub(in crate::runner) fn run_generated_asset_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GeneratedAssetScanOptions,
) -> Result<GeneratedAssetScanResult, RunnerError> {
    let mut findings = Vec::<GeneratedAssetFinding>::new();
    let mut scanned_files = 0usize;
    let mut candidate_files = 0usize;

    for (root, skipped_roots) in workspace_scan_roots(target_root, scan_roots) {
        let result = run_generated_asset_scan_single(&root, &skipped_roots, options)?;
        scanned_files += result.scanned_files;
        candidate_files += result.candidate_files;
        findings.extend(result.findings.into_iter().map(|mut finding| {
            finding.path = rebase_finding_path(target_root, &root, &finding.path);
            finding
        }));
    }

    findings.sort_by(|left, right| {
        generated_asset_severity_rank(right.severity)
            .cmp(&generated_asset_severity_rank(left.severity))
            .then_with(|| right.bytes.cmp(&left.bytes))
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(GeneratedAssetScanResult {
        root: target_root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn run_generated_asset_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &GeneratedAssetScanOptions,
) -> Result<GeneratedAssetScanResult, RunnerError> {
    options.validate()?;
    let mut findings = Vec::<GeneratedAssetFinding>::new();
    let mut scanned_files = 0usize;
    let mut candidate_files = 0usize;
    walk_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_generated_asset_path,
        |path, rel, rel_str| {
            let bytes = std::fs::metadata(path)
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "scan metadata failed for {}: {error}",
                        path.display()
                    ))
                })?
                .len() as usize;
            let sample = read_asset_sample(path)?;
            scanned_files += 1;
            let reason = match generated_asset_reason(rel, &sample) {
                Some(reason) => reason,
                None => return Ok(()),
            };
            candidate_files += 1;

            let severity = classify_generated_asset_severity(bytes, &options.thresholds);
            if let Some(severity) = severity {
                findings.push(GeneratedAssetFinding {
                    path: rel_str.to_owned(),
                    bytes,
                    severity,
                    reason,
                });
            }
            Ok(())
        },
    )?;

    findings.sort_by(|left, right| {
        generated_asset_severity_rank(right.severity)
            .cmp(&generated_asset_severity_rank(left.severity))
            .then_with(|| right.bytes.cmp(&left.bytes))
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(GeneratedAssetScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

pub(in crate::runner) fn run_attention_marker_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &AttentionMarkerScanOptions,
) -> Result<AttentionMarkerScanResult, RunnerError> {
    let mut findings = Vec::<AttentionMarkerFinding>::new();
    let mut scanned_files = 0usize;
    let mut matched_lines = 0usize;

    for (root, skipped_roots) in workspace_scan_roots(target_root, scan_roots) {
        let result = run_attention_marker_scan_single(&root, &skipped_roots, options)?;
        scanned_files += result.scanned_files;
        matched_lines += result.matched_lines;
        findings.extend(result.findings.into_iter().map(|mut finding| {
            finding.path = rebase_finding_path(target_root, &root, &finding.path);
            finding
        }));
    }

    findings.sort_by(|left, right| {
        attention_marker_severity_rank(right.severity)
            .cmp(&attention_marker_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });

    Ok(AttentionMarkerScanResult {
        root: target_root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}

fn run_attention_marker_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &AttentionMarkerScanOptions,
) -> Result<AttentionMarkerScanResult, RunnerError> {
    options.validate()?;
    let patterns = compile_attention_marker_patterns(&options.patterns);
    let mut findings = Vec::<AttentionMarkerFinding>::new();
    let mut scanned_files = 0usize;
    let mut matched_lines = 0usize;

    walk_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |path, rel, rel_str| {
            let contents = std::fs::read_to_string(path).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "scan read failed for {}: {error}",
                    path.display()
                ))
            })?;
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
                    });
                }
                if line_has_match {
                    matched_lines += 1;
                }
            }
            Ok(())
        },
    )?;

    findings.sort_by(|left, right| {
        attention_marker_severity_rank(right.severity)
            .cmp(&attention_marker_severity_rank(left.severity))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.marker.cmp(&right.marker))
    });

    Ok(AttentionMarkerScanResult {
        root: root.display().to_string(),
        scanned_files,
        matched_lines,
        findings,
        patterns: options.patterns.clone(),
    })
}
