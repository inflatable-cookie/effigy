use std::path::{Path, PathBuf};

use super::super::super::model::{
    god_file_language, GodFileFinding, GodFileScanOptions, GodFileScanResult,
};
use super::super::super::support::{
    classify_severity, count_code_lines, is_generated_artifact, rebase_finding_path, severity_rank,
    should_skip_path,
};
use super::super::{
    run_workspace_scan, walk_text_scan_files, ScanError, ScanWorkspaceCounts, WorkspaceScan,
};

pub fn run_god_file_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, ScanError> {
    run_workspace_scan(
        WorkspaceScan {
            target_root,
            scan_roots,
            stats: ScanWorkspaceCounts::default(),
        },
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
            thresholds: options.thresholds,
            thresholds_source: options.thresholds_source(),
        },
    )
}

fn run_god_file_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, ScanError> {
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
            let thresholds = options.thresholds_for(rel);
            if let Some(severity) = classify_severity(code_lines, &thresholds) {
                findings.push(GodFileFinding {
                    path: rel_str.to_owned(),
                    code_lines,
                    total_lines: contents.lines().count(),
                    severity,
                    language: god_file_language(rel),
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
        thresholds: options.thresholds,
        thresholds_source: options.thresholds_source(),
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
