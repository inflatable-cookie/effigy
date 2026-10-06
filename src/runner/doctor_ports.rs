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
    ContainerAction,
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
use crate::runner::exec_command::{run_command_capture_until, run_compose_exec_plan_with_deadline};
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
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. The closure calls
    // `setpgid`, which is async-signal-safe, to place the not-yet-exec'd child
    // in a new process group whose id is the child's own pid. The error path
    // converts `nix::Error` (= `Errno`) with the allocation-free
    // `io::Error::from` impl. That group is exactly what
    // `effigy_process::terminate_process_tree(child.id())` targets on timeout
    // (`kill(-pgid, ...)`), so the bounded kill path owns the whole tree.
    unsafe {
        command
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
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
    let plan = match effigy_runtime::container_manager::compose_invocation_plan(
        repo_root,
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
        ContainerAction::Exec,
        "workspace bun cache path probe",
    ) {
        Ok(plan) => plan,
        Err(error) => return Err(error.to_string()),
    };
    match run_compose_exec_plan_with_deadline(policy, &plan, true, None, deadline) {
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
    use crate::runner::scripted_doctor::{
        self, OwnershipFixture, ScriptedPhase, ScriptedStatus, SCRIPTED_OPERATION_COST,
    };
    use crate::runner::system_command::workspace_permissions::{
        compose_backend_with_deadline, diagnose_workspace_ownership, rust_nested_probe_paths,
        WorkspaceOwnershipProbeStatus,
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
        if [ '{mode}' = 'hang-preflight' ]; then
          printf '%s\n' "$$" > '{process_group_file}'
          sleep 300 &
          printf '%s\n' "$!" >> '{process_group_file}'
          wait
        fi
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
    /// Deterministic scripted handler for one private fake-VM fixture. No
    /// subprocess is launched: the production argv decides the phase, the
    /// fixture answers it, and the runtime records the exact operation.
    fn scripted_ownership_handler(
        policy: &EffectiveContainerPolicy,
        fixture: OwnershipFixture,
    ) -> Box<scripted_doctor::ScriptedHandler> {
        scripted_doctor::ownership_handler(
            fixture,
            policy.project_name.clone(),
            policy.primary_service.clone(),
            policy.repo_root.display().to_string(),
        )
    }

    /// Pin the Colima backend without putting a fake runtime on PATH: if the
    /// scripted seam ever misses a launch, the real spawn fails loudly instead
    /// of silently answering.
    fn with_scripted_backend_env() -> EnvGuard {
        EnvGuard::set_many(&[("EFFIGY_COMPOSE_BACKEND", Some("colima".to_owned()))])
    }

    /// Pre-batching serial reference derived from the SAME workload: every
    /// applicable target root and each of its per-target nested rust probes is
    /// one metadata exec plus one numeric-user access exec, plus the Colima
    /// status/compose-ps/service-resolve, the Bun exec and the two identity
    /// execs the old path issued. It counts the pre-batching shape (no
    /// overlapping-path reuse) and is derived from the plan, not hardcoded.
    fn serial_reference_operations(policy: &EffectiveContainerPolicy) -> usize {
        let plan = load_workspace_ownership_plan(policy).expect("ownership plan");
        let mut path_operations = 0usize;
        for target in plan.targets.iter().filter(|target| {
            target.rust_cache.is_some()
                || target.repair_authority == WorkspaceRepairAuthority::OwnedDisposable
        }) {
            path_operations += 2;
            path_operations += 2 * rust_nested_probe_paths(&target.path, target.rust_cache).len();
        }
        const PREFLIGHT_AND_IDENTITY_OPERATIONS: usize = 6;
        path_operations + PREFLIGHT_AND_IDENTITY_OPERATIONS
    }

    fn assert_numeric_access_user(
        operations: &[scripted_doctor::ScriptedOperation],
        expected: &str,
    ) -> Result<(), String> {
        for operation in operations {
            if operation.phase != ScriptedPhase::AccessBatch {
                continue;
            }
            if operation.user.as_deref() != Some(expected) {
                return Err(format!(
                    "access batch ran as {:?}, expected {expected}: {}",
                    operation.user,
                    operation.rendered()
                ));
            }
        }
        Ok(())
    }

    fn assert_no_mutating_operations(
        operations: &[scripted_doctor::ScriptedOperation],
    ) -> Result<(), String> {
        for operation in operations {
            if operation.mutating {
                return Err(format!(
                    "read-only doctor issued a mutating command: {}",
                    operation.rendered()
                ));
            }
        }
        Ok(())
    }

    fn assert_batched_metadata(
        operations: &[scripted_doctor::ScriptedOperation],
    ) -> Result<(), String> {
        let batches = operations
            .iter()
            .filter(|operation| operation.phase == ScriptedPhase::MetadataBatch)
            .collect::<Vec<_>>();
        if batches.is_empty() {
            return Err("no metadata batch was executed".to_owned());
        }
        let requested = batches
            .iter()
            .map(|operation| operation.paths.len())
            .sum::<usize>();
        if batches.len() >= requested {
            return Err(format!(
                "metadata was not batched: {} batches for {requested} paths",
                batches.len()
            ));
        }
        Ok(())
    }

    fn assert_no_unexpected_operations(
        operations: &[scripted_doctor::ScriptedOperation],
    ) -> Result<(), String> {
        for operation in operations {
            if operation.phase == ScriptedPhase::Unexpected {
                return Err(format!("unexpected launch: {}", operation.rendered()));
            }
        }
        Ok(())
    }

    fn assert_phase_absent(
        operations: &[scripted_doctor::ScriptedOperation],
        phase: ScriptedPhase,
    ) -> Result<(), String> {
        if let Some(operation) = operations.iter().find(|operation| operation.phase == phase) {
            return Err(format!(
                "phase {} ran after it was forbidden: {}",
                phase.label(),
                operation.rendered()
            ));
        }
        Ok(())
    }

    /// Generous bound for a fixture that hangs for 300s. It must still be far
    /// below the sleep it would otherwise wait out, so the bounded probe is
    /// proven without depending on exact host spawn latency.
    const LIVENESS_DEADLINE: Duration = Duration::from_secs(10);
    const LIVENESS_UPPER_BOUND: Duration = Duration::from_secs(30);

    /// Phase-reach budget for the ownership metadata hang proof. Preflight
    /// (Colima `status`, `compose ps`, the BUN probe, and identity) shares this
    /// monotonic deadline, so it must comfortably exceed worst-case preflight
    /// under a loaded host while staying far below the fixture's 300s hang.
    /// The old tight 2s budget let a slow earlier probe expire first, so the
    /// proof judged a preflight timeout instead of the metadata child it owns.
    const OWNERSHIP_HANG_DEADLINE: Duration = Duration::from_secs(10);
    const OWNERSHIP_HANG_UPPER_BOUND: Duration = Duration::from_secs(30);

    #[cfg(unix)]
    fn pid_alive(pid: i32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
    }

    /// The exact PIDs the fixture recorded for the phase it is hanging: the
    /// shell leader plus its `sleep` descendant. This is the readiness proof
    /// that the intended phase spawned; a missing or empty record means an
    /// earlier phase consumed the shared deadline and starved the target, so
    /// any reap judgement would be vacuous.
    #[cfg(unix)]
    fn recorded_owned_pids(pgfile: &Path) -> Vec<i32> {
        let text = fs::read_to_string(pgfile).unwrap_or_else(|error| {
            panic!(
                "fixture never recorded its own pids ({error}): the intended child never spawned"
            )
        });
        let pids = text
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .collect::<Vec<_>>();
        assert!(
            !pids.is_empty(),
            "fixture must record its own process group: {text:?}"
        );
        pids
    }

    /// Reap oracle: every PID the fixture recorded must be gone. The positive
    /// proof expects this to hold after the runtime's deadline reap; the
    /// negative control expects it to fail while timeout/reap is disabled.
    #[cfg(unix)]
    fn assert_owned_processes_gone(pgfile: &Path) {
        let alive = recorded_owned_pids(pgfile)
            .into_iter()
            .filter(|pid| pid_alive(*pid))
            .collect::<Vec<_>>();
        assert!(
            alive.is_empty(),
            "recorded probe process group was not reaped: {alive:?} still alive"
        );
    }

    #[cfg(unix)]
    fn wait_for_processes_gone(pgfile: &Path) {
        let pids = recorded_owned_pids(pgfile);
        let deadline = Instant::now() + Duration::from_secs(10);
        while pids.iter().any(|pid| pid_alive(*pid)) {
            assert!(
                Instant::now() < deadline,
                "recorded probe process group was not reaped: {pids:?} still alive"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Private RAII cleanup for a fixture the test started itself. Only the
    /// exact PIDs the fixture recorded are signalled, plus its own direct
    /// `Child` handle: never a process pattern, never the test's own process
    /// group. Waiting on the direct child reaps its zombie so the recorded
    /// leader is genuinely gone rather than merely signalled.
    #[cfg(unix)]
    struct OwnedFixtureGuard {
        pgfile: PathBuf,
        child: std::process::Child,
    }

    #[cfg(unix)]
    impl OwnedFixtureGuard {
        fn new(pgfile: PathBuf, child: std::process::Child) -> Self {
            Self { pgfile, child }
        }

        fn wait_until_recorded(&self) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if fs::read_to_string(&self.pgfile)
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(false)
                {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "fixture never recorded its own pids: {}",
                    self.pgfile.display()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn reap(&mut self) {
            let pids = fs::read_to_string(&self.pgfile)
                .ok()
                .map(|text| {
                    text.lines()
                        .filter_map(|line| line.trim().parse::<i32>().ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let signal = |pids: &[i32], signal| {
                for pid in pids {
                    let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(*pid), signal);
                }
            };
            signal(&pids, nix::sys::signal::Signal::SIGTERM);
            let grace = Instant::now() + Duration::from_secs(2);
            while pids.iter().any(|pid| pid_alive(*pid)) && Instant::now() < grace {
                std::thread::sleep(Duration::from_millis(20));
            }
            if pids.iter().any(|pid| pid_alive(*pid)) {
                signal(&pids, nix::sys::signal::Signal::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
            let deadline = Instant::now() + Duration::from_secs(10);
            while pids.iter().any(|pid| pid_alive(*pid)) {
                assert!(
                    Instant::now() < deadline,
                    "private guard could not reap recorded fixture pids: {pids:?}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    #[cfg(unix)]
    impl Drop for OwnedFixtureGuard {
        fn drop(&mut self) {
            self.reap();
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
        let policy = policy_for(&root);
        let _env = with_scripted_backend_env();

        let running_guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture::default(),
        ));
        let running = is_primary_service_running_with_deadline(
            &root,
            &policy,
            Some(running_guard.deadline(LIVENESS_DEADLINE)),
        )
        .expect("bounded successful liveness probe");
        assert!(running, "running primary service must read as live");
        let running_operations = running_guard.runtime().operations();
        assert_eq!(
            running_operations
                .iter()
                .map(|operation| operation.phase)
                .collect::<Vec<_>>(),
            vec![
                ScriptedPhase::ColimaStatus,
                ScriptedPhase::LivenessComposePs
            ],
            "production order is status then compose ps: {:?}",
            running_operations
        );
        assert!(
            running_guard.runtime().logical_elapsed() < LIVENESS_DEADLINE,
            "production parsing must settle the running distinction inside the deadline"
        );
        drop(running_guard);

        let stopped_guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                status: ScriptedStatus::Stopped,
                ..OwnershipFixture::default()
            },
        ));
        let stopped = is_primary_service_running_with_deadline(
            &root,
            &policy,
            Some(stopped_guard.deadline(LIVENESS_DEADLINE)),
        )
        .expect("bounded stopped liveness probe");
        assert!(!stopped, "stopped Colima must read as not running");
        assert_phase_absent(
            &stopped_guard.runtime().operations(),
            ScriptedPhase::LivenessComposePs,
        )
        .expect("a stopped profile must not run the compose probe");

        // A stopped profile must not reach ownership at all.
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(stopped_guard.deadline(LIVENESS_DEADLINE)),
            &mut diagnostics,
        );
        assert_eq!(
            diagnostics.evidence,
            vec![format!(
                "container `{}` workspace ownership: not probed (primary service stopped)",
                policy.name
            )],
            "stopped must be a distinct not-probed state"
        );
        assert_phase_absent(
            &stopped_guard.runtime().operations(),
            ScriptedPhase::MetadataBatch,
        )
        .expect("stopped must not run the ownership batches");
        assert_phase_absent(
            &stopped_guard.runtime().operations(),
            ScriptedPhase::AccessBatch,
        )
        .expect("stopped must not run the numeric-user batches");
        drop(stopped_guard);

        // A timeout is unavailable, never stopped and never clean.
        let timeout_guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                status: ScriptedStatus::Timeout,
                ..OwnershipFixture::default()
            },
        ));
        let mut timeout_diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(timeout_guard.deadline(LIVENESS_DEADLINE)),
            &mut timeout_diagnostics,
        );
        let warnings = format!("{:?}", timeout_diagnostics.warnings);
        assert!(
            timeout_diagnostics.findings.is_empty(),
            "a timed-out liveness probe must never produce a finding: {warnings}"
        );
        assert!(
            timeout_diagnostics.warnings.iter().any(|warning| warning
                .contains("workspace ownership probe skipped")
                && warning.contains("timed out")),
            "timeout must be reported as unavailable: {warnings}"
        );
        assert!(
            !timeout_diagnostics
                .evidence
                .iter()
                .any(|line| line.contains("clean") || line.contains("not probed")),
            "timeout must never read as clean or stopped: {:?}",
            timeout_diagnostics.evidence
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
    #[test]
    fn healthy_named_volume_ownership_batches_probes_within_doctor_budget() {
        // Keep this aligned with effigy-doctor's existing fast-doctor budget.
        const DOCTOR_BUDGET_MS: u64 = 10_000;

        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-latency");
        let policy = named_rust_volume_policy(&root);
        let budget = Duration::from_millis(DOCTOR_BUDGET_MS);
        let _env = with_scripted_backend_env();
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture::default(),
        ));
        let mut diagnostics = DoctorRuntimeDiagnostics::default();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );

        let runtime = guard.runtime();
        let executed = runtime.applicable_operations();
        let model = executed
            .iter()
            .map(|operation| operation.cost)
            .sum::<Duration>();
        assert_eq!(
            runtime.logical_elapsed(),
            model + SCRIPTED_OPERATION_COST,
            "the model charges every executed script operation, including the preliminary status"
        );
        assert!(
            model < budget,
            "the batched model must fit the existing doctor budget; model={model:?}, \
             executed={} operations={:?}, warnings={:?}",
            executed.len(),
            executed
                .iter()
                .map(|operation| operation.phase.label())
                .collect::<Vec<_>>(),
            diagnostics.warnings
        );
        assert!(runtime.logical_elapsed() > Duration::ZERO);
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
        assert_no_unexpected_operations(&executed).expect("no unexpected launch");
        assert_no_mutating_operations(&executed).expect("read-only diagnosis");
        assert_batched_metadata(&executed).expect("metadata must stay batched");

        assert_eq!(
            executed.len(),
            10,
            "the applicable trace is the pre-batching workload shape: {:?}",
            executed
                .iter()
                .map(|operation| operation.phase.label())
                .collect::<Vec<_>>()
        );

        let paths = runtime.requested_metadata_paths();
        for path in [
            "/usr/local/cargo",
            "/usr/local/cargo/registry/src",
            "/usr/local/cargo/git/checkouts",
            "/workspace/target",
            "/workspace/target/debug",
            "/workspace/target/debug/.cargo-build-lock",
        ] {
            assert!(
                paths.iter().any(|sample| sample == path),
                "expected exact sampled path {path}; samples were {paths:?}"
            );
        }
        assert_eq!(paths.len(), 8, "known paths are sampled once each");

        let access = runtime.operations_for(ScriptedPhase::AccessBatch);
        assert_eq!(access.len(), 3, "access checks run in three batches");
        assert_numeric_access_user(&access, "501:20")
            .expect("numeric-user batches must use the resolved uid/gid");

        // Serial reference over the SAME workload, then the model budget. The
        // executed batched trace fits; the pre-batching shape does not.
        let serial = serial_reference_operations(&policy);
        assert!(
            serial >= 24,
            "the pre-batching serial reference must stay a real lower bound: {serial}"
        );
        assert!(
            SCRIPTED_OPERATION_COST * (executed.len() as u32) < budget,
            "batched model {} * {}ms must fit {budget:?}",
            executed.len(),
            SCRIPTED_OPERATION_COST.as_millis()
        );
        assert!(
            SCRIPTED_OPERATION_COST * (serial as u32) >= budget,
            "serial model {serial} * {}ms must exceed {budget:?}",
            SCRIPTED_OPERATION_COST.as_millis()
        );
        println!(
            "ownership latency model: {} batched calls vs {serial} serial calls at {}ms each \
             (model, not real performance); trace={:?}",
            executed.len(),
            SCRIPTED_OPERATION_COST.as_millis(),
            executed
                .iter()
                .map(|operation| operation.phase.label())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn named_volume_batch_keeps_wrong_permissions_as_path_specific_finding() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-wrong-permissions");
        let bad_path = "/workspace/target/debug/.cargo-build-lock";
        let policy = named_rust_volume_policy(&root);
        let budget = Duration::from_secs(8);
        let _env = with_scripted_backend_env();
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                bad_path: Some(bad_path.to_owned()),
                ..OwnershipFixture::default()
            },
        ));
        let mut diagnostics = DoctorRuntimeDiagnostics::default();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );

        let runtime = guard.runtime();
        let executed = runtime.applicable_operations();
        assert!(
            runtime.logical_elapsed() < budget,
            "the wrong-permission workload must settle inside the existing budget: {:?}",
            executed
                .iter()
                .map(|operation| operation.phase.label())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            diagnostics.findings.len(),
            1,
            "wrong ownership remains a finding: {:?}",
            diagnostics.findings
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
            runtime
                .requested_metadata_paths()
                .iter()
                .any(|path| path == bad_path),
            "the bad path must be sampled, not dropped: {:?}",
            runtime.requested_metadata_paths()
        );
        assert!(
            runtime
                .requested_access_paths()
                .iter()
                .any(|path| path == bad_path),
            "the bad path must receive a numeric-user access check"
        );
        assert_numeric_access_user(&executed, "501:20")
            .expect("read/write checks must use the resolved uid and gid");
        assert_no_mutating_operations(&executed).expect("read-only diagnosis");
        assert_no_unexpected_operations(&executed).expect("no unexpected launch");
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
        let budget = Duration::from_secs(8);
        let _env = with_scripted_backend_env();
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                bad_path: Some(bad_path.to_owned()),
                ..OwnershipFixture::default()
            },
        ));
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );

        let runtime = guard.runtime();
        let executed = runtime.applicable_operations();
        assert!(
            runtime.logical_elapsed() < budget,
            "verify-only workload must settle inside the existing budget"
        );
        assert_eq!(
            diagnostics.findings.len(),
            1,
            "verify-only wrong ownership remains a finding: {:?}",
            diagnostics.findings
        );
        assert!(diagnostics.findings[0].evidence.contains(bad_path));
        let samples = runtime.requested_metadata_paths();
        assert!(
            samples
                .iter()
                .any(|sample| sample == "/usr/local/cargo/git/checkouts"),
            "the shared Cargo mount must retain its known nested sample: {samples:?}"
        );
        assert_no_mutating_operations(&executed)
            .expect("doctor ownership diagnosis must not launch mutating commands");
        assert_no_unexpected_operations(&executed).expect("no unexpected launch");
        assert!(
            diagnostics.findings.iter().all(|finding| !finding.fixable),
            "verify-only findings are not fixable"
        );
    }
    // ------------------------------------------------------------------
    // Fixture contract (papercut 4ee28d2c): the deterministic capture seams,
    // the bounded negative controls that keep an oracle from going vacuous,
    // the bounded real crosscheck of argv/parse against a private fake
    // runtime, and the model deadline controls.
    // ------------------------------------------------------------------

    fn synthetic_operation(
        phase: ScriptedPhase,
        program: &str,
        args: Vec<String>,
        paths: Vec<String>,
        user: Option<String>,
        mutating: bool,
    ) -> scripted_doctor::ScriptedOperation {
        scripted_doctor::ScriptedOperation {
            phase,
            program: program.to_owned(),
            args,
            paths,
            user,
            mutating,
            cost: SCRIPTED_OPERATION_COST,
        }
    }

    fn assert_unavailable_not_clean(
        status: WorkspaceOwnershipProbeStatus,
        warnings: &[String],
        evidence: &[String],
    ) -> Result<(), String> {
        if status != WorkspaceOwnershipProbeStatus::Unavailable {
            return Err(format!("expected unavailable, got {status:?}"));
        }
        if evidence.iter().any(|line| line.contains("clean")) {
            return Err(format!("unavailable must never report clean: {evidence:?}"));
        }
        if !warnings.iter().any(|warning| warning.contains("timed out")) {
            return Err(format!("unavailable must name its timeout: {warnings:?}"));
        }
        Ok(())
    }

    fn assert_single_actionable_finding(
        diagnostics: &DoctorRuntimeDiagnostics,
        bad_path: &str,
    ) -> Result<(), String> {
        if diagnostics.findings.len() != 1 {
            return Err(format!(
                "expected exactly one actionable finding, got {:?}",
                diagnostics.findings
            ));
        }
        let evidence = &diagnostics.findings[0].evidence;
        if !evidence.contains(&format!("{bad_path}\tunwritable-by-uid-501")) {
            return Err(format!(
                "finding must carry the numeric-user failure for {bad_path}: {evidence}"
            ));
        }
        if !evidence.contains(&format!("{bad_path}\t{bad_path}")) {
            return Err(format!(
                "finding must carry path-specific owner evidence: {evidence}"
            ));
        }
        Ok(())
    }

    /// One owned Rust target so the bounded real crosscheck reaches exactly one
    /// metadata batch and one access batch after the identity probe.
    fn crosscheck_rust_volume_policy(root: &Path) -> EffectiveContainerPolicy {
        let compose = root.join("docker-compose.yml");
        fs::write(
            &compose,
            r#"
services:
  workspace:
    volumes:
      - cargo-git:/usr/local/cargo/git
volumes:
  cargo-git:
"#,
        )
        .expect("write crosscheck volume fixture");
        let mut policy = policy_for(root);
        policy.compose_files = vec![compose];
        policy
    }

    /// Private fake runtime for the bounded crosscheck. It answers
    /// identity/metadata/access for real and appends a canonical summary of the
    /// received argv (`phase|user|paths`) so the real launches can be compared
    /// to the scripted trace for the SAME workload.
    fn install_crosscheck_colima(root: &Path, argv_log: &Path) -> PathBuf {
        let script = format!(
            r#"#!/bin/sh
log='{argv_log}'
case "$1" in
  status)
    printf 'status: Running\n'
    exit 0
    ;;
  nerdctl)
    user=''
    prev=''
    for arg do
      if [ "$prev" = '-u' ]; then user="$arg"; fi
      prev="$arg"
    done
    case "$*" in
      *effigy-workspace-identity*)
        printf 'identity|%s\n' "$user" >> "$log"
        printf '501\n20\n'
        exit 0
        ;;
      *effigy-workspace-doctor-inspect-batch*)
        paths=''
        record=0
        for arg do
          if [ "$arg" = 'effigy-workspace-doctor-inspect-batch' ]; then
            record=1
            continue
          fi
          if [ "$record" -eq 1 ]; then
            if [ -z "$paths" ]; then paths="$arg"; else paths="$paths,$arg"; fi
            printf 'dir 501 20 755\n'
          fi
        done
        printf 'metadata|%s|%s\n' "$user" "$paths" >> "$log"
        exit 0
        ;;
      *effigy-workspace-doctor-access-batch*)
        paths=''
        record=0
        for arg do
          if [ "$arg" = 'effigy-workspace-doctor-access-batch' ]; then
            record=1
            continue
          fi
          if [ "$record" -eq 1 ]; then
            if [ -z "$paths" ]; then paths="$arg"; else paths="$paths,$arg"; fi
            printf 'read-write\n'
          fi
        done
        printf 'access|%s|%s\n' "$user" "$paths" >> "$log"
        exit 0
        ;;
      *"ps"*)
        printf 'demo-stack-1\tUp 2 minutes\t\tdemo-stack\t{root}\tworkspace\t0\n'
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
            argv_log = argv_log.display(),
            root = root.display(),
        );
        install_fake_colima(root, &script)
    }

    fn scripted_summary(operation: &scripted_doctor::ScriptedOperation) -> String {
        let user = operation.user.as_deref().unwrap_or("");
        match operation.phase {
            ScriptedPhase::Identity => format!("identity|{user}"),
            ScriptedPhase::MetadataBatch => {
                format!("metadata|{user}|{}", operation.paths.join(","))
            }
            ScriptedPhase::AccessBatch => {
                format!("access|{user}|{}", operation.paths.join(","))
            }
            other => format!("{}|{user}", other.label()),
        }
    }

    #[test]
    fn fixture_contract_negative_controls_reject_vacuous_fixtures() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("fixture-contract-negative");
        let policy = named_rust_volume_policy(&root);
        let budget = Duration::from_secs(10);
        let bad_path = "/workspace/target/debug/.cargo-build-lock";
        let _env = with_scripted_backend_env();

        // Root access: if the access batch ran as uid 0 the trace oracle must
        // reject it even though the workload otherwise succeeds.
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture::default(),
        ));
        guard
            .runtime()
            .override_recorded_user(ScriptedPhase::AccessBatch, "0");
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );
        let access = guard.runtime().operations_for(ScriptedPhase::AccessBatch);
        assert!(
            access
                .iter()
                .all(|operation| operation.user.as_deref() == Some("0")),
            "fixture switch must expose the forced root access: {access:?}"
        );
        assert!(
            assert_numeric_access_user(&access, "501:20").is_err(),
            "the numeric-user oracle must reject an access batch that ran as root"
        );
        drop(guard);

        // Dropped bad sample: answering a declared-bad path as missing loses the
        // finding, and the finding oracle must reject that.
        let mut fixture = OwnershipFixture {
            bad_path: Some(bad_path.to_owned()),
            ..OwnershipFixture::default()
        };
        fixture
            .path_records
            .insert(bad_path.to_owned(), "missing".to_owned());
        let guard = scripted_doctor::install(scripted_ownership_handler(&policy, fixture));
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );
        assert!(
            diagnostics.findings.is_empty(),
            "dropping the bad sample would hide the finding: {:?}",
            diagnostics.findings
        );
        assert!(
            assert_single_actionable_finding(&diagnostics, bad_path).is_err(),
            "the finding oracle must reject a clean result for a declared-bad fixture"
        );
        drop(guard);

        // All read-write: keeping the owner mismatch but dropping the
        // numeric-user failure must not satisfy the actionable-finding oracle.
        let mut fixture = OwnershipFixture {
            bad_path: Some(bad_path.to_owned()),
            ..OwnershipFixture::default()
        };
        fixture
            .access_records
            .insert(bad_path.to_owned(), "read-write".to_owned());
        let guard = scripted_doctor::install(scripted_ownership_handler(&policy, fixture));
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(guard.deadline(budget)),
            &mut diagnostics,
        );
        assert_eq!(diagnostics.findings.len(), 1);
        assert!(
            !diagnostics.findings[0].evidence.contains("unwritable"),
            "fixture switch must drop the numeric-user evidence: {:?}",
            diagnostics.findings
        );
        assert!(
            assert_single_actionable_finding(&diagnostics, bad_path).is_err(),
            "the actionable-finding oracle must reject a missing access failure"
        );
        drop(guard);

        // Protected scope mutation: any mutating command in the read-only
        // doctor trace must be rejected.
        let mutating = vec![synthetic_operation(
            ScriptedPhase::MetadataBatch,
            "colima",
            vec!["nerdctl".to_owned(), "chown".to_owned()],
            vec!["/usr/local/cargo".to_owned()],
            Some("0".to_owned()),
            true,
        )];
        assert!(
            assert_no_mutating_operations(&mutating).is_err(),
            "the read-only oracle must reject a mutating command"
        );

        // Per-path batch regression: a metadata op per path must be rejected so
        // the batching claim cannot silently regress.
        let per_path = vec![
            synthetic_operation(
                ScriptedPhase::MetadataBatch,
                "colima",
                vec!["nerdctl".to_owned()],
                vec!["/a".to_owned()],
                Some("0".to_owned()),
                false,
            ),
            synthetic_operation(
                ScriptedPhase::MetadataBatch,
                "colima",
                vec!["nerdctl".to_owned()],
                vec!["/b".to_owned()],
                Some("0".to_owned()),
                false,
            ),
        ];
        assert!(
            assert_batched_metadata(&per_path).is_err(),
            "the batching oracle must reject one metadata op per path"
        );

        // Stopped still compose: a compose probe after a stopped status must be
        // rejected.
        let stopped_trace = vec![synthetic_operation(
            ScriptedPhase::LivenessComposePs,
            "colima",
            vec!["nerdctl".to_owned()],
            Vec::new(),
            None,
            false,
        )];
        assert!(
            assert_phase_absent(&stopped_trace, ScriptedPhase::LivenessComposePs).is_err(),
            "the stopped oracle must reject a compose probe after stopped"
        );

        // Timeout as clean: an oracle must never accept clean evidence for an
        // unavailable timeout.
        let timeout_as_clean = assert_unavailable_not_clean(
            WorkspaceOwnershipProbeStatus::Clean,
            &["workspace ownership probe skipped: colima status timed out".to_owned()],
            &["container `stack` workspace ownership: clean".to_owned()],
        );
        assert!(
            timeout_as_clean.is_err(),
            "the unavailable oracle must reject clean evidence after a timeout"
        );
    }

    #[test]
    fn fixture_contract_deadline_controls_are_model_clocked() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("fixture-contract-deadline");
        let policy = named_rust_volume_policy(&root);
        let _env = with_scripted_backend_env();

        // Already-expired model deadline: the liveness probe never executes and
        // is unavailable, never stopped and never clean.
        let expired_guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture::default(),
        ));
        let mut expired_diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(expired_guard.deadline(Duration::ZERO)),
            &mut expired_diagnostics,
        );
        assert!(
            expired_guard.runtime().operations().is_empty(),
            "an expired deadline must never execute a scripted operation: {:?}",
            expired_guard.runtime().operations()
        );
        assert_unavailable_not_clean(
            WorkspaceOwnershipProbeStatus::Unavailable,
            &expired_diagnostics.warnings,
            &expired_diagnostics.evidence,
        )
        .expect("expired model deadline must be unavailable, not clean");
        assert!(
            !expired_diagnostics
                .evidence
                .iter()
                .any(|line| line.contains("not probed")),
            "expired must not read as stopped: {:?}",
            expired_diagnostics.evidence
        );
        drop(expired_guard);

        // A model deadline that expires mid-workload: the earlier phases run,
        // the following phase times out, and no clean claim survives.
        let mid_guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture::default(),
        ));
        let mut mid_diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(mid_guard.deadline(SCRIPTED_OPERATION_COST + SCRIPTED_OPERATION_COST)),
            &mut mid_diagnostics,
        );
        let recorded = mid_guard.runtime().operations();
        assert!(
            !recorded.is_empty(),
            "the first scripted operations must fit the mid-workload model budget"
        );
        assert!(
            mid_diagnostics.findings.is_empty(),
            "a starved workload must not report a finding: {:?}",
            mid_diagnostics.findings
        );
        assert!(
            mid_diagnostics
                .warnings
                .iter()
                .any(|warning| warning.contains("timed out")),
            "a starved workload must report its timeout: {:?}",
            mid_diagnostics.warnings
        );
        assert!(
            !mid_diagnostics
                .evidence
                .iter()
                .any(|line| line.contains("clean")),
            "a starved workload must never claim clean ownership: {:?}",
            mid_diagnostics.evidence
        );
        assert_phase_absent(&recorded, ScriptedPhase::MetadataBatch)
            .expect("the mid-workload deadline must starve the metadata batch");
    }

    #[test]
    fn fixture_contract_real_crosscheck_matches_scripted_trace() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("fixture-contract-crosscheck");
        let policy = crosscheck_rust_volume_policy(&root);
        let budget = Duration::from_secs(10);
        let fixture = OwnershipFixture::default();
        let argv_log = root.join("crosscheck-argv.log");

        // Scripted trace for the workload.
        crate::runner::exec_command::clear_service_container_name_cache();
        let _env = with_scripted_backend_env();
        let scripted_guard =
            scripted_doctor::install(scripted_ownership_handler(&policy, fixture.clone()));
        let mut scripted_diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(scripted_guard.deadline(budget)),
            &mut scripted_diagnostics,
        );
        let scripted_trace = scripted_guard.runtime().applicable_operations();
        drop(scripted_guard);

        // Same workload, with identity/metadata/access passed through to the
        // private fake runtime; liveness, service resolve and Bun stay scripted.
        crate::runner::exec_command::clear_service_container_name_cache();
        let bin = install_crosscheck_colima(&root, &argv_log);
        let _real_env = with_runtime_env(&bin);
        let real_guard = scripted_doctor::install(scripted_ownership_handler(&policy, fixture));
        real_guard.passthrough([
            ScriptedPhase::Identity,
            ScriptedPhase::MetadataBatch,
            ScriptedPhase::AccessBatch,
        ]);
        let mut real_diagnostics = DoctorRuntimeDiagnostics::default();
        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(real_guard.deadline(budget)),
            &mut real_diagnostics,
        );
        let real_trace = real_guard.runtime().applicable_operations();
        let real_summary = fs::read_to_string(&argv_log).expect("crosscheck argv log");

        let expected = scripted_trace
            .iter()
            .filter(|operation| {
                matches!(
                    operation.phase,
                    ScriptedPhase::Identity
                        | ScriptedPhase::MetadataBatch
                        | ScriptedPhase::AccessBatch
                )
            })
            .map(scripted_summary)
            .collect::<Vec<_>>();
        let observed = real_summary
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();

        let context = || {
            format!(
                "resolved executable={}, scripted phases={:?}, real phases={:?}, \
                 remaining model budget={:?}, argv log={real_summary:?}, \
                 findings={:?}, warnings={:?}, evidence={:?}",
                bin.join("colima").display(),
                scripted_trace
                    .iter()
                    .map(|operation| operation.phase.label())
                    .collect::<Vec<_>>(),
                real_trace
                    .iter()
                    .map(|operation| operation.phase.label())
                    .collect::<Vec<_>>(),
                budget.saturating_sub(real_guard.runtime().logical_elapsed()),
                real_diagnostics.findings,
                real_diagnostics.warnings,
                real_diagnostics.evidence,
            )
        };

        assert_eq!(
            observed.len(),
            expected.len(),
            "real launches must match the scripted passthrough count; {}",
            context()
        );
        assert_eq!(
            observed,
            expected,
            "real argv summaries must match the scripted trace for the same workload; {}",
            context()
        );
        assert!(
            observed.len() <= 6,
            "the real crosscheck must stay bounded to a handful of launches; {}",
            context()
        );
        assert_eq!(
            real_diagnostics.findings,
            scripted_diagnostics.findings,
            "parsed findings must match the scripted workload; {}",
            context()
        );
        assert_eq!(
            real_diagnostics.warnings,
            scripted_diagnostics.warnings,
            "parsed warnings must match the scripted workload; {}",
            context()
        );
        assert_eq!(
            real_diagnostics.evidence,
            scripted_diagnostics.evidence,
            "parsed evidence must match the scripted workload; {}",
            context()
        );
        assert_no_mutating_operations(&real_trace).expect("read-only crosscheck");
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
            Some(started + OWNERSHIP_HANG_DEADLINE),
            &mut diagnostics,
        );

        // Readiness before judgement: the fixture records its own leader and
        // descendant when the metadata batch starts. An empty record means an
        // earlier phase consumed the deadline and this proof never reached the
        // child it owns, so the reap result below would be vacuous.
        let recorded = recorded_owned_pids(&process_group);
        assert!(
            recorded.len() >= 2,
            "the intended metadata batch recorded its leader and descendant: {recorded:?}"
        );

        assert!(
            started.elapsed() < OWNERSHIP_HANG_UPPER_BOUND,
            "the metadata batch must die at the deadline, not wait out sleep 300"
        );
        assert!(diagnostics.findings.is_empty());
        assert!(
            diagnostics
                .warnings
                .iter()
                .any(|warning| warning.contains("verification incomplete")
                    && warning.contains("workspace ownership metadata batch")
                    && warning.contains("/usr/local/cargo")
                    && warning.contains("timed out")),
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
        assert_owned_processes_gone(&process_group);
    }

    /// Controlled earlier-phase delay. The BUN preflight hangs and consumes the
    /// shared deadline, so the metadata batch is never spawned. This is exactly
    /// the shape that failed milestone 4dee6cf6 under host load with the old
    /// tight budget: the doctor correctly reported the earlier probe as
    /// unavailable, but the old oracle demanded a metadata-batch context. The
    /// doomed metadata assertion is deliberately absent here; instead the
    /// proof requires the preflight's own context and forbids any metadata
    /// reap claim.
    #[cfg(unix)]
    #[test]
    fn ownership_preflight_hang_is_unavailable_without_metadata_reap_claim() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-preflight-hang");
        let policy = named_rust_volume_policy(&root);
        let preflight_group = root.join("preflight-process-group");
        let metadata_group = root.join("ownership-process-group");
        let bin = install_fake_ownership_colima(&root, "0", "hang-preflight", "", &preflight_group);
        let _env = with_runtime_env(&bin);
        let mut diagnostics = DoctorRuntimeDiagnostics::default();
        let started = Instant::now();

        append_workspace_ownership_diagnostics(
            &root,
            std::slice::from_ref(&policy),
            Some(started + OWNERSHIP_HANG_DEADLINE),
            &mut diagnostics,
        );

        // The hung earlier phase is reaped and reported with its own context;
        // the metadata child is never spawned, so no metadata reap is claimed.
        wait_for_processes_gone(&preflight_group);
        assert!(diagnostics.findings.is_empty());
        assert!(
            diagnostics.warnings.iter().any(|warning| warning
                .contains("workspace ownership probe skipped")
                && warning.contains("BUN_INSTALL")
                && warning.contains("timed out")),
            "the hung preflight probe must report its own context: {:?}",
            diagnostics.warnings
        );
        assert!(
            diagnostics
                .warnings
                .iter()
                .all(|warning| !warning.contains("workspace ownership metadata batch")),
            "a starved metadata batch must not be claimed as exercised: {:?}",
            diagnostics.warnings
        );
        assert!(
            !metadata_group.exists(),
            "no metadata child may be spawned once the preflight deadline expired"
        );
        assert!(
            diagnostics
                .evidence
                .iter()
                .all(|line| !line.contains("clean")),
            "a preflight timeout must not claim clean ownership"
        );
    }

    /// Negative control: with the runtime deadline/reap disabled, the owned
    /// metadata hang stays alive, so the reap oracle used by the positive proof
    /// must fail. The private guard then reaps exactly the recorded leader and
    /// descendant, leaving no leaked process behind.
    #[cfg(unix)]
    #[test]
    fn ownership_hang_reap_oracle_fails_when_runtime_reap_is_disabled() {
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-hang-negative-control");
        let process_group = root.join("ownership-process-group");
        let bin = install_fake_ownership_colima(&root, "0", "hang", "", &process_group);
        let fixture = bin.join("colima");
        let child = std::process::Command::new(&fixture)
            .args([
                "nerdctl",
                "--profile",
                "effigy",
                "--",
                "exec",
                "-u",
                "0",
                "demo-stack-1",
                "sh",
                "-c",
                "effigy-workspace-doctor-inspect-batch /usr/local/cargo",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the metadata hang fixture without runtime reap");

        let guard = OwnedFixtureGuard::new(process_group.clone(), child);
        guard.wait_until_recorded();
        let recorded = recorded_owned_pids(&process_group);
        assert!(
            recorded.iter().all(|pid| pid_alive(*pid)),
            "timeout/reap disabled: the recorded owned hang must still be alive: {recorded:?}"
        );

        let oracle = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_owned_processes_gone(&process_group);
        }));
        assert!(
            oracle.is_err(),
            "the reap oracle must fail while the owned hang is still alive"
        );

        drop(guard);
        assert_owned_processes_gone(&process_group);
    }
}
