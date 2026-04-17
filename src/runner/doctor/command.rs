use effigy_cli::DoctorArgs;

use super::report::DoctorReport;
use super::{explain, progress, render, run};
use crate::runner::error::RunnerError;

pub(in crate::runner) fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
    if let Some(request) = args.explain.clone() {
        return explain::run_doctor_explain(
            request,
            args.repo_override,
            args.output_json,
            args.fix,
            args.verbose,
        );
    }

    let outcome =
        run_workflow_with_progress(args.repo_override.clone(), args.fix, args.output_json)?;
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
) -> Result<run::DoctorRunOutput, RunnerError> {
    let mut progress = progress::DoctorProgressReporter::new(output_json);
    let outcome = run::run_doctor_workflow(repo_override, fix, progress.as_mut());
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
