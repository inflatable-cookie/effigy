use super::*;

pub fn run_god_file_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &GodFileScanOptions,
) -> Result<GodFileScanResult, ScanError> {
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

pub fn run_comment_ratio_scan_workspace(
    target_root: &Path,
    scan_roots: &[PathBuf],
    options: &CommentRatioScanOptions,
) -> Result<CommentRatioScanResult, ScanError> {
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

fn run_comment_ratio_scan_single(
    root: &Path,
    skipped_roots: &[PathBuf],
    options: &CommentRatioScanOptions,
) -> Result<CommentRatioScanResult, ScanError> {
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

fn sort_comment_ratio_findings(findings: &mut [CommentRatioFinding]) {
    findings.sort_by(|left, right| {
        comment_ratio_severity_rank(right.severity)
            .cmp(&comment_ratio_severity_rank(left.severity))
            .then_with(|| right.ratio.total_cmp(&left.ratio))
            .then_with(|| right.comment_lines.cmp(&left.comment_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
}
