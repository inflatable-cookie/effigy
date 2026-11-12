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
    request: WorkspaceScan<'_, TStats>,
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
    let WorkspaceScan {
        target_root,
        scan_roots,
        mut stats,
    } = request;
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

pub struct WorkspaceScan<'a, TStats> {
    pub target_root: &'a Path,
    pub scan_roots: &'a [PathBuf],
    pub stats: TStats,
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
            // Text scans need text: unreadable files are typed read
            // failures and non-UTF-8 bytes are typed content refusals,
            // never a generic invocation string.
            let contents = std::fs::read_to_string(path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::InvalidData {
                    ScanError::non_utf8_contents(path)
                } else {
                    ScanError::read_failed(path.display(), error)
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(root: &std::path::Path) -> Result<Vec<String>, ScanError> {
        let mut seen = Vec::new();
        walk_text_scan_files(
            root,
            &[],
            false,
            &[],
            &[],
            |_, _, _, _| false,
            |_, _, rel_str, _| {
                seen.push(rel_str.to_owned());
                Ok(())
            },
        )?;
        Ok(seen)
    }

    #[test]
    fn non_utf8_contents_are_a_typed_contents_refusal() {
        // `read_to_string` fails with `InvalidData` on non-UTF-8 bytes;
        // the text walk reports `NonUtf8Contents`, not a generic string.
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("binary.rs"), [0xff, 0xfe, 0x62]).expect("write");
        let error = collect(temp.path()).expect_err("non-UTF-8 contents must fail");
        assert!(
            matches!(error, ScanError::NonUtf8Contents { .. }),
            "got: {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_files_are_a_typed_read_failure() {
        // A permissionless regular file still lists in the walk, so the
        // text read fails with `PermissionDenied`: typed `ReadFailed`.
        // Root can read anything, so only assert when the read refuses.
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("secret.rs");
        std::fs::write(&path, b"fn secret() {}\n").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read_to_string(&path).is_err() {
            let error = collect(temp.path()).expect_err("unreadable file must fail");
            assert!(
                matches!(error, ScanError::ReadFailed { .. }),
                "got: {error}"
            );
            assert!(error.to_string().contains("secret.rs"), "got: {error}");
        }
    }
}
