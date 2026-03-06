use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::model::{
    AttentionMarkerFinding, AttentionMarkerScanOptions, AttentionMarkerScanResult,
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, GeneratedAssetFinding,
    GeneratedAssetScanOptions, GeneratedAssetScanResult, GodFileFinding, GodFileScanOptions,
    GodFileScanResult,
};
use super::support::{
    attention_marker_category, attention_marker_matches_line, attention_marker_severity_rank,
    classify_comment_ratio_severity, classify_generated_asset_severity, classify_severity,
    comment_ratio_counts, comment_ratio_severity_rank, compile_attention_marker_patterns,
    count_code_lines, generated_asset_reason, generated_asset_severity_rank, is_generated_artifact,
    read_asset_sample, rebase_finding_path, severity_rank, should_skip_generated_asset_path,
    should_skip_path, trim_snippet, walk_scan_files, workspace_scan_roots,
};
use crate::runner::error::RunnerError;

#[path = "execution/duplicate_blocks.rs"]
mod duplicate_blocks;

pub(in crate::runner) use duplicate_blocks::run_duplicate_block_scan_workspace;

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

    sort_god_file_findings(&mut findings);

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

    sort_generated_asset_findings(&mut findings);

    Ok(GeneratedAssetScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_files,
        findings,
        thresholds: options.thresholds.clone(),
    })
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

pub(in crate::runner) fn run_attention_marker_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &AttentionMarkerScanOptions,
) -> Result<AttentionMarkerScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
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

pub(super) fn run_workspace_scan<
    TStats,
    TLocalResult,
    TFinding,
    TResult,
    FSingle,
    FMerge,
    FExtract,
    FRebase,
    FSort,
    FBuild,
>(
    target_root: &Path,
    scan_roots: &[PathBuf],
    mut stats: TStats,
    run_single: FSingle,
    mut merge_stats: FMerge,
    extract_findings: FExtract,
    mut rebase_finding: FRebase,
    sort_findings: FSort,
    build_result: FBuild,
) -> Result<TResult, RunnerError>
where
    FSingle: Fn(&Path, &[PathBuf]) -> Result<TLocalResult, RunnerError>,
    FMerge: FnMut(&mut TStats, &TLocalResult),
    FExtract: Fn(TLocalResult) -> Vec<TFinding>,
    FRebase: FnMut(&Path, &mut TFinding),
    FSort: FnOnce(&mut [TFinding]),
    FBuild: FnOnce(String, TStats, Vec<TFinding>) -> TResult,
{
    let mut findings = Vec::new();
    for (root, skipped_roots) in workspace_scan_roots(target_root, scan_roots) {
        let result = run_single(&root, &skipped_roots)?;
        merge_stats(&mut stats, &result);
        let mut root_findings = extract_findings(result);
        for finding in &mut root_findings {
            rebase_finding(&root, finding);
        }
        findings.extend(root_findings);
    }

    sort_findings(findings.as_mut_slice());
    Ok(build_result(
        target_root.display().to_string(),
        stats,
        findings,
    ))
}

pub(super) fn walk_text_scan_files<ShouldSkip, Visit>(
    root: &Path,
    skipped_roots: &[PathBuf],
    respect_gitignore: bool,
    include_patterns: &[String],
    exclude_patterns: &[String],
    should_skip: ShouldSkip,
    mut visit: Visit,
) -> Result<(), RunnerError>
where
    ShouldSkip: Fn(&Path, &str, Option<&globset::GlobSet>, Option<&globset::GlobSet>) -> bool,
    Visit: FnMut(&Path, &Path, &str, String) -> Result<(), RunnerError>,
{
    walk_scan_files(
        root,
        skipped_roots,
        respect_gitignore,
        include_patterns,
        exclude_patterns,
        should_skip,
        |path, rel, rel_str| {
            let contents = std::fs::read_to_string(path).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "scan read failed for {}: {error}",
                    path.display()
                ))
            })?;
            visit(path, rel, rel_str, contents)
        },
    )
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

fn sort_comment_ratio_findings(findings: &mut [CommentRatioFinding]) {
    findings.sort_by(|left, right| {
        comment_ratio_severity_rank(right.severity)
            .cmp(&comment_ratio_severity_rank(left.severity))
            .then_with(|| right.ratio.total_cmp(&left.ratio))
            .then_with(|| right.comment_lines.cmp(&left.comment_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
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

#[derive(Default)]
pub(super) struct ScanWorkspaceCounts {
    pub(super) scanned_files: usize,
    pub(super) secondary: usize,
}
