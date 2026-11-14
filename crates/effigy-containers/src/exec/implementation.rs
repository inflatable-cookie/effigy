//! Container-domain execution helpers extracted from
//! `src/runner/container_command.rs`.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use super::colima_runtime::{
    default_runtime_profile, detect_container_backend,
    run_runtime_command_capture_for_policy_with_absolute_deadline,
    run_runtime_command_capture_for_policy_with_timeout, running_colima_profiles,
};
use super::parse::{
    infer_host_working_dir_from_inspect, parse_running_compose_containers,
    parse_running_container_stats, RunningComposeContainer, RunningComposeContainerProfiled,
    RunningContainerStatsCapture,
};
use super::participation::{
    colima_runtime_participation, docker_runtime_participation, RuntimeParticipation,
};
use super::process::{
    run_command_capture_os, run_command_capture_os_with_env,
    run_command_capture_with_timeout_and_env, run_detached_command_streaming_with_env,
    run_detached_command_with_env,
};

use crate::{
    colima::shutdown_compose_commands, compose::compose_invocation_for_repo, BackendId,
    ContainerBackendDetection, ContainerManager, ContainerManagerError, EffectiveContainerPolicy,
};

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}";
const DOCKER_STATS_FORMAT: &str = "{{ json . }}";
const CONTAINER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
const COMPOSE_PS_TIMEOUT: Duration = Duration::from_secs(30);
/// One bounded retry for the stopped-inclusive project inventory. The read is
/// a pure observation, so a retry cannot duplicate an effect, and a second
/// timeout still refuses: this is not a longer timeout.
const INVENTORY_RETRY_BACKOFF: Duration = Duration::from_millis(500);
/// Bound for stopping and removing one stale owned container a deleted or
/// restarted profile left behind.
const DOCKER_GLOBAL_RUNTIME_LABEL: &str = "docker";

#[derive(Debug)]
pub enum ContainerExecError {
    Launch {
        command: String,
        error: std::io::Error,
    },
    /// An observation refused the evidence it read. No command ended, so the
    /// diagnostic must never borrow a process exit code or claim a signal: it
    /// names the observation and the concrete reason it refused.
    Refusal { command: String, reason: String },
    Failure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl ContainerExecError {
    /// A validation refusal with its concrete cause. Only a real child
    /// termination may carry an exit code or a signal claim.
    fn refused(command: &str, reason: impl Into<String>) -> Self {
        Self::Refusal {
            command: command.to_owned(),
            reason: reason.into(),
        }
    }

    /// The engine's own answer that it is not running, decoded once by the
    /// module that owns its vocabulary ([`super::engine`]). A caller that needs
    /// to know whether a failure means a stopped engine matches this type
    /// instead of reading the engine's text (effigy#933).
    pub fn engine_absence(&self) -> Option<super::engine::EngineAbsence> {
        match self {
            Self::Failure { stdout, stderr, .. } => {
                super::engine::EngineAbsence::observed_in(stdout, stderr)
            }
            Self::Launch { .. } | Self::Refusal { .. } => None,
        }
    }
}

fn container_manager_error(error: ContainerManagerError) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "container manager backend selection".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error.to_string(),
    }
}

impl std::fmt::Display for ContainerExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch { command, error } => {
                write!(f, "failed to launch `{command}`: {error}")
            }
            Self::Refusal { command, reason } => {
                write!(f, "{command} refused: {reason}")
            }
            Self::Failure {
                command,
                code,
                stdout,
                stderr,
            } => {
                write!(
                    f,
                    "{command} failed ({})\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    effigy_ui::human::exit_code(*code)
                )
            }
        }
    }
}

