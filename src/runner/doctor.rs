use crate::DoctorArgs;

use super::{CatalogSelectionMode, LoadedCatalog, ManifestJsPackageManager, RunnerError};

mod conflicts;
mod environment;
mod explain;
mod health;
mod manifest;
mod references;
mod render;
mod report;
mod run;

pub(super) type DoctorSeverity = report::DoctorSeverity;
pub(super) type DoctorFinding = report::DoctorFinding;
pub(super) type DoctorFixStatus = report::DoctorFixStatus;
pub(super) type DoctorFixAction = report::DoctorFixAction;
pub(super) type DoctorReport = report::DoctorReport;
pub(super) type DoctorState = report::DoctorState;
pub(super) type ManifestSnapshot = report::ManifestSnapshot;

pub(super) fn add_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut std::collections::HashMap<String, DoctorSeverity>,
    finding: DoctorFinding,
) {
    report::add_finding(findings, statuses, finding);
}

pub(super) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
    if let Some(request) = args.explain.clone() {
        return explain::run_doctor_explain(
            request,
            args.repo_override,
            args.output_json,
            args.fix,
            args.verbose,
        );
    }

    let outcome = run::run_doctor_workflow(args.repo_override.clone(), args.fix)?;
    render_doctor_result(
        &outcome.report,
        args.output_json,
        args.verbose,
        outcome.error_count,
    )
}

fn render_doctor_result(
    report: &DoctorReport,
    output_json: bool,
    verbose: bool,
    error_count: usize,
) -> Result<String, RunnerError> {
    let rendered = if output_json {
        render::render_json(report)?
    } else {
        render::render_text(report, verbose)?
    };
    if error_count > 0 {
        return Err(RunnerError::DoctorNonZero {
            error_count,
            rendered,
        });
    }
    Ok(rendered)
}
