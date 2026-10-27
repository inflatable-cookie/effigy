//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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

use crate::runner::container_command::is_primary_service_running_with_deadline;
use crate::runner::deferral;
use crate::runner::error::RunnerError;
#[cfg(not(test))]
use crate::runner::exec_command::{capture_command_until, CommandCaptureError};
use crate::runner::exec_command::{run_command_capture_until, run_compose_exec_plan_with_deadline};
use crate::runner::execute::api;
use crate::runner::workspace_command::permissions::{
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
    let deadline = Instant::now() + budget;
    let executable = std::env::current_exe().map_err(|error| {
        DoctorError::task_invocation(format!("failed to resolve Effigy executable: {error}"))
    })?;
    let args: Vec<OsString> = std::iter::once(&invocation.name)
        .chain(&invocation.args)
        .map(OsString::from)
        .collect();
    let output = capture_command_until(cwd, executable.as_os_str(), &args, None, Some(deadline))
        .map_err(|error| match error {
            CommandCaptureError::Deadline { .. } => DoctorError::BudgetExhausted {
                phase: "health_task".to_owned(),
            },
            CommandCaptureError::Runner(error) => runner_to_doctor(error),
        })?;
    let status = output.status;
    let stdout = output.stdout;
    let stderr = output.stderr;
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

    // TCP alias listener health is machine-global too: a duplicate bind
    // tuple or a missing mapped-port fallback hangs the alias on any repo,
    // so it also precedes the container-policy early returns.
    append_gateway_tcp_alias_diagnostics(&mut diagnostics);

    // v0.14 task state is ignored, never imported or deleted (contract 052
    // §11); listing it tells the operator what is safe to remove.
    append_legacy_task_state_diagnostics(resolved_root, &mut diagnostics);

    // The user-configured scratch root is machine-global: report whether it is
    // in force, or why Effigy fell back to the default temp directory.
    append_scratch_diagnostics(&mut diagnostics);

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

    // Each environment's exact backend and why, and its record (contract 053).
    for policy in &policies {
        let view = effigy_containers::environment::EnvironmentView::read(resolved_root, policy);
        for line in view.lines() {
            diagnostics
                .evidence
                .push(format!("environment `{}` {line}", policy.name));
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
            "container `{container_name}` has rust build/cache, managed workspace, or dependency volume (vendor/node_modules) paths that the declared workspace user {identity} cannot use: {}",
            samples.join(", ")
        ),
        remediation: "Headless workspace, primary-service exec, and routed tasks repair owned disposable rust/cargo/target named volumes before children launch. If doctor still reports this, rerun `effigy doctor --verbose`. Do not recursively chown host source, sibling, or shared binds; isolate `target` with catalog `isolated_dirs` when the path is a host bind.".to_owned(),
        fixable: false,
    }
}

/// Lists each v0.14 task-state family still on disk, with what is safe to do
/// with it. Informational: nothing reads it for exclusion.
fn append_legacy_task_state_diagnostics(
    resolved_root: &Path,
    diagnostics: &mut DoctorRuntimeDiagnostics,
) {
    for family in crate::runner::task_engine::legacy_families(resolved_root) {
        diagnostics.findings.push(effigy_doctor::DoctorFinding {
            check_id: format!("tasks.legacy-state.{}", family.family),
            severity: effigy_doctor::DoctorSeverity::Info,
            evidence: format!(
                "{} entr{} of v0.14 task state at {}",
                family.entries,
                if family.entries == 1 { "y" } else { "ies" },
                family.path.display()
            ),
            remediation: format!(
                "{}. Effigy v0.15 never reads it and never deletes it",
                family.disposition
            ),
            fixable: false,
        });
    }
}

/// Report the user-configured per-run scratch root: what runs under it, or
/// why Effigy fell back to the default temp directory. The reason is an
/// observation, never a failure.
fn append_scratch_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    use crate::runner::scratch;
    match scratch::status() {
        scratch::Status::Unconfigured => {}
        scratch::Status::Admitted { root } => diagnostics.evidence.push(format!(
            "scratch: host-placed runs use per-run directories under {}",
            root.display()
        )),
        scratch::Status::Refused { root, reason } => {
            let where_ = if root.as_os_str().is_empty() {
                String::new()
            } else {
                format!("`{}` was refused: ", root.display())
            };
            diagnostics.warnings.push(format!(
                "scratch: {where_}{reason}; runs use the default temp directory"
            ));
        }
    }
}