impl std::error::Error for ContainerExecError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInventoryFailure {
    pub backend: String,
    pub profile: String,
    pub error: String,
    /// The profile's VM is stopped rather than broken. Its created or stopped
    /// containers cannot be listed without starting it, but it publishes no
    /// host port while it is stopped, so only owners recorded on that profile
    /// are uncertain (effigy#294).
    pub stopped_profile: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunningComposeContainerInventory {
    pub rows: Vec<RunningComposeContainerProfiled>,
    pub failures: Vec<RuntimeInventoryFailure>,
}

impl RunningComposeContainerInventory {
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failure_summary(&self) -> Option<String> {
        (!self.failures.is_empty()).then(|| {
            self.failures
                .iter()
                .map(|failure| {
                    format!(
                        "{} profile `{}`: {}",
                        failure.backend, failure.profile, failure.error
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    /// Colima profiles whose VM is stopped. Their containers cannot be listed
    /// without starting the profile, so an owner that may live there is
    /// uncertain; every other failure makes the inventory incomplete for
    /// every owner.
    pub fn stopped_profiles(&self) -> Vec<&str> {
        self.failures
            .iter()
            .filter(|failure| failure.stopped_profile)
            .map(|failure| failure.profile.as_str())
            .collect()
    }

    /// Failures that stop the inventory from proving absence for any owner.
    pub fn incomplete_failure_summary(&self) -> Option<String> {
        let failures = self
            .failures
            .iter()
            .filter(|failure| !failure.stopped_profile)
            .collect::<Vec<_>>();
        (!failures.is_empty()).then(|| {
            failures
                .iter()
                .map(|failure| {
                    format!(
                        "{} profile `{}`: {}",
                        failure.backend, failure.profile, failure.error
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    fn into_complete_rows(
        self,
    ) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
        let Some(summary) = self.failure_summary() else {
            return Ok(self.rows);
        };
        Err(ContainerExecError::Failure {
            command: "container runtime inventory".to_owned(),
            code: None,
            stdout: format!("collected {} container rows", self.rows.len()),
            stderr: format!("inventory is incomplete: {summary}"),
        })
    }
}

pub fn capture_compose_ps(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<String, ContainerExecError> {
    let output = run_docker_capture(repo_root, policy, args, label)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn shutdown_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for (args, label) in shutdown_compose_commands(policy) {
        let (program, invocation_args) = compose_invocation_for_repo(repo_root, policy, &args);
        let rendered_args = invocation_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        run_command_capture_with_timeout_and_env(
            repo_root,
            program,
            &rendered_args
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            label,
            CONTAINER_SHUTDOWN_TIMEOUT,
            &[],
        )?;
    }
    Ok(())
}

pub fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_capture(repo_root, policy, args, label)
}

/// Ask the runtime for a restart policy on these containers, named by verified
/// full ID only. The caller proves ownership first (`restart::apply_after_start`).
/// This is a runtime
/// configuration change on the containers Compose created, not a start of
/// anything, so it cannot race Compose's own create/start (effigy#383).
pub fn set_restart_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    containers: &[String],
    restart: &str,
) -> Result<(), ContainerExecError> {
    let mut args = vec![
        OsString::from("update"),
        OsString::from("--restart"),
        OsString::from(restart),
    ];
    args.extend(containers.iter().map(OsString::from));
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime update",
        Some(CONTAINER_SHUTDOWN_TIMEOUT),
        None,
    )
    .map(|_| ())
}

pub fn run_compose_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_capture_with_env(repo_root, policy, args, label, &[])
}

/// [`run_compose_capture`] with the environment the one apply delivers. A
/// task's environment ensure and `container up` both use it (effigy#371).
pub fn run_compose_capture_with_env(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let (program, args) = compose_invocation_for_repo(repo_root, policy, args);
    run_compose_invocation_capture_with_env(repo_root, OsStr::new(program), &args, label, env)
}

pub fn list_running_compose_containers_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        None,
        None,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// One container of a shared service's own Compose project, with the Compose
/// service and render identity the container carries. A shared project belongs
/// to no checkout, so a data command selects a container by these labels rather
/// than by a name pattern (effigy#477).
pub struct SharedProjectContainer {
    pub container_name: String,
    pub running: bool,
    /// `None` when no Effigy wrote the render label, which is what a container
    /// an older Effigy created looks like (effigy#377).
    pub render: Option<String>,
    /// The Compose service label, which is how a shared data target proves the
    /// container belongs to the declared service rather than a lookalike.
    pub service: Option<String>,
}

const SHARED_PROJECT_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Label \"com.effigy.shared-render\"}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.service\"}}";

/// Every container of one shared-service Compose project, running or stopped,
/// with the render each was created from. A shared project belongs to no
/// checkout, so this reads the runtime by project name alone.
pub fn shared_project_containers(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<Vec<SharedProjectContainer>, ContainerExecError> {
    let args = vec![
        OsString::from("ps"),
        OsString::from("--all"),
        OsString::from("--filter"),
        OsString::from(format!("label=com.docker.compose.project={project_name}")),
        OsString::from("--format"),
        OsString::from(SHARED_PROJECT_PS_FORMAT),
    ];
    let output = capture_runtime_command(repo_root, policy, &args, "runtime ps --all", None, None)?;
    let refuse = || {
        ContainerExecError::refused(
            "runtime ps --all",
            "shared inventory is malformed or undecodable",
        )
    };
    std::str::from_utf8(&output.stdout)
        .map_err(|_| refuse())?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            let [name, status, render, project, service] = fields.as_slice() else {
                return Err(refuse());
            };
            if name.trim().is_empty() || status.trim().is_empty() || *project != project_name {
                return Err(refuse());
            }
            Ok(SharedProjectContainer {
                container_name: name.trim().to_owned(),
                running: super::parse::compose_status_is_running(status),
                render: (!render.is_empty()).then(|| (*render).to_owned()),
                service: (!service.is_empty()).then(|| (*service).to_owned()),
            })
        })
        .collect()
}

/// Runtime `ps` arguments listing one Compose project's containers. The label
/// filter keeps the runtime from enumerating every container in a shared VM;
/// callers still filter the parsed rows by project as the authority.
fn project_ps_args(project_name: &str, include_stopped: bool) -> Vec<OsString> {
    let mut args = vec![OsString::from("ps")];
    if include_stopped {
        args.push(OsString::from("--all"));
    }
    args.extend([
        OsString::from("--filter"),
        OsString::from(format!("label=com.docker.compose.project={project_name}")),
        OsString::from("--format"),
        OsString::from(DOCKER_PS_FORMAT),
    ]);
    args
}

/// Deadline-aware compose `ps` probe. One caller-visible monotonic deadline is
/// converted to a per-spawn remainder, so a preceding liveness probe consumes
/// the same budget instead of granting this probe a fresh timeout. An expired
/// deadline never spawns; timeouts stay `unavailable` (distinct from stopped or
/// clean) and the bounded doctor path never triggers a Colima runtime repair.
pub fn list_running_compose_containers_for_policy_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        None,
        deadline,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// Complete project inventory, including stopped rows, for a primary decision.
/// Unlike the general listing, incomplete labels and invalid UTF-8 refuse.
pub fn decision_project_inventory(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selected: &crate::environment::BackendSelection,
    deadline: Instant,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let (program, args) = selected
        .compose_invocation(&project_ps_args(&policy.project_name, true))
        .map_err(|error| ContainerExecError::Failure {
            command: "runtime ps decision inventory".into(),
            code: None,
            stdout: String::new(),
            stderr: error.to_string(),
        })?;
    let output = run_backend_invocation_until(
        repo_root,
        &program,
        &args,
        "runtime ps decision inventory",
        &[],
        deadline,
    )?;
    let text = std::str::from_utf8(&output.stdout).map_err(|_| ContainerExecError::Failure {
        command: "runtime ps decision inventory".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "inventory is not UTF-8".to_owned(),
    })?;
    super::parse::strict_project_inventory(text, &policy.project_name)
}

/// Deadline-aware compose `ps` probe for an arbitrary Compose project in
/// this checkout — used by the `data_from` handover to discover a peer
/// environment's running services. The policy only selects the runtime
/// profile and backend; container selection is by the given project name.
pub fn list_running_compose_containers_for_project_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(project_name, false),
        "runtime ps",
        None,
        deadline,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(project_name))
            .collect(),
    )
}

/// Deadline-aware compose `ps --all` probe that keeps stopped rows. A
/// project-wide `compose up` can start or recreate a stopped declared sibling,
/// so ownership must be proven for those rows before it is authorized, not
/// only for the running ones. A caller-visible monotonic deadline bounds the
/// single spawn and an expired deadline never spawns.
pub fn list_compose_containers_for_policy_including_stopped_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), true),
        "runtime ps --all",
        None,
        deadline,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

pub fn list_running_compose_containers_for_policy_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// Re-admit inventory rows from exact current inspect identities and bindings.
/// A ps name, reserved address, or successful command alone is not port proof.
/// Read the Compose labels an owned binding check needs. Docker compose
/// records the project working dir and an explicit `oneoff`; nerdctl compose
/// records neither, so the working dir falls back to Effigy's checkout label
/// and a missing `oneoff` is accepted (the inspected identity still rejects
/// one-off containers).
mod ownership;
pub(super) use ownership::declared_service_names;
use ownership::{compose_binding_labels, inspect_owned_binding_args};
pub use ownership::{
    compose_stack_is_running, owned_service_listing_args, shared_render_identity,
    verify_owned_running_compose_identities, verify_owned_running_compose_rows,
    verify_shared_render_container, OwnershipContext, VerifiedComposeContainer,
};

mod workspace_builder;
pub use workspace_builder::{
    expected_workspace_source, observed_workspace_artifact_builder,
    verify_workspace_artifact_builder, verify_workspace_artifact_builder_declaration,
    workspace_artifact_builder_preparation, ObservedWorkspaceArtifactBuilder,
    WorkspaceBuilderPreparation,
};

pub fn list_compose_containers_for_project_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = retry_inventory_once(|| {
        capture_runtime_command(
            repo_root,
            policy,
            &project_ps_args(project_name, true),
            "runtime ps --all",
            Some(COMPOSE_PS_TIMEOUT),
            None,
        )
    })?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(project_name))
            .collect(),
    )
}

