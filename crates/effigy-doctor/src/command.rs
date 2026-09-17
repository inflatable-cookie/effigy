use effigy_cli::DoctorArgs;

use super::{explain, progress, render, workflow};
use crate::{DoctorError, DoctorReport, DoctorRuntimePorts};

pub fn run_doctor(args: DoctorArgs, ports: &dyn DoctorRuntimePorts) -> Result<String, DoctorError> {
    validate_invocation(&args)?;
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
        workflow::DoctorRunConfig::from_args(&args)?,
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
    config: workflow::DoctorRunConfig,
    ports: &dyn DoctorRuntimePorts,
) -> Result<workflow::DoctorRunOutput, DoctorError> {
    let mut progress = progress::DoctorProgressReporter::new(output_json);
    let outcome =
        workflow::run_doctor_workflow(repo_override, fix, config, progress.as_mut(), ports);
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
    if error_count > 0 || !report.complete {
        return Err(DoctorError::DoctorNonZero {
            error_count: error_count.max(usize::from(!report.complete)),
            rendered,
        });
    }
    Ok(rendered)
}

fn validate_invocation(args: &DoctorArgs) -> Result<(), DoctorError> {
    if args.catalog.is_some() && args.all_catalogs {
        return Err(DoctorError::task_invocation(
            "`--catalog` and `--all-catalogs` are mutually exclusive",
        ));
    }
    if !args.deep && (args.catalog.is_some() || args.all_catalogs || args.refresh) {
        return Err(DoctorError::task_invocation(
            "`--catalog`, `--all-catalogs`, and `--refresh` require `--deep`",
        ));
    }
    if args.explain.is_some()
        && (args.deep || args.catalog.is_some() || args.all_catalogs || args.refresh)
    {
        return Err(DoctorError::task_invocation(
            "doctor explanation mode cannot combine with `--deep`, `--catalog`, `--all-catalogs`, or `--refresh`",
        ));
    }
    Ok(())
}