/// Surface gateway TCP alias listeners that can never serve traffic as
/// doctor runtime warnings: duplicate bind tuples with divergent mapped-port
/// fallbacks (only one alias wins the listener) and alias routes with a
/// missing or invalid fallback (the manager starts no listener at all). Both
/// name the alias and the fallback so an operator sees the hang cause
/// without reading daemon logs. A missing or unreadable table simply yields
/// no alias findings here.
fn append_gateway_tcp_alias_diagnostics(diagnostics: &mut DoctorRuntimeDiagnostics) {
    use effigy_gateway::routes::RouteTable;
    use effigy_gateway::server::GatewayConfig;

    let Ok(gateway_dir) = crate::runner::gateway_command::gateway_dir() else {
        return;
    };
    let route_table_path = GatewayConfig::standard(gateway_dir).route_table_path;
    let Ok(route_table) = RouteTable::load(&route_table_path) else {
        return;
    };
    for conflict in &crate::runner::gateway_command::gateway_tcp_bind_conflicts(&route_table) {
        let routes = conflict
            .routes
            .iter()
            .map(|route| {
                format!(
                    "`{}` -> {}",
                    route.domain,
                    route.target.as_deref().unwrap_or("<missing>")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.warnings.push(format!(
            "gateway TCP bind {}:{} is claimed by aliases with different mapped-port fallbacks ({}); only one listener wins and the other alias hangs while its mapped port stays ready. Retry `effigy container up` for the environment that owns the alias.",
            conflict.bind_ip, conflict.bind_port, routes,
        ));
    }
    diagnostics.warnings.extend(
        crate::runner::gateway_command::render_gateway_tcp_alias_unbound_lines(
            &crate::runner::gateway_command::gateway_tcp_alias_unbound_routes(&route_table),
        ),
    );
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
    use crate::runner::test_support::effective_container_policy;
    use crate::runner::workspace_command::permissions::{
        compose_backend_with_deadline, diagnose_workspace_ownership, rust_nested_probe_paths,
        WorkspaceOwnershipProbeStatus,
    };

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
    fn fresh_root(label: &str) -> (effigy_test_support::Fixture, PathBuf) {
        let temp = effigy_test_support::Fixture::new(&format!("effigy-doctor-liveness-{label}-"))
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

    /// Generated PHP app + Nginx shape: `app` is the primary workspace writer
    /// of Composer vendor and JS node_modules; `web` mounts them read-only.
    fn dependency_volume_policy(root: &Path, web_mode: &str) -> EffectiveContainerPolicy {
        let compose = root.join("docker-compose.yml");
        fs::write(
            &compose,
            format!(
                r#"
services:
  app:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
      - cbs-node-modules:/var/www/cbs/node_modules
  web:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor:{web_mode}
      - cbs-node-modules:/var/www/cbs/node_modules:{web_mode}
volumes:
  cbs-vendor:
  cbs-node-modules:
"#
            ),
        )
        .expect("write dependency volume fixture");
        let mut policy = policy_for(root);
        policy.primary_service = "app".to_owned();
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
    const LIVENESS_DEADLINE: Duration = Duration::from_secs(1);
    const LIVENESS_UPPER_BOUND: Duration = Duration::from_secs(30);

    /// Phase-reach budget for the ownership metadata hang proof. Preflight
    /// (Colima `status`, `compose ps`, the BUN probe, and identity) shares this
    /// monotonic deadline, so it must comfortably exceed worst-case preflight
    /// under a loaded host while staying far below the fixture's 300s hang.
    /// Three seconds keeps roughly a 6x margin on preflight spawn latency and
    /// still makes the child it owns the deadline's cause.
    const OWNERSHIP_HANG_DEADLINE: Duration = Duration::from_secs(3);
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

    #[test]
    fn dependency_volume_wrong_permissions_stay_a_path_specific_finding() {
        // root0700 app-writer/web-reader fixture: the unwritable dependency
        // volume is probed and reported as an actionable finding, never
        // omitted into a clean bill, and the diagnosis stays read-only.
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-dependency-volume");
        let bad_path = "/var/www/cbs/vendor";
        let policy = dependency_volume_policy(&root, /* web_mode */ "ro");
        let plan = load_workspace_ownership_plan(&policy).expect("ownership plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == bad_path)
            .expect("vendor target");
        assert_eq!(
            vendor.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable,
            "a sole-writer volume with read-only consumers stays repairable"
        );
        let budget = Duration::from_secs(8);
        let _env = with_scripted_backend_env();
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                path_records: [(bad_path.to_owned(), "0 0 700".to_owned())].into(),
                access_records: [(bad_path.to_owned(), "unwritable".to_owned())].into(),
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
        assert_eq!(
            diagnostics.findings.len(),
            1,
            "unwritable dependency volume remains a finding: {:?}",
            diagnostics.findings
        );
        assert!(
            diagnostics.findings[0]
                .evidence
                .contains(&format!("{bad_path}\tunwritable-by-uid-501")),
            "numeric-user failure must identify the exact dependency path: {:?}",
            diagnostics.findings[0]
        );
        assert!(
            runtime
                .requested_metadata_paths()
                .iter()
                .any(|path| path == bad_path),
            "the dependency volume must be sampled, not dropped: {:?}",
            runtime.requested_metadata_paths()
        );
        assert!(
            runtime
                .requested_access_paths()
                .iter()
                .any(|path| path == bad_path),
            "the dependency volume must receive a numeric-user access check"
        );
        assert_numeric_access_user(&executed, "501:20")
            .expect("access checks must use the resolved uid and gid");
        assert_no_mutating_operations(&executed).expect("read-only diagnosis");
        assert_no_unexpected_operations(&executed).expect("no unexpected launch");
    }

    #[test]
    fn shared_dependency_volume_wrong_permissions_stay_reported() {
        // Two writable services: the vendor volume stays forbidden, but an
        // unwritable root must still be probed and reported, not silently
        // omitted into a clean result.
        let _lock = lock_test();
        let (_temp, root) = fresh_root("ownership-shared-dependency-volume");
        let bad_path = "/var/www/cbs/vendor";
        let policy = dependency_volume_policy(&root, /* web_mode */ "rw");
        let plan = load_workspace_ownership_plan(&policy).expect("ownership plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == bad_path)
            .expect("vendor target");
        assert_eq!(
            vendor.repair_authority,
            WorkspaceRepairAuthority::Forbidden,
            "two writable services keep the volume forbidden"
        );
        let budget = Duration::from_secs(8);
        let _env = with_scripted_backend_env();
        let guard = scripted_doctor::install(scripted_ownership_handler(
            &policy,
            OwnershipFixture {
                path_records: [(bad_path.to_owned(), "0 0 700".to_owned())].into(),
                access_records: [(bad_path.to_owned(), "unwritable".to_owned())].into(),
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
        assert_eq!(
            diagnostics.findings.len(),
            1,
            "unwritable shared dependency volume is reported: {:?}",
            diagnostics.findings
        );
        assert!(diagnostics.findings[0].evidence.contains(bad_path));
        assert!(
            runtime
                .requested_metadata_paths()
                .iter()
                .any(|path| path == bad_path),
            "the forbidden dependency volume must still be probed: {:?}",
            runtime.requested_metadata_paths()
        );
        assert_no_mutating_operations(&executed).expect("read-only diagnosis");
        assert_no_unexpected_operations(&executed).expect("no unexpected launch");
    }
    // ------------------------------------------------------------------
    // Fixture contract (papercut 4ee28d2c): the deterministic capture seams,
    // the bounded negative controls that keep an oracle from going vacuous,
    // the bounded real crosscheck of argv/parse against a private fake
    // runtime, and the model deadline controls.
    // ------------------------------------------------------------------
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
        assert!(
            process_group.exists(),
            "metadata fixture did not reach its phase: {diagnostics:?}"
        );
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
        assert!(
            preflight_group.exists(),
            "preflight fixture did not reach its phase: {diagnostics:?}"
        );
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
}
