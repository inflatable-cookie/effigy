use super::*;

pub(crate) struct ScanDoctorCheck {
    pub(crate) check_id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) remediation: &'static str,
}

pub(crate) fn run_scan_check<TOptions, TResult, FLoad, FRun>(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
    check: ScanDoctorCheck,
    load_options: FLoad,
    run_scan: FRun,
) where
    TOptions: DoctorIntegratedScanOptions,
    TResult: DoctorIntegratedScanResult,
    FLoad: FnOnce(&Path, &[LoadedCatalog]) -> Result<TOptions, RunnerError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, RunnerError>,
{
    let options = match load_options(resolved_root, catalogs) {
        Ok(options) => options,
        Err(error) => {
            state.add_check_error(
                check.check_id,
                format!("{} configuration is invalid: {error}", check.label),
                "Fix manifest parse/schema errors first, then re-run `effigy doctor`.",
            );
            return;
        }
    };
    if !options.doctor_enabled() {
        return;
    }

    let scan_roots = catalog_scan_roots(resolved_root, catalogs);
    let result = match run_scan(resolved_root, &scan_roots, &options) {
        Ok(result) => result,
        Err(error) => {
            state.add_check_error(
                check.check_id,
                format!("{} scan failed: {error}", check.label),
                "No action required.",
            );
            return;
        }
    };

    for finding in result.into_findings() {
        state.add_check_finding(
            check.check_id,
            finding.doctor_severity(),
            finding.doctor_evidence(),
            check.remediation,
            false,
        );
    }
}

pub(crate) trait DoctorIntegratedScanOptions {
    fn doctor_enabled(&self) -> bool;
}

pub(crate) trait DoctorIntegratedScanResult {
    type Finding: DoctorIntegratedScanFinding;

    fn into_findings(self) -> Vec<Self::Finding>;
}

pub(crate) trait DoctorIntegratedScanFinding {
    fn doctor_severity(&self) -> DoctorSeverity;
    fn doctor_evidence(&self) -> String;
}
