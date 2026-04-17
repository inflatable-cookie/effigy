use std::path::{Path, PathBuf};

use crate::DoctorError;
use crate::{DoctorFixAction, DoctorState};
use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::LoadedCatalog;

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
) -> Result<ManifestScanResult, DoctorError> {
    scan::collect_manifest_findings(resolved_root, state)
}

pub(super) fn apply_fixers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<DoctorFixAction> {
    fixers::apply_fixers(resolved_root, catalogs)
}
