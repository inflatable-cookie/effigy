//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

#[cfg(not(test))]
use std::io::Read;
#[cfg(all(unix, not(test)))]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(test))]
use std::process::Stdio;
#[cfg(not(test))]
use std::thread;
use std::time::{Duration, Instant};

#[cfg(all(unix, not(test)))]
use nix::unistd::{setpgid, Pid};

use effigy_cli::TaskInvocation;
use effigy_containers::{
    colima::parse_colima_running,
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    exec::{inspect_colima_ssh_agent_socket_for_profile, SshAgentSocketHealth},
    load_all_container_policies, user_global_backend_preference, user_global_colima_profile,
};
use effigy_doctor::{
    check_id, DoctorError, DoctorFinding, DoctorRuntimeDiagnostics, DoctorRuntimePorts,
    DoctorSeverity,
};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{DeferredCommand, LoadedCatalog, ManifestContainerDriver};
use effigy_tasks::TaskSelector;

use crate::runner::deferral;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::run_compose_exec_with_deadline;
use crate::runner::execute::api;
use crate::runner::system_command::is_primary_service_running;
use crate::runner::system_command::workspace_permissions::{
    compose_backend_with_deadline, diagnose_workspace_ownership, WorkspaceOwnershipProbeStatus,
};

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

    fn run_manifest_task_bounded(
        &self,
        invocation: &TaskInvocation,
        cwd: PathBuf,
        remaining_budget: Option<Duration>,
    ) -> Result<String, DoctorError> {
        #[cfg(test)]
        {
            let _ = remaining_budget;
            self.run_manifest_task(invocation, cwd)
        }
        #[cfg(not(test))]
        {
            let Some(budget) = remaining_budget else {
                return self.run_manifest_task(invocation, cwd);
            };
            run_manifest_task_subprocess_bounded(invocation, &cwd, budget)
        }
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
        collect_runtime_diagnostics(resolved_root, None)
    }

    fn runtime_diagnostics_bounded(
        &self,
        resolved_root: &Path,
        remaining_budget: Option<Duration>,
    ) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
        collect_runtime_diagnostics(resolved_root, remaining_budget)
    }
}

