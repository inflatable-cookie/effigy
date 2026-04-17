use std::path::Path;

use super::finding_templates::HealthFinding;
use super::report::DoctorState;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
#[path = "health/invocation.rs"]
mod invocation;
#[path = "health/json_output.rs"]
mod json_output;
#[path = "health/summarize.rs"]
mod summarize;

pub(super) fn check_health_task(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    let health_catalogs = catalogs_with_health_task(catalogs);

    if health_catalogs.is_empty() {
        add_discovery_missing_finding(state);
        return;
    }

    add_discovery_found_finding(&health_catalogs, state);

    match invocation::run_health_task_json(resolved_root) {
        Ok(output) => {
            add_execute_success_finding(&output, state);
        }
        Err(error) => {
            add_execute_failure_finding(&error, state);
        }
    }
}

fn catalogs_with_health_task(catalogs: &[LoadedCatalog]) -> Vec<String> {
    catalogs
        .iter()
        .filter(|catalog| catalog.manifest.tasks.contains_key("health"))
        .map(|catalog| catalog.alias.clone())
        .collect::<Vec<String>>()
}

fn add_discovery_missing_finding(state: &mut DoctorState) {
    HealthFinding::discovery_missing().emit(state);
}

fn add_discovery_found_finding(health_catalogs: &[String], state: &mut DoctorState) {
    HealthFinding::discovery_found(health_catalogs).emit(state);
}

fn add_execute_success_finding(output: &str, state: &mut DoctorState) {
    HealthFinding::execution_success(summarize::summarize_health_task_json_success(output))
        .emit(state);
}

fn add_execute_failure_finding(error: &RunnerError, state: &mut DoctorState) {
    let failure_evidence = match error {
        RunnerError::CommandJsonFailure { rendered } => {
            summarize::summarize_health_task_json_failure(rendered)
        }
        _ => format!("health task execution failed: {error}"),
    };
    HealthFinding::execution_failure(failure_evidence).emit(state);
}
