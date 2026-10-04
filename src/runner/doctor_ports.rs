//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

use std::ffi::{OsStr, OsString};
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
    exec::{inspect_colima_ssh_agent_socket_for_profile_with_deadline, SshAgentSocketHealth},
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
use crate::runner::exec_command::{run_command_capture_until, run_compose_exec_with_deadline};
use crate::runner::execute::api;
use crate::runner::system_command::is_primary_service_running_with_deadline;
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
    // One monotonic deadline for every preliminary probe that follows. Each
    // probe converts its own remainder from this deadline, so no subprocess
    // gets a fresh budget of its own.
    let deadline = remaining_budget.map(|budget| Instant::now() + budget);
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
            match colima_profile_running(profile, deadline) {
                Ok(running) => {
                    diagnostics.evidence.push(format!(
                        "colima-profile `{profile}`: {}",
                        if running { "running" } else { "stopped" }
                    ));
                    if running {
                        append_ssh_agent_socket_warning(
                            profile,
                            resolved_root,
                            deadline,
                            &mut diagnostics,
                        );
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

    append_workspace_ownership_diagnostics(resolved_root, &policies, deadline, &mut diagnostics);

    Ok(diagnostics)
}

pub(in crate::runner) fn append_workspace_ownership_diagnostics(
    repo_root: &Path,
    policies: &[effigy_containers::EffectiveContainerPolicy],
    deadline: Option<Instant>,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    for policy in policies {
        let running = is_primary_service_running_with_deadline(repo_root, policy, deadline)
            .map_err(|error| error.to_string());
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
    deadline: Option<Instant>,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    let detail = match inspect_colima_ssh_agent_socket_for_profile_with_deadline(
        profile, repo_root, deadline,
    ) {
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

fn colima_profile_running(profile: &str, deadline: Option<Instant>) -> Result<bool, DoctorError> {
    let args = [
        OsString::from("status"),
        OsString::from("--profile"),
        OsString::from(profile),
    ];
    let output =
        run_command_capture_until(Path::new("."), OsStr::new("colima"), &args, None, deadline)
            .map_err(|error| DoctorError::task_invocation(error.to_string()))?;
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
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use effigy_containers::{
        load_workspace_ownership_plan, BackendId, EffectiveContainerPolicy, WorkspaceMountKind,
        WorkspaceRepairAuthority,
    };
    use effigy_doctor::{check_id, DoctorRuntimeDiagnostics, DoctorSeverity};

    use super::{
        append_workspace_ownership_diagnostics, docker_context_mismatch_warning,
        is_primary_service_running_with_deadline, workspace_ownership_finding,
    };
    use crate::contract_test_support::{lock_test, EnvGuard};
    use crate::runner::system_command::workspace_permissions::{
        compose_backend_with_deadline, diagnose_workspace_ownership, WorkspaceOwnershipProbeStatus,
    };
    use crate::runner::test_support::effective_container_policy;

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

    // ------------------------------------------------------------------
    // Preliminary doctor liveness probes (papercut effigy#074).
    //
    // These use private fresh-root fixtures only: a temporary directory with
    // a fake `colima` executable on PATH. They never touch a live endpoint, VM
    // or container, and they only observe child processes they started.
    // ------------------------------------------------------------------

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, body).expect("write fixture executable");
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(path).expect("stat").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("chmod fixture executable");
        }
    }

    /// Create a fresh fixture root with a manifest marker so a compose working
    /// directory resolves back to this repository root.
    fn fresh_root(label: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::Builder::new()
            .prefix(&format!("effigy-doctor-liveness-{label}-"))
            .tempdir()
            .expect("tempdir");
        let root = fs::canonicalize(temp.path()).expect("canonicalize fixture root");
        fs::write(
            root.join("effigy.toml"),
            "[containers]\ndefault = \"stack\"\n",
        )
        .expect("write manifest marker");
        (temp, root)
    }

    fn install_fake_colima(root: &Path, script: &str) -> PathBuf {
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("mkdir fake runtime bin");
        write_executable(&bin.join("colima"), script);
        bin
    }

    /// Prepend the fixture bin to PATH and pin the Colima backend. Holds the
    /// global test lock (reentrant) for the lifetime of the guard.
    fn with_runtime_env(bin: &Path) -> EnvGuard {
        let base = std::env::var("PATH").unwrap_or_default();
        EnvGuard::set_many(&[
            ("PATH", Some(format!("{}:{base}", bin.display()))),
            ("EFFIGY_COMPOSE_BACKEND", Some("colima".to_owned())),
        ])
    }

    fn policy_for(root: &Path) -> EffectiveContainerPolicy {
        let mut policy = effective_container_policy(
            "stack",
            "demo-stack",
            "workspace",
            root.join("docker-compose.yml"),
        );
        policy.repo_root = root.to_path_buf();
        policy.workspace_user = Some("dev".to_owned());
        policy
    }

    fn named_rust_volume_policy(root: &Path) -> EffectiveContainerPolicy {
        let compose = root.join("docker-compose.yml");
        fs::write(
            &compose,
            r#"
services:
  workspace:
    volumes:
      - cargo-home:/usr/local/cargo
      - cargo-git:/usr/local/cargo/git
      - cargo-registry:/usr/local/cargo/registry
      - target:/workspace/target
volumes:
  cargo-home:
  cargo-git:
  cargo-registry:
  target:
"#,
        )
        .expect("write owned Rust volume fixture");
        let mut policy = policy_for(root);
        policy.compose_files = vec![compose];
        policy
    }

    fn verify_only_rust_volume_policy(root: &Path) -> EffectiveContainerPolicy {
        let compose = root.join("docker-compose.yml");
        fs::write(
            &compose,
            r#"
services:
  workspace:
    volumes:
      - ./target:/workspace/target
      - shared-cargo:/usr/local/cargo/git
  helper:
    volumes:
      - shared-cargo:/usr/local/cargo/git
volumes:
  shared-cargo:
"#,
        )
        .expect("write bind and shared volume fixture");
        let mut policy = policy_for(root);
        policy.compose_files = vec![compose];
        policy
    }

    fn install_fake_ownership_colima(
        root: &Path,
        delay_secs: &str,
        mode: &str,
        wrong_path: &str,
        process_group_file: &Path,
    ) -> PathBuf {
        let calls_file = root.join("ownership-execs.log");
        let path_log = root.join("ownership-paths.log");
        let access_log = root.join("ownership-access.log");
        let script = format!(
            r#"#!/bin/sh
case "$1" in
  status)
    printf 'status: Running\n'
    exit 0
    ;;
  nerdctl)
    case "$*" in
      *"ps"*) printf 'ps\n' >> '{calls_file}' ;;
      *effigy-workspace-identity*) printf 'identity\n' >> '{calls_file}' ;;
      *effigy-workspace-doctor-inspect-batch*) printf 'metadata\n' >> '{calls_file}' ;;
      *effigy-workspace-doctor-access-batch*) printf 'access\n' >> '{calls_file}' ;;
      *BUN_INSTALL*) printf 'bun\n' >> '{calls_file}' ;;
      *) printf 'other\n' >> '{calls_file}' ;;
    esac
    case "$*" in
      *"ps"*)
        sleep {delay_secs}
        printf 'demo-stack-1\tUp 2 minutes\t\tdemo-stack\t{root}\tworkspace\t0\n'
        exit 0
        ;;
      *effigy-workspace-identity*)
        sleep {delay_secs}
        printf '501\n20\n'
        exit 0
        ;;
      *effigy-workspace-doctor-inspect-batch*)
        if [ '{mode}' = 'hang' ]; then
          printf '%s\n' "$$" > '{process_group_file}'
          sleep 300 &
          printf '%s\n' "$!" >> '{process_group_file}'
          wait
        fi
        sleep {delay_secs}
        record=0
        for arg do
          if [ "$arg" = 'effigy-workspace-doctor-inspect-batch' ]; then
            record=1
            continue
          fi
          if [ "$record" -eq 1 ]; then
            printf '%s\n' "$arg" >> '{path_log}'
            if [ "$arg" = '{wrong_path}' ]; then
              printf 'file 0 0 644\n'
            else
              printf 'dir 501 20 755\n'
            fi
          fi
        done
        exit 0
        ;;
      *effigy-workspace-doctor-access-batch*)
        case "$*" in
          *'501:20'*) printf '501:20\n' >> '{access_log}' ;;
          *) printf 'wrong-identity\n' >> '{access_log}' ;;
        esac
        sleep {delay_secs}
        record=0
        for arg do
          if [ "$arg" = 'effigy-workspace-doctor-access-batch' ]; then
            record=1
            continue
          fi
          if [ "$record" -eq 1 ]; then
            if [ "$arg" = '{wrong_path}' ]; then
              printf 'unwritable\n'
            else
              printf 'read-write\n'
            fi
          fi
        done
        exit 0
        ;;
      *BUN_INSTALL*)
        sleep {delay_secs}
        exit 0
        ;;
      *)
        exit 0
        ;;
    esac
    ;;
  *)
    exit 0
    ;;