#[cfg(not(test))]
fn run_manifest_task_subprocess_bounded(
    invocation: &TaskInvocation,
    cwd: &Path,
    budget: Duration,
) -> Result<String, DoctorError> {
    if budget.is_zero() {
        return Err(DoctorError::BudgetExhausted {
            phase: "health_task".to_owned(),
        });
    }
    let executable = std::env::current_exe().map_err(|error| {
        DoctorError::task_invocation(format!("failed to resolve Effigy executable: {error}"))
    })?;
    let mut command = Command::new(executable);
    command
        .arg(&invocation.name)
        .args(&invocation.args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    let mut child = command.spawn().map_err(|error| {
        DoctorError::task_invocation(format!("failed to start bounded health task: {error}"))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DoctorError::task_invocation("health task stdout was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DoctorError::task_invocation("health task stderr was unavailable"))?;
    let stdout_reader = thread::spawn(move || read_process_stream(stdout));
    let stderr_reader = thread::spawn(move || read_process_stream(stderr));
    let deadline = Instant::now() + budget;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            DoctorError::task_invocation(format!("failed to poll bounded health task: {error}"))
        })? {
            break status;
        }
        if Instant::now() >= deadline {
            effigy_process::terminate_process_tree(child.id(), false);
            let grace_deadline = Instant::now() + Duration::from_millis(500);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= grace_deadline {
                    effigy_process::terminate_process_tree(child.id(), true);
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(DoctorError::BudgetExhausted {
                phase: "health_task".to_owned(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| DoctorError::task_invocation("health task stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| DoctorError::task_invocation("health task stderr reader panicked"))??;
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    if status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
    if !stdout.trim().is_empty() {
        Err(DoctorError::CommandJsonFailure { rendered: stdout })
    } else {
        Err(DoctorError::task_invocation(format!(
            "health task failed with {}{}",
            status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        )))
    }
}

#[cfg(not(test))]
fn read_process_stream(mut stream: impl Read) -> Result<Vec<u8>, DoctorError> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).map_err(|error| {
        DoctorError::task_invocation(format!("failed to read bounded health output: {error}"))
    })?;
    Ok(bytes)
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
    remaining_budget: Option<Duration>,
) -> Result<DoctorRuntimeDiagnostics, DoctorError> {
    let mut diagnostics = DoctorRuntimeDiagnostics::default();

    // Gateway route-table trust is machine-global and independent of container
    // policies, so surface it before any container-policy early return.
    append_route_table_trust_diagnostics(&mut diagnostics);

    // Catalog-pack health is machine-global too, and matters on repos with no
    // container policy at all, so it also precedes the early returns below.
    append_catalog_pack_diagnostics(&mut diagnostics);

    let policies = match load_all_container_policies(resolved_root) {
        Ok(value) => value,
        Err(_) => return Ok(diagnostics),
    };
    if policies.is_empty() {
        return Ok(diagnostics);
    }

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
                Ok(running) => {
                    diagnostics.evidence.push(format!(
                        "colima-profile `{profile}`: {}",
                        if running { "running" } else { "stopped" }
                    ));
                    if running {
                        append_ssh_agent_socket_warning(profile, resolved_root, &mut diagnostics);
                    }
                }
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

    append_workspace_ownership_diagnostics(
        resolved_root,
        &policies,
        remaining_budget,
        &mut diagnostics,
    );

    Ok(diagnostics)
}

fn append_workspace_ownership_diagnostics(
    repo_root: &Path,
    policies: &[effigy_containers::EffectiveContainerPolicy],
    remaining_budget: Option<Duration>,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    let deadline = remaining_budget.map(|budget| Instant::now() + budget);
    for policy in policies {
        let running =
            is_primary_service_running(repo_root, policy).map_err(|error| error.to_string());
        let extra = match running {
            Ok(true) => match bun_install_scan_target(repo_root, policy, deadline) {
                Ok(extra) => extra,
                Err(error) => {
                    diagnostics.warnings.push(format!(
                        "container `{}` workspace ownership probe skipped: {error}",
                        policy.name
                    ));
                    continue;
                }
            },
            _ => Vec::new(),
        };
        let mut backend = compose_backend_with_deadline(repo_root, policy, deadline);
        let diagnosis = diagnose_workspace_ownership(policy, running, &extra, &mut backend);
        if let Some(evidence) = diagnosis.evidence {
            diagnostics.evidence.push(evidence);
        }
        if let Some(warning) = diagnosis.warning {
            diagnostics.warnings.push(warning);
        }
        if diagnosis.status == WorkspaceOwnershipProbeStatus::Finding {
            let user = policy.workspace_user.as_deref().unwrap_or("workspace-user");
            diagnostics.findings.push(workspace_ownership_finding(
                &policy.name,
                user,
                &diagnosis.samples,
                diagnosis
                    .identity
                    .as_ref()
                    .map(|identity| (identity.uid, identity.gid)),
            ));
        }
    }
}

fn bun_install_scan_target(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<String>, String> {
    let args = effigy_containers::compose::compose_args(
        policy,
        [
            "exec",
            "-T",
            "-u",
            "0",
            policy.primary_service.as_str(),
            "sh",
            "-c",
            r#"if [ -n "$BUN_INSTALL" ]; then printf '%s/install\n' "$BUN_INSTALL"; fi"#,
        ],
    );
    match run_compose_exec_with_deadline(
        repo_root,
        policy,
        &args,
        true,
        "workspace bun cache path probe",
        deadline,
    ) {
        Ok(output) if output.status.success() => Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()),
        Err(error) => {
            let message = error.to_string();
            if message.contains("timed out") {
                Err(message)
            } else {
                Ok(Vec::new())
            }
        }
        _ => Ok(Vec::new()),
    }
}

fn workspace_ownership_finding(
    container_name: &str,
    workspace_user: &str,
    samples: &[String],
    numeric: Option<(u32, u32)>,
) -> DoctorFinding {
    let identity = match numeric {
        Some((uid, gid)) => format!("`{workspace_user}` (uid={uid} gid={gid})"),
        None => format!("`{workspace_user}`"),
    };
    DoctorFinding {
        check_id: check_id::CONTAINER_WORKSPACE_OWNERSHIP.to_owned(),
        severity: DoctorSeverity::Warning,
        evidence: format!(
            "container `{container_name}` has rust build/cache or managed workspace paths that the declared workspace user {identity} cannot use: {}",
            samples.join(", ")
        ),
        remediation: "Headless workspace, primary-service exec, and routed tasks repair owned disposable rust/cargo/target named volumes before children launch. If doctor still reports this, rerun `effigy doctor --verbose`. Do not recursively chown host source, sibling, or shared binds; isolate `target` with catalog `isolated_dirs` when the path is a host bind.".to_owned(),
        fixable: false,
    }
}

/// Surface installed catalog-pack health. A pack that has become unreadable or
/// incompatible resolves to the compiled baseline silently as far as compose
/// output is concerned, so doctor is where an operator finds out — with one
/// direct repair command.
fn append_catalog_pack_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    let selection =
        effigy_catalog::pack::select_pack(crate::runner::service_command::effigy_version());
    if let Some(finding) = crate::runner::service_command::pack_health_finding(&selection) {
        diagnostics.findings.push(finding);
        return;
    }
    if let Some(record) = selection.active.as_ref() {
        diagnostics.evidence.push(format!(
            "catalog-pack: active {} {} ({})",
            record.pack_id, record.pack_version, record.install_id
        ));
    }
}

/// Surface gateway route-table trust state (contract 033) as a doctor runtime
/// diagnostic: an evidence line when trusted, a remediation warning when not.
fn append_route_table_trust_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    use effigy_gateway::server::GatewayConfig;
    use effigy_gateway::trust::{inspect_route_table_trust, RouteTableTrust};

    let Ok(gateway_dir) = crate::runner::gateway_command::gateway_dir() else {
        return;
    };
    let route_table_path = GatewayConfig::standard(gateway_dir).route_table_path;

    match inspect_route_table_trust(&route_table_path) {
        // No table yet — nothing to report.
        RouteTableTrust::Absent => {}
        RouteTableTrust::Trusted => diagnostics
            .evidence
            .push("gateway-route-table-trust: trusted".to_string()),
        RouteTableTrust::Untrusted { reason } => diagnostics.warnings.push(format!(
            "gateway route table is untrusted ({reason}); the gateway keeps its last-known-good routes. Restore owner-only permissions (no group/other write) or re-register routes with `effigy container up` to re-stamp it."
        )),
    }
}

/// Flag a stale colima SSH-agent forwarding socket for a running profile. A
/// dangling `/run/host-services/ssh-auth.sock` (host agent socket rotated on a
/// long-running VM) makes `effigy container up` fail with `mkdir ... file
/// exists`; surface it here with the `colima restart` remediation (g08.017).
fn append_ssh_agent_socket_warning(
    profile: &str,
    repo_root: &Path,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    let detail = match inspect_colima_ssh_agent_socket_for_profile(profile, repo_root) {
        SshAgentSocketHealth::Stale => "is stale (host SSH-agent socket rotated)",
        SshAgentSocketHealth::Absent => "is not set up",
        SshAgentSocketHealth::Healthy | SshAgentSocketHealth::Unknown => return,
    };
    diagnostics.warnings.push(format!(
        "colima profile `{profile}`: workspace SSH-agent forwarding {detail}; `effigy container \
         up` can fail with `mkdir /run/host-services/ssh-auth.sock: file exists`. \
         Fix: `colima restart {profile}`."
    ));
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
    use super::{docker_context_mismatch_warning, workspace_ownership_finding};
    use effigy_containers::BackendId;
    use effigy_doctor::{check_id, DoctorSeverity};

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

    #[test]
    fn workspace_ownership_finding_is_named_and_actionable() {
        let finding = workspace_ownership_finding(
            "workspace",
            "dev",
            &["/workspace/target/debug/.cargo-build-lock".to_owned()],
            Some((501, 20)),
        );

        assert_eq!(finding.check_id, check_id::CONTAINER_WORKSPACE_OWNERSHIP);
        assert_eq!(finding.severity, DoctorSeverity::Warning);
        assert!(finding.evidence.contains("target/debug/.cargo-build-lock"));
        assert!(finding.evidence.contains("uid=501"));
        assert!(finding.evidence.contains("gid=20"));
        assert!(finding.remediation.contains("isolated_dirs"));
        assert!(finding.remediation.contains("effigy doctor"));
        assert!(!finding.fixable);
    }
}
