use std::path::{Path, PathBuf};

use super::super::model::{
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, GeneratedAssetFinding,
    GeneratedAssetScanOptions, GeneratedAssetScanResult, GeneratedInSrcFinding,
    GeneratedInSrcScanOptions, GeneratedInSrcScanResult, GodFileFinding, GodFileScanOptions,
    GodFileScanResult,
};
use super::super::support::{
    classify_comment_ratio_severity, classify_generated_asset_severity,
    classify_generated_in_src_severity, classify_severity, comment_ratio_counts,
    comment_ratio_severity_rank, compile_glob_set, count_code_lines, generated_asset_reason,
    generated_asset_severity_rank, generated_in_src_category_rank, generated_in_src_reason,
    generated_in_src_severity_rank, is_generated_artifact, read_asset_sample, rebase_finding_path,
    severity_rank, should_skip_generated_asset_path, should_skip_path, walk_scan_files,
};
use super::{run_workspace_scan, walk_text_scan_files, RunnerError, ScanWorkspaceCounts};

pub(in crate::runner) fn run_god_file_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_god_file_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.skipped_generated;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_god_file_findings,
        |root, counts, findings| GodFileScanResult {
            root,
            scanned_files: counts.scanned_files,
            skipped_generated: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
        },
    )
}

pub(in crate::runner) fn run_generated_asset_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GeneratedAssetScanOptions,
) -> Result<GeneratedAssetScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_generated_asset_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.candidate_files;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_generated_asset_findings,
        |root, counts, findings| GeneratedAssetScanResult {
            root,
            scanned_files: counts.scanned_files,
            candidate_files: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
        },
    )
}

pub(in crate::runner) fn run_generated_in_src_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GeneratedInSrcScanOptions,
) -> Result<GeneratedInSrcScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_generated_in_src_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.candidate_files;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_generated_in_src_findings,
        |root, counts, findings| GeneratedInSrcScanResult {
            root,
            scanned_files: counts.scanned_files,
            candidate_files: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
            source_roots: options.source_roots.clone(),
        },
    )
}

pub(in crate::runner) fn run_comment_ratio_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &CommentRatioScanOptions,
) -> Result<CommentRatioScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_comment_ratio_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.candidate_files;
        },
        |result| result.findings,
        |root, finding| {
            finding.path = rebase_finding_path(target_root, root, &finding.path);
        },
        sort_comment_ratio_findings,
        |root, counts, findings| CommentRatioScanResult {
            root,
            scanned_files: counts.scanned_files,
            candidate_files: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
        },
    )
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
    walk_text_scan_files(
        root,
        skipped_roots,
        options.respect_gitignore,
        &options.include,
        &options.exclude,
        should_skip_path,
        |_path, rel, rel_str, contents| {
            if is_generated_artifact(rel, &contents) {
                skipped_generated += 1;
                return Ok(());
            }

            let code_lines = count_code_lines(rel, &contents);
            scanned_files += 1;
            if let Some(severity) = classify_severity(code_lines, &options.thresholds) {
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

    sort_god_file_findings(&mut findings);

    Ok(GodFileScanResult {
        root: root.display().to_string(),
        scanned_files,
        skipped_generated,
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
            let Some(reason) = generated_asset_reason(rel, &sample) else {
                return Ok(());
            };
            candidate_files += 1;

            if let Some(severity) = classify_generated_asset_severity(bytes, &options.thresholds) {
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

    sort_generated_asset_findings(&mut findings);

    Ok(GeneratedAssetScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn run_generated_in_src_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &GeneratedInSrcScanOptions,
) -> Result<GeneratedInSrcScanResult, RunnerError> {
    options.validate()?;
    let source_roots = compile_glob_set(&options.source_roots, "source_root")?;
    let mut findings = Vec::<GeneratedInSrcFinding>::new();
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
            if !source_roots
                .as_ref()
                .is_some_and(|set| set.is_match(rel_str))
            {
                return Ok(());
            }

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
            let Some((category, reason)) = generated_in_src_reason(rel, &sample) else {
                return Ok(());
            };
            candidate_files += 1;

            if let Some(severity) = classify_generated_in_src_severity(bytes, &options.thresholds) {
                findings.push(GeneratedInSrcFinding {
                    path: rel_str.to_owned(),
                    category,
                    severity,
                    reason,
                    size_bytes: bytes,
                });
            }
            Ok(())
        },
    )?;

    sort_generated_in_src_findings(&mut findings);

    Ok(GeneratedInSrcScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
        source_roots: options.source_roots.clone(),
    })
}

fn run_comment_ratio_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &CommentRatioScanOptions,
) -> Result<CommentRatioScanResult, RunnerError> {
    options.validate()?;
    let mut findings = Vec::<CommentRatioFinding>::new();
    let mut scanned_files = 0usize;
    let mut candidate_files = 0usize;

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
            let counts = comment_ratio_counts(rel, &contents);
            if counts.code_lines < options.thresholds.min_code_lines {
                return Ok(());
            }
            candidate_files += 1;
            let ratio = counts.comment_lines as f64 / counts.code_lines as f64;
            if let Some(severity) = classify_comment_ratio_severity(ratio, &options.thresholds) {
                findings.push(CommentRatioFinding {
                    path: rel_str.to_owned(),
                    code_lines: counts.code_lines,
                    comment_lines: counts.comment_lines,
                    ratio,
                    severity,
                });
            }
            Ok(())
        },
    )?;

    sort_comment_ratio_findings(&mut findings);

    Ok(CommentRatioScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn sort_god_file_findings(findings: &mut [GodFileFinding]) {
    findings.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.code_lines.cmp(&left.code_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn sort_generated_asset_findings(findings: &mut [GeneratedAssetFinding]) {
    findings.sort_by(|left, right| {
        generated_asset_severity_rank(right.severity)
            .cmp(&generated_asset_severity_rank(left.severity))
            .then_with(|| right.bytes.cmp(&left.bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn sort_generated_in_src_findings(findings: &mut [GeneratedInSrcFinding]) {
    findings.sort_by(|left, right| {
        generated_in_src_severity_rank(right.severity)
            .cmp(&generated_in_src_severity_rank(left.severity))
            .then_with(|| {
                generated_in_src_category_rank(right.category)
                    .cmp(&generated_in_src_category_rank(left.category))
            })
            .then_with(|| right.size_bytes.cmp(&left.size_bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn sort_comment_ratio_findings(findings: &mut [CommentRatioFinding]) {
    findings.sort_by(|left, right| {
        comment_ratio_severity_rank(right.severity)
            .cmp(&comment_ratio_severity_rank(left.severity))
            .then_with(|| right.ratio.total_cmp(&left.ratio))
            .then_with(|| right.comment_lines.cmp(&left.comment_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
}
