use super::*;

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
