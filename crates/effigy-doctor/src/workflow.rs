use super::progress::DoctorProgressReporter;
use crate::util::current_working_dir;
use crate::DoctorMode;
use crate::{DoctorError, DoctorReport, DoctorRuntimePorts};
use std::time::Duration;
#[path = "workflow/handler.rs"]
mod handler;
#[path = "workflow/phases.rs"]
mod phases;
#[path = "workflow/preparation.rs"]
mod preparation;

pub(crate) struct DoctorRunOutput {
    pub(crate) report: DoctorReport,
    pub(crate) error_count: usize,
}

pub(crate) const FAST_DOCTOR_TIMEOUT_MS: u64 = 10_000;
pub(crate) const DEEP_DOCTOR_TIMEOUT_MS: u64 = 120_000;
pub(crate) const DOCTOR_TIMEOUT_ENV: &str = "EFFIGY_DOCTOR_TIMEOUT_MS";

#[derive(Debug, Clone)]
pub(crate) struct DoctorRunConfig {
    pub(crate) mode: DoctorMode,
    pub(crate) catalog: Option<String>,
    pub(crate) all_catalogs: bool,
    pub(crate) refresh: bool,
    pub(crate) budget: Option<Duration>,
}

impl DoctorRunConfig {
    pub(crate) fn from_args(args: &effigy_cli::DoctorArgs) -> Result<Self, DoctorError> {
        let default_ms = if args.deep {
            DEEP_DOCTOR_TIMEOUT_MS
        } else {
            FAST_DOCTOR_TIMEOUT_MS
        };
        let timeout_ms = match std::env::var(DOCTOR_TIMEOUT_ENV) {
            Ok(value) => value.parse::<u64>().map_err(|_| {
                DoctorError::task_invocation(format!(
                    "{DOCTOR_TIMEOUT_ENV} value `{value}` is invalid (expected an unsigned millisecond count; `0` disables the deadline)"
                ))
            })?,
            Err(std::env::VarError::NotPresent) => default_ms,
            Err(error) => {
                return Err(DoctorError::task_invocation(format!(
                    "failed to read {DOCTOR_TIMEOUT_ENV}: {error}"
                )))
            }
        };
        Ok(Self {
            mode: if args.deep {
                DoctorMode::Deep
            } else {
                DoctorMode::Fast
            },
            catalog: args.catalog.clone(),
            all_catalogs: args.all_catalogs,
            refresh: args.refresh,
            budget: (timeout_ms != 0).then(|| Duration::from_millis(timeout_ms)),
        })
    }
}

pub(crate) fn run_doctor_workflow(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
    config: DoctorRunConfig,
    progress: Option<&mut DoctorProgressReporter>,
    ports: &dyn DoctorRuntimePorts,
) -> Result<DoctorRunOutput, DoctorError> {
    let cwd = current_working_dir()?;
    let mut handler =
        handler::DefaultWorkflowPhaseHandler::new(cwd.clone(), config, progress, ports);
    phases::run_workflow_phase_pipeline(cwd, repo_override, fix, &mut handler)
}

#[cfg(test)]
mod tests;
