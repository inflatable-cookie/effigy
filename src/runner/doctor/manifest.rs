use std::path::{Path, PathBuf};

use super::super::manifest::config_sections::ManifestJsPackageManager;
use super::super::model::catalog::LoadedCatalog;
use super::report::{DoctorFixAction, DoctorState};
use crate::runner::error::RunnerError;

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
    state: &mut DoctorState,
) -> Result<ManifestScanResult, RunnerError> {
    scan::collect_manifest_findings(resolved_root, state)
}

pub(super) fn apply_fixers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<DoctorFixAction> {
    fixers::apply_fixers(resolved_root, catalogs)
}
