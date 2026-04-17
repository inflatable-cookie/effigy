use std::path::{Path, PathBuf};

use super::super::support::{walk_scan_files, workspace_scan_roots};
use super::ScanError;

pub fn run_workspace_scan<
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
) -> Result<TResult, ScanError>
where
    FSingle: Fn(&Path, &[PathBuf]) -> Result<TLocalResult, ScanError>,
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

pub fn walk_text_scan_files<ShouldSkip, Visit>(
    root: &Path,
    skipped_roots: &[PathBuf],
    respect_gitignore: bool,
    include_patterns: &[String],
    exclude_patterns: &[String],
    should_skip: ShouldSkip,
    mut visit: Visit,
) -> Result<(), ScanError>
where
    ShouldSkip: Fn(&Path, &str, Option<&globset::GlobSet>, Option<&globset::GlobSet>) -> bool,
    Visit: FnMut(&Path, &Path, &str, String) -> Result<(), ScanError>,
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
                ScanError::invocation(format!("scan read failed for {}: {error}", path.display()))
            })?;
            visit(path, rel, rel_str, contents)
        },
    )
}

#[derive(Default)]
pub struct ScanWorkspaceCounts {
    pub scanned_files: usize,
    pub secondary: usize,
}
