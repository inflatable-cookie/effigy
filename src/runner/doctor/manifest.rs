use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::{LoadedCatalog, ManifestJsPackageManager, RunnerError};
use super::{DoctorFinding, DoctorFixAction, DoctorSeverity};

mod fixers;
mod scan;
mod schema;

type ManifestScanResult = (
    Vec<PathBuf>,
    Vec<LoadedCatalog>,
    Option<ManifestJsPackageManager>,
    bool,
);

pub(super) fn collect_manifest_findings(
    resolved_root: &Path,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) -> Result<ManifestScanResult, RunnerError> {
    scan::collect_manifest_findings(resolved_root, findings, statuses)
}

pub(super) fn apply_fixers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<DoctorFixAction> {
    fixers::apply_fixers(resolved_root, catalogs)
}