/// Retry one read-only runtime inventory once when Effigy's own bound expired.
/// A machine under load can miss a single 30s observation; a parse error, a
/// refused runtime or a second timeout is returned unchanged so admission still
/// fails closed and the message names the runtime observation, not the task.
fn retry_inventory_once(
    mut run: impl FnMut() -> Result<Output, ContainerExecError>,
) -> Result<Output, ContainerExecError> {
    match run() {
        Err(error) if super::process::error_is_timeout(&error) => {
            std::thread::sleep(INVENTORY_RETRY_BACKOFF);
            run().map_err(|second| match second {
                ContainerExecError::Failure {
                    command,
                    code,
                    stdout,
                    stderr,
                } => ContainerExecError::Failure {
                    command,
                    code,
                    stdout,
                    stderr: format!(
                        "{stderr}\n[effigy] runtime inventory admission timed out twice; the runtime did not answer an observation, so the task did not run. Check `effigy container status` and retry, or run the task's host route with `cargo` by hand where the repository defines one. First timeout: {error}"
                    ),
                },
                other => other,
            })
        }
        result => result,
    }
}

pub fn list_compose_containers_for_policy_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    list_compose_containers_for_project_including_stopped(
        repo_root,
        policy,
        policy.project_name.as_str(),
    )
}

