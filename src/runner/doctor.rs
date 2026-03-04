use crate::DoctorArgs;

use super::{CatalogSelectionMode, LoadedCatalog, ManifestJsPackageManager, RunnerError};

mod conflicts;
mod contracts;
mod environment;
mod explain;
mod finding_templates;
mod health;
mod manifest;
mod references;
mod render;
mod render_support;
mod report;
mod run;
mod task_graph;
mod text_blocks;

pub(super) type DoctorSeverity = report::DoctorSeverity;
pub(super) type DoctorFinding = report::DoctorFinding;
pub(super) type DoctorFixStatus = report::DoctorFixStatus;
pub(super) type DoctorFixAction = report::DoctorFixAction;
pub(super) type DoctorReport = report::DoctorReport;
pub(super) type DoctorState = report::DoctorState;
pub(super) type ManifestSnapshot = report::ManifestSnapshot;

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
