//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_cli::TaskInvocation;
use effigy_containers::{
    colima::parse_colima_running,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    load_all_container_policies, user_global_backend_preference, user_global_colima_profile,
};
use effigy_doctor::{DoctorError, DoctorRuntimeDiagnostics, DoctorRuntimePorts};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{DeferredCommand, LoadedCatalog, ManifestContainerDriver};
use effigy_tasks::TaskSelector;

use crate::runner::deferral;
use crate::runner::error::RunnerError;
use crate::runner::execute::api;

#[derive(Debug, Default)]
pub(in crate::runner) struct RunnerDoctorPorts;

impl RunnerDoctorPorts {
    pub(in crate::runner) fn new() -> Self {
        Self
    }
}

impl DoctorRuntimePorts for RunnerDoctorPorts {
    fn run_manifest_task(
        &self,
        invocation: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, DoctorError> {
        api::run_manifest_task_with_surface(invocation, cwd, ExecutionSurface::DirectCli)
            .map_err(runner_to_doctor)
    }

    fn select_deferral(
        &self,
        selector: &TaskSelector,
        catalogs: &[LoadedCatalog],
        cwd: &Path,
        workspace_root: &Path,
    ) -> Option<DeferredCommand> {
        deferral::select_deferral(selector, catalogs, cwd, workspace_root)
    }

    fn runtime_diagnostics(
        &self,
        resolved_root: &Path,
    ) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
        collect_runtime_diagnostics(resolved_root)
    }
}

fn runner_to_doctor(error: RunnerError) -> DoctorError {
    match error {
        RunnerError::CommandJsonFailure { rendered } => {
            DoctorError::CommandJsonFailure { rendered }
        }
        RunnerError::TaskInvocation(message) => DoctorError::TaskInvocation(message),
        RunnerError::Ui(message) => DoctorError::Ui(message),
        other => DoctorError::TaskInvocation(other.to_string()),
    }
}

fn collect_runtime_diagnostics(
    resolved_root: &Path,
) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
    let policies = match load_all_container_policies(resolved_root) {
        Ok(value) => value,
        Err(_) => return Ok(DoctorRuntimeDiagnostics::default()),
    };
    if policies.is_empty() {
        return Ok(DoctorRuntimeDiagnostics::default());
    }

    let mut diagnostics = DoctorRuntimeDiagnostics::default();
    let mut profiles = policies
        .iter()
        .filter(|policy| policy.driver == ManifestContainerDriver::Colima)
        .map(|policy| policy.profile.clone())
        .collect::<Vec<_>>();
    profiles.sort();
    profiles.dedup();

    if !profiles.is_empty() {
        let selected_backend = match resolve_compose_backend_for_repo(resolved_root, &policies[0]) {
            ComposeBackend::Docker => "docker-compose",
            ComposeBackend::ColimaNerdctl => "colima-nerdctl",
        };
        diagnostics.evidence.push(format!(
            "container-backend-selection: {selected_backend} (manifest driver=colima, profiles={})",
            profiles.join(", ")
        ));
        for profile in &profiles {
            match colima_profile_running(profile) {
                Ok(running) => diagnostics.evidence.push(format!(
                    "colima-profile `{profile}`: {}",
                    if running { "running" } else { "stopped" }
                )),
                Err(error) => diagnostics
                    .warnings
                    .push(format!("colima profile `{profile}` probe failed: {error}")),
            }
        }
    }

    let user_backend = user_global_backend_preference();
    if let Some(backend) = user_backend.clone() {
        diagnostics.evidence.push(format!(
            "user-global container backend preference: {backend}"
        ));
    }
    if let Some(profile) = user_global_colima_profile() {
        diagnostics
            .evidence
            .push(format!("user-global Colima profile preference: {profile}"));
    }

    match docker_context_name() {
        Ok(Some(context)) => {
            diagnostics
                .evidence
                .push(format!("docker-context: {context}"));
            if let Some(warning) =
                docker_context_mismatch_warning(&context, !profiles.is_empty(), user_backend)
            {
                diagnostics.warnings.push(warning);
            }
        }
        Ok(None) => {}
        Err(error) => diagnostics
            .warnings
            .push(format!("docker context probe failed: {error}")),
    }

    Ok(diagnostics)
}

fn docker_context_mismatch_warning(
    context: &str,
    has_colima_profiles: bool,
    user_backend: Option<effigy_containers::BackendId>,
) -> Option<String> {
    if !has_colima_profiles || user_backend == Some(effigy_containers::BackendId::colima_nerdctl())
    {
        return None;
    }
    Some(format!(
        "docker CLI context is `{context}`, but Effigy will prefer Colima for declared `driver = \"colima\"` containers. If Colima should stay your machine-wide default for unscoped runtime commands too, set `[containers] backend = \"containerd\"` in `~/.effigy/config.toml`."
    ))
}

fn colima_profile_running(profile: &str) -> Result<bool, DoctorError> {
    let output = Command::new("colima")
        .args(["status", "--profile", profile])
        .output()
        .map_err(|error| {
            DoctorError::task_invocation(format!(
                "failed to launch `colima status --profile {profile}`: {error}"
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_colima_running(&stdout, &stderr))
}

fn docker_context_name() -> Result<Option<String>, DoctorError> {
    let output = match Command::new("docker").args(["context", "show"]).output() {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(DoctorError::task_invocation(format!(
                "failed to launch `docker context show`: {error}"
            )));
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            return Ok(None);
        }
        return Err(DoctorError::task_invocation(format!(
            "`docker context show` failed: {stderr}"
        )));
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::docker_context_mismatch_warning;
    use effigy_containers::BackendId;

    #[test]
    fn docker_context_warning_shows_when_colima_repo_has_no_pinned_containerd_preference() {
        let warning =
            docker_context_mismatch_warning("default", true, Some(BackendId::docker_compose()))
                .expect("warning");
        assert!(warning.contains("docker CLI context is `default`"));
        assert!(warning.contains("[containers] backend = \"containerd\""));
    }

    #[test]
    fn docker_context_warning_stays_hidden_when_containerd_is_already_pinned() {
        assert_eq!(
            docker_context_mismatch_warning("default", true, Some(BackendId::colima_nerdctl())),
            None
        );
    }

    #[test]
    fn docker_context_warning_stays_hidden_without_colima_profiles() {
        assert_eq!(
            docker_context_mismatch_warning("default", false, None),
            None
        );
    }
}
