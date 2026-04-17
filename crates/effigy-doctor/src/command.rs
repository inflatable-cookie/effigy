use effigy_cli::DoctorArgs;

use super::{explain, progress, render, workflow};
use crate::{DoctorError, DoctorReport, DoctorRuntimePorts};

pub fn run_doctor(args: DoctorArgs, ports: &dyn DoctorRuntimePorts) -> Result<String, DoctorError> {
    if let Some(request) = args.explain.clone() {
        return explain::run_doctor_explain(
            request,
            args.repo_override,
            args.output_json,
            args.fix,
            args.verbose,
            ports,
        );
    }

    let outcome = run_workflow_with_progress(
        args.repo_override.clone(),
        args.fix,
        args.output_json,
        ports,
    )?;
    render_doctor_result(
        &outcome.report,
        args.output_json,
        args.verbose,
        outcome.error_count,
    )
}

fn run_workflow_with_progress(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
    output_json: bool,
    ports: &dyn DoctorRuntimePorts,
) -> Result<workflow::DoctorRunOutput, DoctorError> {
    let mut progress = progress::DoctorProgressReporter::new(output_json);
    let outcome = workflow::run_doctor_workflow(repo_override, fix, progress.as_mut(), ports);
    if let Some(progress) = progress.as_mut() {
        progress.finish();
    }
    outcome
}

fn render_doctor_result(
    report: &DoctorReport,
    output_json: bool,
    verbose: bool,
    error_count: usize,
) -> Result<String, DoctorError> {
    let rendered = if output_json {
        render::render_json(report)?
    } else {
        render::render_text(report, verbose)?
    };
    if error_count > 0 {
        return Err(DoctorError::DoctorNonZero {
            error_count,
            rendered,
        });
    }
    Ok(rendered)
}