esac
"#,
            calls_file = calls_file.display(),
            path_log = path_log.display(),
            access_log = access_log.display(),
            process_group_file = process_group_file.display(),
            root = root.display(),
        );
        install_fake_colima(root, &script)
    }

    /// Generous bound for a fixture that hangs for 300s. It must still be far
    /// below the sleep it would otherwise wait out, so the bounded probe is
    /// proven without depending on exact host spawn latency.
    const LIVENESS_DEADLINE: Duration = Duration::from_secs(10);
    const LIVENESS_UPPER_BOUND: Duration = Duration::from_secs(30);

    #[cfg(unix)]
    fn pid_alive(pid: i32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
    }

    #[cfg(unix)]
    fn wait_for_processes_gone(pgfile: &Path) {
        let text = fs::read_to_string(pgfile).expect("read recorded probe pids");
        let pids = text
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .collect::<Vec<_>>();
        assert!(
            !pids.is_empty(),
            "fixture must record its own process group: {text:?}"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while pids.iter().any(|pid| pid_alive(*pid)) {
            assert!(
                Instant::now() < deadline,
                "recorded probe process group was not reaped: {text:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn expired_deadline_never_spawns_the_colima_liveness_probe() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("expired-liveness");
        let marker = root.join("colima-spawned");
        let bin = install_fake_colima(
            &root,
            &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
        );
        let _env = with_runtime_env(&bin);
        let policy = policy_for(&root);
        let expired = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .expect("expired instant");

        let error = is_primary_service_running_with_deadline(&root, &policy, Some(expired))
            .expect_err("expired deadline must not report a live runtime");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "expired deadline must never spawn the probe"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hung_colima_liveness_probe_is_bounded_and_reaps_its_process_group() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("hung-colima-liveness");
        let pgfile = root.join("colima-process-group");
        let bin = install_fake_colima(
            &root,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$$\" > '{pg}'\nsleep 300 &\nprintf '%s\\n' \"$!\" >> '{pg}'\nwait\n",
                pg = pgfile.display()
            ),
        );
        let _env = with_runtime_env(&bin);
        let policy = policy_for(&root);

        let started = Instant::now();
        let error = is_primary_service_running_with_deadline(
            &root,
            &policy,
            Some(Instant::now() + LIVENESS_DEADLINE),
        )
        .expect_err("hung probe must time out");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            started.elapsed() < LIVENESS_UPPER_BOUND,
            "hung probe must die at the deadline, not wait out sleep 300"
        );
        wait_for_processes_gone(&pgfile);
    }

    #[cfg(unix)]
    #[test]
    fn hung_compose_ps_probe_is_bounded_by_the_shared_deadline() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("hung-compose-ps");
        let marker = root.join("ps-spawned");
        let spawn_log = root.join("ps-argv");
        let bin = install_fake_colima(
            &root,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{log}'\ncase \"$1\" in\n  status) printf 'status: Running\\n'; exit 0 ;;\n  nerdctl) printf 'spawned\\n' >> '{}'; sleep 300 ;;\n  *) exit 0 ;;\nesac\n",
                marker.display(),
                log = spawn_log.display()
            ),
        );
        let _env = with_runtime_env(&bin);
        let policy = policy_for(&root);

        let started = Instant::now();
        let error = is_primary_service_running_with_deadline(
            &root,
            &policy,
            Some(Instant::now() + LIVENESS_DEADLINE),
        )
        .expect_err("hung compose ps must time out");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            started.elapsed() < LIVENESS_UPPER_BOUND,
            "compose ps probe must die at the shared deadline"
        );
        assert!(
            marker.exists(),
            "the composed probe must have been exercised; argv log: {:?}",
            fs::read_to_string(&spawn_log).unwrap_or_default()
        );
    }

    #[test]
    fn successful_and_stopped_liveness_distinctions_survive_a_deadline() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("liveness-distinctions");
        let running_marker = root.join("stopped-probe-spawned");
        let bin = install_fake_colima(
            &root,
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n  status) printf 'status: Running\\n'; exit 0 ;;\n  nerdctl) printf 'demo-stack-1\\tUp 2 minutes\\t\\tdemo-stack\\t{}\\tworkspace\\t0\\n'; exit 0 ;;\n  *) exit 0 ;;\nesac\n",
                root.display()
            ),
        );
        {
            let _env = with_runtime_env(&bin);
            let policy = policy_for(&root);
            let deadline = Some(Instant::now() + LIVENESS_DEADLINE);
            let running = is_primary_service_running_with_deadline(&root, &policy, deadline)
                .expect("bounded successful liveness probe");
            assert!(running, "running primary service must read as live");
        }

        let stopped_marker_bin = install_fake_colima(
            &root,
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n  status) printf 'status: Stopped\\n'; exit 0 ;;\n  *) touch '{}'; exit 0 ;;\nesac\n",
                running_marker.display()
            ),
        );
        let _env = with_runtime_env(&stopped_marker_bin);
        let policy = policy_for(&root);
        let stopped = is_primary_service_running_with_deadline(
            &root,
            &policy,
            Some(Instant::now() + LIVENESS_DEADLINE),
        )
        .expect("bounded stopped liveness probe");
        assert!(!stopped, "stopped Colima must read as not running");
        assert!(
            !running_marker.exists(),
            "a stopped profile must not run the compose probe"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ownership_probe_reports_unavailable_not_clean_when_liveness_times_out() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-budget");
        let bin = install_fake_colima(
            &root,
            "#!/bin/sh\ncase \"$1\" in\n  status) sleep 300 ;;\n  *) sleep 300 ;;\nesac\n",
        );
        let _env = with_runtime_env(&bin);
        let policy = policy_for(&root);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();

        let started = Instant::now();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(Instant::now() + LIVENESS_DEADLINE),
            &mut diagnostics,
        );

        assert!(
            started.elapsed() < LIVENESS_UPPER_BOUND,
            "ownership diagnostics must share the liveness deadline"
        );
        assert!(
            diagnostics.findings.is_empty(),
            "a timed-out liveness probe must never produce a workspace finding"
        );
        assert!(
            diagnostics
                .warnings
                .iter()
                .any(|warning| warning.contains("workspace ownership probe skipped")),
            "timeout must be reported as unavailable, got {:?}",
            diagnostics.warnings
        );
        assert!(
            diagnostics
                .evidence
                .iter()
                .all(|line| !line.contains("clean")),
            "timeout must never report clean ownership, got {:?}",
            diagnostics.evidence
        );
    }
    #[cfg(unix)]
    #[test]
    fn healthy_named_volume_ownership_batches_probes_within_doctor_budget() {
        const PER_EXEC_DELAY_MS: u64 = 430;
        // Keep this aligned with effigy-doctor's existing fast-doctor budget.
        const DOCTOR_BUDGET_MS: u64 = 10_000;
        // Before batching: four roots plus six nested Rust samples (including
        // overlapping declarations), each with root and numeric-user execs,
        // two identity execs, liveness, and Bun detection. This is a lower
        // bound on launches because it counts liveness only once.
        const SERIAL_EXEC_LOWER_BOUND: u64 = 24;
        const BATCHED_EXEC_COUNT: u64 = 10;

        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-latency");
        let policy = named_rust_volume_policy(&root);
        let pgfile = root.join("unused-process-group");
        let bin = install_fake_ownership_colima(&root, "0.43", "healthy", "", &pgfile);
        let _env = with_runtime_env(&bin);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        let started = Instant::now();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(started + Duration::from_millis(DOCTOR_BUDGET_MS)),
            &mut diagnostics,
        );

        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(DOCTOR_BUDGET_MS),
            "healthy owned Rust volumes must fit the existing doctor budget; elapsed={elapsed:?}, warnings={:?}",
            diagnostics.warnings
        );
        assert!(
            diagnostics.findings.is_empty(),
            "healthy named Rust volumes must not produce findings: {:?}",
            diagnostics.findings
        );
        assert!(
            diagnostics.warnings.is_empty(),
            "completed probes must not be reported unavailable: {:?}",
            diagnostics.warnings
        );
        assert!(
            diagnostics
                .evidence
                .iter()
                .any(|line| line.contains("workspace ownership: clean")),
            "all sampled roots and nested paths must be verified clean: {:?}",
            diagnostics.evidence
        );

        let calls = fs::read_to_string(root.join("ownership-execs.log")).expect("exec log");
        assert_eq!(
            calls.lines().count(),
            BATCHED_EXEC_COUNT as usize,
            "expected batched launch count, calls were {calls:?}"
        );
        println!(
            "ownership latency: {} backend launches in {elapsed:?} ({calls:?})",
            calls.lines().count()
        );
        assert!(
            Duration::from_millis(PER_EXEC_DELAY_MS * SERIAL_EXEC_LOWER_BOUND)
                > Duration::from_millis(DOCTOR_BUDGET_MS),
            "the former serial path count must exceed the same budget"
        );
        assert!(
            Duration::from_millis(PER_EXEC_DELAY_MS * BATCHED_EXEC_COUNT)
                < Duration::from_millis(DOCTOR_BUDGET_MS),
            "the batched path count must fit the same budget with controlled backend latency"
        );
        let paths = fs::read_to_string(root.join("ownership-paths.log")).expect("sample log");
        for path in [
            "/usr/local/cargo",
            "/usr/local/cargo/registry/src",
            "/usr/local/cargo/git/checkouts",
            "/workspace/target",
            "/workspace/target/debug",
            "/workspace/target/debug/.cargo-build-lock",
        ] {
            assert!(
                paths.lines().any(|sample| sample == path),
                "expected exact sampled path {path}; samples were {paths:?}"
            );
        }
        assert_eq!(
            paths.lines().count(),
            8,
            "known paths are sampled once each"
        );
        let access = fs::read_to_string(root.join("ownership-access.log")).expect("access log");
        assert_eq!(
            access.lines().count(),
            3,
            "access checks run in three batches"
        );
        assert!(
            access.lines().all(|identity| identity == "501:20"),
            "access checks must run as the resolved numeric uid/gid: {access:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn named_volume_batch_keeps_wrong_permissions_as_path_specific_finding() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-wrong-permissions");
        let bad_path = "/workspace/target/debug/.cargo-build-lock";
        let policy = named_rust_volume_policy(&root);
        let bin = install_fake_ownership_colima(
            &root,
            "0",
            "wrong",
            bad_path,
            &root.join("unused-process-group"),
        );
        let _env = with_runtime_env(&bin);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(Instant::now() + Duration::from_secs(8)),
            &mut diagnostics,
        );

        assert_eq!(
            diagnostics.findings.len(),
            1,
            "wrong ownership remains a finding"
        );
        assert!(
            diagnostics.findings[0]
                .evidence
                .contains(&format!("{bad_path}\tunwritable-by-uid-501")),
            "numeric-user failure must identify its exact path: {:?}",
            diagnostics.findings[0]
        );
        assert!(
            diagnostics.findings[0]
                .evidence
                .contains(&format!("{bad_path}\t{bad_path}")),
            "owner evidence must remain path-specific: {:?}",
            diagnostics.findings[0]
        );
        assert!(
            fs::read_to_string(root.join("ownership-access.log"))
                .expect("access log")
                .lines()
                .all(|identity| identity == "501:20"),
            "read/write checks must use the resolved uid and gid"
        );
    }

    #[test]
    fn doctor_batches_keep_bind_and_shared_volumes_verify_only() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-verify-only");
        let policy = verify_only_rust_volume_policy(&root);
        let plan = load_workspace_ownership_plan(&policy).expect("ownership plan");
        let bind_target = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace/target")
            .expect("bind target");
        assert_eq!(bind_target.mount_kind, WorkspaceMountKind::Bind);
        assert_eq!(
            bind_target.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        let shared_target = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("shared Cargo volume");
        assert_eq!(shared_target.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(
            shared_target.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );

        let bad_path = "/workspace/target/debug/.cargo-build-lock";
        let bin = install_fake_ownership_colima(
            &root,
            "0",
            "wrong",
            bad_path,
            &root.join("unused-process-group"),
        );
        let _env = with_runtime_env(&bin);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(Instant::now() + Duration::from_secs(8)),
            &mut diagnostics,
        );

        assert_eq!(diagnostics.findings.len(), 1);
        assert!(diagnostics.findings[0].evidence.contains(bad_path));
        let samples = fs::read_to_string(root.join("ownership-paths.log")).expect("sample log");
        assert!(
            samples
                .lines()
                .any(|sample| sample == "/usr/local/cargo/git/checkouts"),
            "the shared Cargo mount must retain its known nested sample: {samples:?}"
        );
        let calls = fs::read_to_string(root.join("ownership-execs.log")).expect("exec log");
        assert!(
            calls.lines().all(|kind| kind != "other"),
            "doctor ownership diagnosis must not launch mutating commands: {calls:?}"
        );
    }

    #[test]
    fn expired_ownership_deadline_does_not_spawn_identity_probe() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-expired");
        let policy = named_rust_volume_policy(&root);
        let bin = install_fake_ownership_colima(
            &root,
            "0",
            "healthy",
            "",
            &root.join("unused-process-group"),
        );
        let _env = with_runtime_env(&bin);
        let deadline = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("expired deadline");
        let mut backend = compose_backend_with_deadline(&root, &policy, Some(deadline));

        let diagnosis = diagnose_workspace_ownership(&policy, Ok(true), &[], &mut backend);

        assert_eq!(diagnosis.status, WorkspaceOwnershipProbeStatus::Unavailable);
        assert!(diagnosis.evidence.is_none());
        assert!(diagnosis.samples.is_empty());
        assert!(
            diagnosis
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("verification incomplete")
                    && warning.contains("workspace identity probe timed out")),
            "expired deadline must be reported as incomplete: {:?}",
            diagnosis.warning
        );
        assert!(
            !root.join("ownership-execs.log").exists(),
            "an expired ownership deadline must not spawn a backend command"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hung_ownership_batch_is_bounded_and_reaps_its_process_group() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-hung-batch");
        let policy = named_rust_volume_policy(&root);
        let process_group = root.join("ownership-process-group");
        let bin = install_fake_ownership_colima(&root, "0", "hang", "", &process_group);
        let _env = with_runtime_env(&bin);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        let started = Instant::now();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(started + Duration::from_secs(2)),
            &mut diagnostics,
        );

        assert!(started.elapsed() < Duration::from_secs(15));
        assert!(diagnostics.findings.is_empty());
        assert!(
            diagnostics
                .warnings
                .iter()
                .any(|warning| warning.contains("verification incomplete")
                    && warning.contains("workspace ownership metadata batch")
                    && warning.contains("/usr/local/cargo")),
            "a genuine backend hang must be unavailable with its exact batch context: {:?}",
            diagnostics.warnings
        );
        assert!(
            diagnostics
                .evidence
                .iter()
                .all(|line| !line.contains("clean")),
            "an incomplete batch must not claim clean ownership"
        );
        wait_for_processes_gone(&process_group);
    }
}