pub fn list_compose_containers_for_policy_including_stopped_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), true),
        "runtime ps --all",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// `runtime inspect` for the named containers of one policy's backend.
///
/// A readiness wait reads every service's own state and health check from
/// here; the bounded deadline is the wait's, so a slow runtime cannot extend
/// it (effigy#313).
pub fn inspect_containers_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    names: &[String],
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let mut args = vec![OsString::from("inspect")];
    args.extend(names.iter().map(OsString::from));
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime inspect",
        Some(COMPOSE_PS_TIMEOUT),
        deadline,
    )
}

/// `runtime image inspect` for one exact image. The caller reads the platform
/// the backend reports for the image itself; a container's own report, a host
/// default or a guess never substitutes for it.
pub fn inspect_image_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    image: &str,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let args = vec![
        OsString::from("image"),
        OsString::from("inspect"),
        OsString::from(image),
    ];
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime image inspect",
        Some(COMPOSE_PS_TIMEOUT),
        deadline,
    )
}

/// The one line that says why a container stopped: the last line it logged
/// that names a cause, else the last line it logged. Read on demand for one
/// failure diagnostic; only exited or unhealthy services are read this way, so
/// a healthy stack pays nothing.
pub fn container_last_log_line_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container: &str,
    tail: u32,
) -> Result<String, ContainerExecError> {
    let args = vec![
        OsString::from("logs"),
        OsString::from("--tail"),
        OsString::from(tail.to_string()),
        OsString::from(container),
    ];
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &args,
        "runtime logs",
        COMPOSE_PS_TIMEOUT,
    )?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(effigy_ui::human::last_output_line(&combined).unwrap_or_default())
}

/// One runtime capture. A failure is reported as it stands: Effigy never
/// stops or restarts the shared Colima profile to "repair" it (effigy#371).
///
/// Every capture is bounded: a read that names no deadline and no timeout is
/// still the standard runtime read, so a runtime that stops answering reports
/// that read as unavailable instead of holding the command open (effigy#411).
fn capture_runtime_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    timeout: Option<Duration>,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    if let Some(deadline) = deadline {
        let remaining = super::process::remaining_until_deadline(deadline, label)?;
        return run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root, policy, args, label, deadline, remaining,
        );
    }
    run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        args,
        label,
        timeout.unwrap_or(COMPOSE_PS_TIMEOUT),
    )
}

/// Runs one invocation an environment backend adapter resolved for its exact
/// selection. Detached Compose activation never inherits a capture pipe.
pub fn run_backend_invocation(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return run_detached_command_with_env(workdir, &program, args, label, env);
    }
    run_command_capture_os_with_env(workdir, &program, args, label, env)
}

/// [`run_backend_invocation`] bounded by the caller's absolute deadline.
/// Detached Compose keeps regular-file output custody and retained logs;
/// observations use bounded capture. Both share the activation's deadline.
pub fn run_backend_invocation_until(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return super::process::run_detached_command_with_env_until(
            workdir, &program, args, label, env, deadline,
        );
    }
    super::process::run_command_capture_os_with_env_until(
        workdir, &program, args, label, env, deadline,
    )
}

