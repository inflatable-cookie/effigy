use std::path::Path;
use std::time::Duration;

use effigy_manifest::LoadedCatalog;

use super::finding_templates::HealthFinding;
use crate::{DoctorError, DoctorRuntimePorts, DoctorState};

#[path = "health/bootstrap.rs"]
mod bootstrap;
#[path = "health/invocation.rs"]
mod invocation;
#[path = "health/json_output.rs"]
mod json_output;
#[path = "health/summarize.rs"]
mod summarize;

#[cfg(test)]
#[path = "health/tests.rs"]
mod tests;

/// What the health check will do, decided without touching doctor state, so
/// the task can start before the deep scan finishes.
pub(crate) enum HealthPlan {
    NoTask,
    Blocked {
        health_catalogs: Vec<String>,
        evidence: String,
        remediation: String,
    },
    Run {
        health_catalogs: Vec<String>,
    },
}

pub(crate) fn plan_health_task(
    scope_root: &Path,
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
) -> HealthPlan {
    let health_catalogs = catalogs_with_health_task(catalogs);
    if health_catalogs.is_empty() {
        return HealthPlan::NoTask;
    }
    match selected_catalog(catalogs, scope_root).and_then(|catalog| {
        bootstrap::health_js_bootstrap_gap(scope_root, workspace_root, &catalog.manifest)
    }) {
        Some(gap) => HealthPlan::Blocked {
            health_catalogs,
            evidence: gap.evidence(),
            remediation: gap.remediation(),
        },
        None => HealthPlan::Run { health_catalogs },
    }
}

impl HealthPlan {
    /// Runs the task only when the plan needs it. It touches no doctor state,
    /// so the caller may run it while the deep scan walks.
    pub(crate) fn invoke(
        &self,
        scope_root: &Path,
        ports: &dyn DoctorRuntimePorts,
        remaining_budget: Option<Duration>,
    ) -> Option<Result<String, DoctorError>> {
        match self {
            Self::Run { .. } => Some(invocation::run_health_task_json(
                scope_root,
                ports,
                remaining_budget,
            )),
            Self::NoTask | Self::Blocked { .. } => None,
        }
    }

    /// Records the findings. `outcome` is the result of `invoke`; it is present
    /// exactly when the plan is `Run`.
    pub(crate) fn record(
        self,
        outcome: Option<Result<String, DoctorError>>,
        state: &mut DoctorState,
    ) {
        match self {
            Self::NoTask => add_discovery_missing_finding(state),
            Self::Blocked {
                health_catalogs,
                evidence,
                remediation,
            } => {
                add_discovery_found_finding(&health_catalogs, state);
                HealthFinding::bootstrap_missing(evidence, remediation).emit(state);
            }
            Self::Run { health_catalogs } => {
                add_discovery_found_finding(&health_catalogs, state);
                match outcome {
                    Some(Ok(output)) => add_execute_success_finding(&output, state),
                    Some(Err(error)) => add_execute_failure_finding(&error, state),
                    None => {}
                }
            }
        }
    }
}

pub(crate) fn check_health_task(
    scope_root: &Path,
    workspace_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
    ports: &dyn DoctorRuntimePorts,
    remaining_budget: Option<Duration>,
) {
    let plan = plan_health_task(scope_root, workspace_root, catalogs);
    let outcome = plan.invoke(scope_root, ports, remaining_budget);
    plan.record(outcome, state);
}

fn selected_catalog<'a>(
    catalogs: &'a [LoadedCatalog],
    scope_root: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .find(|catalog| catalog.catalog_root == scope_root)
        .or_else(|| catalogs.first())
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

fn add_execute_failure_finding(error: &DoctorError, state: &mut DoctorState) {
    let failure_evidence = match error {
        DoctorError::TaskJsonFailure(failure) => summarize::summarize_health_task_failure(
            Some(failure.code),
            &failure.stdout,
            &failure.stderr,
        ),
        DoctorError::CommandJsonFailure { rendered } => {
            summarize::summarize_health_task_json_failure(rendered)
        }
        _ => format!("health task execution failed: {error}"),
    };
    HealthFinding::execution_failure(failure_evidence).emit(state);
}
