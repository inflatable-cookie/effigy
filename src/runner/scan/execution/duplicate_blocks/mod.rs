use std::path::{Path, PathBuf};

use super::super::model::{
    DuplicateBlockFinding, DuplicateBlockLocation, DuplicateBlockScanOptions,
    DuplicateBlockScanResult, DuplicateBlockSeverity,
};
use super::super::support::{
    classify_duplicate_block_severity, duplicate_block_severity_rank, is_generated_artifact,
    normalized_code_lines, rebase_finding_path, should_skip_path, trim_snippet, NormalizedCodeLine,
};
use super::{run_workspace_scan, walk_text_scan_files, RunnerError, ScanWorkspaceCounts};

mod engine;

use engine::{candidate_block_count, detect_duplicate_blocks, DuplicateBlockFile};

pub(in crate::runner) fn run_duplicate_block_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &DuplicateBlockScanOptions,
) -> Result<DuplicateBlockScanResult, RunnerError> {
    run_workspace_scan(
        target_root,
        scan_roots,
        ScanWorkspaceCounts::default(),
        |root, skipped_roots| run_duplicate_block_scan_single(root, skipped_roots, options),
        |counts, result| {
            counts.scanned_files += result.scanned_files;
            counts.secondary += result.candidate_blocks;
        },
        |result| result.findings,
        |root, finding| {
            for location in &mut finding.locations {
                location.path = rebase_finding_path(target_root, root, &location.path);
            }
        },
        sort_duplicate_block_findings,
        |root, counts, findings| DuplicateBlockScanResult {
            root,
            scanned_files: counts.scanned_files,
            candidate_blocks: counts.secondary,
            findings,
            thresholds: options.thresholds.clone(),
        },
    )
}

fn run_duplicate_block_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &DuplicateBlockScanOptions,
) -> Result<DuplicateBlockScanResult, RunnerError> {
    options.validate()?;
    let mut scanned_files = 0usize;
    let mut file_blocks = Vec::<DuplicateBlockFile>::new();

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

            let lines = normalized_code_lines(rel, &contents);
            scanned_files += 1;
            if lines.len() >= options.thresholds.warn {
                file_blocks.push(DuplicateBlockFile {
                    path: rel_str.to_owned(),
                    lines,
                });
            }
            Ok(())
        },
    )?;

    let candidate_blocks = candidate_block_count(&file_blocks, options.thresholds.warn);
    let mut findings = detect_duplicate_blocks(&file_blocks, options)?;
    sort_duplicate_block_findings(&mut findings);

    Ok(DuplicateBlockScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_blocks,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

fn sort_duplicate_block_findings(findings: &mut [DuplicateBlockFinding]) {
    findings.sort_by(|left, right| {
        duplicate_block_severity_rank(right.severity)
            .cmp(&duplicate_block_severity_rank(left.severity))
            .then_with(|| right.block_lines.cmp(&left.block_lines))
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
}