/// Refuse private execution until the reviewed same-Env capability exists.
/// Every input reaches the same refusal before interpolation or process work.
pub fn run_backend_invocation_private_until(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<(), ContainerExecError> {
    super::process::run_private_detached_command(
        workdir,
        &program.to_string_lossy(),
        args,
        label,
        env,
        deadline,
    )
}

pub fn run_compose_invocation_capture(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_invocation_capture_with_env(repo_root, program, args, label, &[])
}

pub fn run_compose_invocation_capture_with_env(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return run_detached_command_with_env(repo_root, &program, args, label, env);
    }
    run_command_capture_os_with_env(repo_root, &program, args, label, env)
}

/// Run one Compose invocation whose progress a person is watching.
///
/// The command's output still lands in the checkout's retained backend
/// record, exactly as a detached invocation does; every complete line is
/// reported to `on_line` as it is written, so the caller can name what the
/// runtime is doing while it does it (effigy#313). A stop request ends the
/// invocation's whole process group and returns its status.
#[allow(
    clippy::too_many_arguments,
    reason = "one invocation needs its policy, argv, env, stop flag and line sink"
)]
pub fn run_compose_invocation_streaming_with_env(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    match run_detached_command_streaming_with_env(
        repo_root,
        &program,
        args,
        label,
        env,
        stop_requested,
        on_line,
    ) {
        Ok(output) => Ok(output),
        // nerdctl keeps its own name store. A container it left behind makes
        // replacing a service that really changed fail on that name; clear the
        // stale one and retry once.
        Err(error) => {
            if let ContainerExecError::Failure { stdout, stderr, .. } = &error {
                if take_stale_owned_container(repo_root, policy, stdout, stderr)? {
                    return run_detached_command_streaming_with_env(
                        repo_root,
                        &program,
                        args,
                        label,
                        env,
                        stop_requested,
                        on_line,
                    );
                }
            }
            Err(error)
        }
    }
}

/// Remove the container nerdctl's name store still holds when it blocked a
/// create, and report that the caller may retry once.
///
/// A container that carries another project's `com.docker.compose.project`
/// label is never touched: its name conflict is reported as it stands.
fn take_stale_owned_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stdout: &str,
    stderr: &str,
) -> Result<bool, ContainerExecError> {
    let Some(claimed) = name_store_conflict(&format!("{stdout}\n{stderr}")) else {
        return Ok(false);
    };
    let Ok(observed) =
        inspect_containers_for_policy(repo_root, policy, std::slice::from_ref(&claimed), None)
    else {
        return Ok(false);
    };
    let project = serde_json::from_slice::<serde_json::Value>(&observed.stdout)
        .ok()
        .and_then(|value| {
            let labels = value.get(0)?.get("Config")?.get("Labels")?;
            labels
                .get(crate::COMPOSE_PROJECT_LABEL)?
                .as_str()
                .map(str::to_owned)
        });
    if project.as_deref() != Some(policy.project_name.as_str()) {
        return Ok(false);
    }
    // A running container's `rm -f` is a kill the restart manager may undo for
    // `restart: unless-stopped`; stopping first records a user stop, so the
    // removal holds (effigy#376). A stop that finds nothing running is fine.
    let _ = run_docker_capture(
        repo_root,
        policy,
        &[OsString::from("stop"), OsString::from(&claimed)],
        "runtime stop",
    );
    let removed = run_docker_capture(
        repo_root,
        policy,
        &[
            OsString::from("rm"),
            OsString::from("-f"),
            OsString::from(&claimed),
        ],
        "runtime rm",
    )?;
    Ok(removed.status.success())
}

/// The identifier nerdctl names when a create lost to its own name store:
/// `name "<container>" is already used by ID <id>`.
fn name_store_conflict(message: &str) -> Option<String> {
    let marker = "is already used by id";
    let start = message.to_ascii_lowercase().find(marker)? + marker.len();
    message[start..]
        .split(|character: char| !character.is_ascii_hexdigit())
        .find(|candidate| candidate.len() >= 12)
        .map(str::to_owned)
}

#[path = "implementation/inventory.rs"]
mod inventory;
pub use inventory::{
    capture_running_container_stats, capture_running_container_stats_for_profile,
    discover_compose_containers_all_states, discover_running_compose_containers,
    infer_host_working_dir_for_container, list_running_compose_containers,
    list_running_compose_containers_for_profile, list_running_compose_containers_profiled,
};
