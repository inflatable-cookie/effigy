use serde_json::Value as JsonValue;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use super::healthcheck_pidfiles::prune_stale_health_check_pidfiles;
use super::implementation::ContainerExecError;
use super::parse::docker_failure_looks_like_colima_runtime_state_loss;
use super::process::{
    error_is_timeout, format_args, run_command_capture,
    run_command_capture_allow_failure_with_deadline, run_command_capture_with_absolute_deadline,
    run_command_capture_with_timeout,
};
use crate::{
    colima::{
        colima_start_command, colima_status_command, colima_stop_command,
        managed_colima_profile_drift, managed_colima_profile_resources, parse_colima_running,
        prepare_managed_colima_profile,
    },
    compose::{resolve_compose_backend_for_repo, resolve_host_cli_program, ComposeBackend},
    user_global_backend_preference, user_global_colima_profile, BackendId,
    ContainerBackendDetection, ContainerManager, ContainerManagerError, EffectiveContainerPolicy,
    DEFAULT_COLIMA_PROFILE,
};

const COLIMA_START_TIMEOUT: Duration = Duration::from_secs(90);
/// A profile that does not exist yet pays for the VM image, its boot and the
/// boot provisioning, so creating it is bounded further out than starting a
/// stopped one. Without this a deleted profile could never finish its first
/// boot inside the start bound.
const COLIMA_CREATE_TIMEOUT: Duration = Duration::from_secs(300);
const COLIMA_RUNTIME_READY_TIMEOUT: Duration = Duration::from_secs(90);
const COLIMA_RUNNING_PROFILE_GRACE: Duration = Duration::from_secs(30);
const COLIMA_RUNTIME_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const COLIMA_RUNTIME_PROBE_INTERVAL: Duration = Duration::from_millis(500);
const COLIMA_STATUS_TIMEOUT: Duration = Duration::from_secs(30);
const COLIMA_STOP_TIMEOUT: Duration = Duration::from_secs(45);
const COLIMA_STOP_SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound for the stale owned-container reconcile that follows a profile
/// creation or restart: one project inventory plus an inspect per row.
/// Bound for starting stopped runtime units inside a running VM, and for the
/// readiness re-check that follows, before falling back to a VM restart.
const COLIMA_UNIT_START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaRecoveryReport {
    pub profile: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColimaProfileEntry {
    name: String,
    status: String,
    cpus: u64,
    memory: u64,
    disk: u64,
}

/// Bound for the `container up` prune of leaked nerdctl health-check pid
/// files. A profile holding the leak needs a few seconds to walk and delete
/// its files; a clean profile walks a handful.
const HEALTHCHECK_PIDFILE_PRUNE_TIMEOUT: Duration = Duration::from_secs(30);

/// What one managed-profile ensure did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColimaEnsureReport {
    /// The profile was not running, so this call started it.
    pub started: bool,
    /// The profile was created by this call. Every owned container admitted
    /// against the previous profile is gone or stale, so a retained container
    /// admission recorded earlier is void.
    pub profile_created_or_restarted: bool,
}

pub fn ensure_colima_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    Ok(ensure_colima_running_report(policy, repo_root)?.started)
}

/// The one entry point for a managed Colima profile: create it when it does not
/// exist and start it when stopped. A running profile is never stopped or
/// restarted here: other checkouts' stacks share it.
pub fn ensure_colima_running_report(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    let action = ensure_colima_profile_running(policy, repo_root)?;
    let report = ColimaEnsureReport {
        started: action != ColimaProfileAction::Reused,
        profile_created_or_restarted: action.recreates_profile(),
    };
    maintain_health_check_pid_files(policy, repo_root);
    Ok(report)
}

/// What the one managed-profile entry point did to reach a running profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColimaProfileAction {
    /// The profile was already running with Effigy's managed settings.
    Reused,
    /// The profile did not exist: first use, or the user deleted it.
    Created,
    /// The profile existed but its VM was stopped.
    Started,
}

impl ColimaProfileAction {
    /// Whether this call replaced the profile the previous generation ran on.
    /// A created profile keeps nothing, so a retained container admission can
    /// no longer describe it.
    fn recreates_profile(self) -> bool {
        matches!(self, Self::Created)
    }
}

/// `container up`'s bounded prune of the pid files nerdctl 2.3.1 health checks
/// leak into the VM's `/run` (effigy#291). It never fails the bring-up and says
/// nothing when it succeeds: a failed prune warns, and the explicit
/// `container profile prune-health-pid-files` reports the counts.
fn maintain_health_check_pid_files(policy: &EffectiveContainerPolicy, repo_root: &Path) {
    let deadline = Instant::now() + HEALTHCHECK_PIDFILE_PRUNE_TIMEOUT;
    if let Err(error) = prune_stale_health_check_pidfiles(&policy.profile, repo_root, deadline) {
        eprintln!(
            "[warn] Colima profile `{}`: stale nerdctl health-check pid-file prune skipped: {error}. `effigy container profile prune-health-pid-files --profile {}` runs it on request",
            policy.profile, policy.profile
        );
    }
}

fn ensure_colima_profile_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaProfileAction, ContainerExecError> {
    if colima_is_running(policy, repo_root)? {
        return ensure_running_managed_profile_ready(policy, repo_root);
    }
    // A profile that is not running is either stopped or gone. Only the
    // inventory can tell them apart, and both take the same managed start; the
    // user is told which one it was.
    if colima_profile_exists(policy, repo_root)? {
        start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Started)
    } else {
        // First use, or the user deleted it.
        start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Created)
    }
}

/// Whether `colima list` knows the profile at all. An inventory that cannot be
/// read is not proof of absence, so it takes the stopped-profile start path.
fn colima_profile_exists(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    let Ok(entries) = list_colima_profiles(repo_root) else {
        return Ok(true);
    };
    Ok(entries
        .into_iter()
        .any(|entry| entry.name == policy.profile))
}

/// A running profile is only reusable when it carries Effigy's managed
/// settings. Observed drift warns once and names the deliberate command: the
/// profile is shared with every other checkout's stacks, so Effigy never stops
/// or restarts it, and the user's profile is never deleted.
fn ensure_running_managed_profile_ready(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaProfileAction, ContainerExecError> {
    if let Some(drift) = managed_colima_profile_drift(&policy.profile) {
        // Write the managed settings so the operator's own restart applies them.
        let _ = prepare_managed_colima_profile(policy);
        eprintln!("[warn] {}", drift.warning(&policy.profile));
    }
    ensure_running_profile_runtime_ready(
        policy.profile.as_str(),
        COLIMA_RUNNING_PROFILE_GRACE,
        |timeout| wait_for_colima_runtime_ready(policy, repo_root, timeout),
        || start_colima_runtime_units(policy, repo_root),
    )?;
    Ok(ColimaProfileAction::Reused)
}

/// The only place a managed Colima profile is created or started: Effigy's
/// managed config is written first, then `colima start` runs with the managed
/// arguments (SSH agent, DNS fallbacks, managed resources, platform overrides).
fn start_managed_colima_profile(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    action: ColimaProfileAction,
) -> Result<ColimaProfileAction, ContainerExecError> {
    match action {
        ColimaProfileAction::Created => eprintln!(
            "[info] {}",
            crate::colima::colima_profile_creation_notice(&policy.profile)
        ),
        ColimaProfileAction::Started => eprintln!(
            "[info] {}",
            crate::colima::colima_profile_start_notice(&policy.profile)
        ),
        ColimaProfileAction::Reused => {}
    }
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    let cmd = colima_start_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    // A profile that does not exist pays for its first VM image and boot.
    let start_timeout = if action == ColimaProfileAction::Created {
        COLIMA_CREATE_TIMEOUT
    } else {
        COLIMA_START_TIMEOUT
    };
    run_command_capture_with_timeout(repo_root, &cmd.program, &args, &cmd.label, start_timeout)?;
    if action == ColimaProfileAction::Created {
        eprintln!(
            "[info] {}",
            crate::colima::colima_profile_recreated_notice(&policy.profile)
        );
    }
    wait_for_colima_runtime_ready(policy, repo_root, COLIMA_RUNTIME_READY_TIMEOUT)
        .map_err(|error| runtime_readiness_error(&error))?;
    Ok(action)
}

fn ensure_running_profile_runtime_ready(
    profile: &str,
    grace: Duration,
    mut wait_for_runtime: impl FnMut(Duration) -> Result<(), String>,
    mut start_units: impl FnMut() -> Result<(), String>,
) -> Result<bool, ContainerExecError> {
    match wait_for_runtime(grace) {
        Ok(()) => Ok(false),
        Err(readiness_error) => {
            // A VM restarted outside Colima's boot provisioning can come back
            // Running with containerd and BuildKit stopped. Starting those
            // units inside Effigy's own profile recovers it without touching
            // the VM or any other profile. Stopping or restarting the VM is a
            // host-wide effect no task bring-up may take on its own: when the
            // units still do not come up, refuse and name the operator path.
            match start_units() {
                Ok(()) => match wait_for_runtime(COLIMA_UNIT_START_TIMEOUT) {
                    Ok(()) => {
                        eprintln!(
                            "[info] Colima profile `{profile}` was running with containerd/BuildKit stopped; started them inside the VM without restarting it"
                        );
                        Ok(true)
                    }
                    Err(probe_error) => Err(runtime_unit_start_refusal(
                        profile,
                        &readiness_error,
                        None,
                        &probe_error,
                    )),
                },
                Err(start_error) => Err(runtime_unit_start_refusal(
                    profile,
                    &readiness_error,
                    Some(&start_error),
                    "not retried: the unit start itself failed",
                )),
            }
        }
    }
}

/// The bounded unit start is the only implicit recovery a task bring-up may
/// perform. This refusal names the profile, the three bounded observations and
/// the explicit operator commands; it never restarts the VM.
fn runtime_unit_start_refusal(
    profile: &str,
    readiness_error: &str,
    start_error: Option<&str>,
    probe_error: &str,
) -> ContainerExecError {
    let start = start_error
        .map(|error| format!("\nunit start: {error}"))
        .unwrap_or_default();
    ContainerExecError::Failure {
        command: "Colima runtime readiness".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "Colima profile `{profile}` is running but containerd/BuildKit are not ready, and starting those units inside the VM did not recover them within {}s. Effigy did not stop or restart the profile: that stops every stack on it.\nfirst readiness probe: {readiness_error}{start}\nruntime probe after unit start: {probe_error}\nRetry, then recover this profile in place with `effigy container profile repair`, or stop it deliberately with `effigy container profile reset-runtime`.",
            COLIMA_UNIT_START_TIMEOUT.as_secs(),
        ),
    }
}

pub fn runtime_backend_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            Ok(output.status.success())
        }
        ComposeBackend::ColimaNerdctl => colima_is_running(policy, repo_root),
    }
}

pub fn ensure_runtime_backend_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    Ok(ensure_runtime_backend_running_report(policy, repo_root)?.started)
}

/// [`ensure_runtime_backend_running`] with the profile transition reported, so
/// a caller holding a container admission can tell that the profile it was
/// recorded against was created or restarted.
pub fn ensure_runtime_backend_running_report(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            if output.status.success() {
                Ok(ColimaEnsureReport::default())
            } else {
                Err(ContainerExecError::Failure {
                    command: "docker ps".to_owned(),
                    code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            }
        }
        ComposeBackend::ColimaNerdctl => ensure_colima_running_report(policy, repo_root),
    }
}

pub fn colima_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    colima_is_running_with_deadline(policy, repo_root, None)
}

/// Deadline-aware Colima liveness probe. A bounded (doctor) call shares one
/// monotonic deadline: an expired deadline never spawns, and
/// timeouts/unavailable never report clean. The probe only reads: Effigy never
/// stops or restarts the shared profile to repair it (effigy#371).
pub fn colima_is_running_with_deadline(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    deadline: Option<Instant>,
) -> Result<bool, ContainerExecError> {
    let cmd = colima_status_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        &cmd.program,
        &args,
        &cmd.label,
        deadline,
    )?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_colima_running(&stdout, &stderr))
}

fn colima_runtime_probe_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(BackendId::colima_nerdctl());
    let args = [OsString::from("info")];
    ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", &args)
        .map(|(_, args)| args)
        .unwrap_or_else(|_| {
            vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
                OsString::from("info"),
            ]
        })
}

fn colima_buildkit_probe_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    vec![
        OsString::from("ssh"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
        OsString::from("sudo"),
        OsString::from("buildctl"),
        OsString::from("debug"),
        OsString::from("workers"),
    ]
}

fn probe_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let per_probe_timeout = (timeout / 2).min(COLIMA_RUNTIME_PROBE_TIMEOUT);
    run_colima_readiness_command(
        repo_root,
        &colima_runtime_probe_args(policy),
        "Colima containerd readiness probe",
        per_probe_timeout,
    )?;
    run_colima_readiness_command(
        repo_root,
        &colima_buildkit_probe_args(policy),
        "Colima BuildKit readiness probe",
        per_probe_timeout,
    )
}

fn run_colima_readiness_command(
    repo_root: &Path,
    args: &[OsString],
    label: &str,
    timeout: Duration,
) -> Result<(), String> {
    let rendered = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let borrowed = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    run_command_capture_with_timeout(repo_root, "colima", &borrowed, label, timeout)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn wait_for_colima_runtime_ready(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    timeout: Duration,
) -> Result<(), String> {
    wait_for_runtime_readiness(
        timeout,
        COLIMA_RUNTIME_PROBE_INTERVAL,
        |probe_timeout| probe_colima_runtime(policy, repo_root, probe_timeout),
        thread::sleep,
    )
}

fn wait_for_runtime_readiness(
    timeout: Duration,
    interval: Duration,
    mut probe: impl FnMut(Duration) -> Result<(), String>,
    mut pause: impl FnMut(Duration),
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match probe(remaining) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        pause(interval.min(remaining));
    }
    Err(format!(
        "Colima containerd and BuildKit did not become ready within {}s; last runtime probe: {}",
        timeout.as_secs(),
        last_error.as_deref().unwrap_or("no probe completed")
    ))
}

fn runtime_readiness_error(detail: &str) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "Colima runtime readiness".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: detail.to_owned(),
    }
}

pub fn colima_profile_warnings(policy: &EffectiveContainerPolicy, repo_root: &Path) -> Vec<String> {
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    if policy.profile == "default" {
        warnings.push(
            "Colima profile `default` is shared with unrelated Colima workloads; Effigy now reserves the implicit profile name `effigy`. Keep `default` only if that sharing is intentional.".to_owned(),
        );
    }

    if !policy.has_selected_workspace_service() {
        return warnings;
    }

    let Some(resources) = managed_colima_profile_resources(&policy.profile) else {
        return warnings;
    };
    let Some(entry) = colima_profile_entry(repo_root, &policy.profile) else {
        return warnings;
    };
    if !entry.status.eq_ignore_ascii_case("running") {
        return warnings;
    }

    let host_memory = resources
        .host_memory_gib
        .map(|value| format!(" on this {value}GiB host"))
        .unwrap_or_default();
    let expected_memory_bytes = resources.memory_gib.saturating_mul(1024 * 1024 * 1024);
    if entry.memory < expected_memory_bytes {
        let actual_memory_gib = entry.memory / (1024 * 1024 * 1024);
        warnings.push(format!(
            "Colima profile `{}` is running with {}GiB RAM; Effigy recommends {}GiB memory and {}GiB swap{} for workspace-heavy Rust builds. Run `effigy container profile resize` to stop and restart the profile with the managed sizing.",
            policy.profile,
            actual_memory_gib.max(1),
            resources.memory_gib,
            resources.swap_gib,
            host_memory,
        ));
    }

    let expected_disk_bytes = resources.disk_gib.saturating_mul(1024 * 1024 * 1024);
    if entry.disk < expected_disk_bytes {
        let actual_disk_gib = entry.disk / (1024 * 1024 * 1024);
        warnings.push(format!(
            "Colima profile `{}` is running with {}GiB disk; Effigy recommends {}GiB disk for multi-repo Rust/container workspaces. Run `effigy container profile resize` to grow the profile without deleting local runtime data.",
            policy.profile,
            actual_disk_gib.max(1),
            resources.disk_gib,
        ));
    }
    warnings
}

pub fn selected_backend_label(policy: &EffectiveContainerPolicy, repo_root: &Path) -> &'static str {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => "docker",
        ComposeBackend::ColimaNerdctl => "containerd",
    }
}

pub(super) fn detect_container_backend() -> Result<BackendId, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override = user_global_backend_preference();
    }
    if detection.backend_override.is_none() && !running_colima_profiles(Path::new("."))?.is_empty()
    {
        detection.backend_override = Some(BackendId::colima_nerdctl());
    }
    ContainerManager::defaults()
        .registry()
        .detect_backend(&detection)
        .map_err(container_manager_error)
}

pub fn running_colima_profiles(repo_root: &Path) -> Result<Vec<String>, ContainerExecError> {
    let profiles = list_colima_profiles(repo_root)?;
    Ok(profiles
        .into_iter()
        .filter(|entry| entry.status.eq_ignore_ascii_case("running"))
        .map(|entry| entry.name)
        .collect())
}

/// Profile names for an all-states inventory: the profiles that are running,
/// and the ones whose VM is stopped.
///
/// A stopped profile is reported separately instead of failing the whole
/// inventory. Its created or stopped containers cannot be listed without
/// starting it, so absence is unprovable for an owner recorded on that
/// profile; every other owner is unaffected, because a stopped VM publishes no
/// host port (effigy#294).
pub(super) fn colima_profiles_for_all_states_inventory(
) -> Result<(Vec<String>, Vec<String>), ContainerExecError> {
    let entries = list_colima_profiles(Path::new("."))?;
    let mut running = Vec::new();
    let mut stopped = Vec::new();
    for entry in entries {
        if entry.status.eq_ignore_ascii_case("running") {
            running.push(entry.name);
        } else {
            stopped.push(entry.name);
        }
    }
    Ok((running, stopped))
}

pub(crate) fn default_runtime_profile() -> String {
    user_global_colima_profile().unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned())
}

pub(super) fn run_runtime_command_capture_for_policy_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
    label: &str,
    timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let detection = runtime_detection_for_policy(repo_root, policy);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let rendered = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    run_command_capture_with_timeout(
        repo_root,
        &program,
        &rendered.iter().map(String::as_str).collect::<Vec<_>>(),
        label,
        timeout,
    )
}

/// Absolute-deadline runtime capture. The caller's monotonic deadline is the
/// single supervision origin; it is never converted to a fresh relative
/// timeout after spawn, so a slow spawn cannot extend it.
pub(super) fn run_runtime_command_capture_for_policy_with_absolute_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let detection = runtime_detection_for_policy(repo_root, policy);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let rendered = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    run_command_capture_with_absolute_deadline(
        repo_root,
        &program,
        &rendered.iter().map(String::as_str).collect::<Vec<_>>(),
        label,
        deadline,
        reported_timeout,
    )
}

pub(super) fn run_runtime_command_capture_for_policy_allow_failure(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let detection = runtime_detection_for_policy(repo_root, policy);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = resolve_host_cli_program(&program);
    Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", format_args(&args)),
            error,
        })
}

/// Start containerd and BuildKit inside a running Colima VM.
fn start_colima_runtime_units(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), String> {
    let args = [
        "ssh",
        "--profile",
        policy.profile.as_str(),
        "--",
        "sudo",
        "systemctl",
        "start",
        "containerd",
        "buildkit",
    ];
    run_command_capture_with_timeout(
        repo_root,
        "colima",
        &args,
        "Colima runtime unit start",
        COLIMA_UNIT_START_TIMEOUT,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// One phase of a Colima runtime recovery or reset, reported while the work
/// runs so the owning command plane names the command actually in flight
/// (Q-017). The recovery used to report nothing, so a plane could only guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColimaRecoveryPhase {
    /// `colima stop` for the profile.
    Stopping,
    /// Terminating the profile's leftover processes from its pidfiles.
    TerminatingProcesses,
    /// `colima status` proving the profile is stopped.
    VerifyingStopped,
    /// Creating the profile directories and reading, parsing and writing its
    /// managed configuration, before `colima start` can run.
    Configuring,
    /// `colima start` for the profile.
    Restarting,
    /// `colima status` proving the restarted profile is healthy.
    VerifyingHealth,
}

/// Reports each recovery phase as it starts.
pub type ColimaRecoveryProgress<'a> = &'a mut dyn FnMut(ColimaRecoveryPhase);

pub fn recover_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    recover_colima_runtime_with_progress(policy, repo_root, &mut |_| {})
}

/// Recover the profile, reporting the phase now in flight to `progress`.
pub fn recover_colima_runtime_with_progress(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] recovering Colima profile `{}`",
        policy.profile
    )];

    progress(ColimaRecoveryPhase::Stopping);
    let graceful_ok = match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => {
            steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile));
            true
        }
        Err(error) => {
            steps.push(format!(
                "[warning] graceful stop failed for profile `{}`; inspecting profile pidfiles\n{}",
                policy.profile, error
            ));
            false
        }
    };

    progress(ColimaRecoveryPhase::TerminatingProcesses);
    force_terminate_colima_profile_processes(policy, graceful_ok, &mut steps)?;

    progress(ColimaRecoveryPhase::VerifyingStopped);
    verify_profile_stopped(policy, repo_root, &steps, true)?;

    if let Err(error) = restart_and_verify_colima_profile(policy, repo_root, &mut steps, progress) {
        let runtime_state_loss = match &error {
            ContainerExecError::Failure { stdout, stderr, .. } => {
                docker_failure_looks_like_colima_runtime_state_loss(stdout, stderr)
            }
            ContainerExecError::Launch { .. } => false,
        };
        if !runtime_state_loss {
            return Err(error);
        }

        steps.push(format!(
            "[warning] profile `{}` still reported an empty runtime after restart; retrying one deeper repair pass",
            policy.profile
        ));
        progress(ColimaRecoveryPhase::Stopping);
        let stopped = match stop_colima_profile_for_repair(policy, repo_root) {
            Ok(()) => true,
            Err(error) => {
                steps.push(format!("[warning] retry graceful stop failed: {error}"));
                false
            }
        };
        progress(ColimaRecoveryPhase::TerminatingProcesses);
        force_terminate_colima_profile_processes(policy, stopped, &mut steps)?;
        progress(ColimaRecoveryPhase::VerifyingStopped);
        verify_profile_stopped(policy, repo_root, &steps, true)?;
        restart_and_verify_colima_profile(policy, repo_root, &mut steps, progress)?;
    }

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

pub fn reset_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    reset_colima_runtime_with_progress(policy, repo_root, &mut |_| {})
}

/// Reset the profile to a fully stopped state, reporting each phase in flight.
pub fn reset_colima_runtime_with_progress(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] resetting Colima profile `{}` to a fully stopped state",
        policy.profile
    )];

    progress(ColimaRecoveryPhase::Stopping);
    let graceful_ok = match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => {
            steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile));
            true
        }
        Err(error) => {
            steps.push(format!(
                "[warning] graceful stop failed for profile `{}`; inspecting profile pidfiles\n{}",
                policy.profile, error
            ));
            false
        }
    };

    progress(ColimaRecoveryPhase::TerminatingProcesses);
    force_terminate_colima_profile_processes(policy, graceful_ok, &mut steps)?;
    progress(ColimaRecoveryPhase::VerifyingStopped);
    verify_profile_stopped(policy, repo_root, &steps, false)?;
    steps.push(format!(
        "[ok] verified Colima profile `{}` is fully stopped",
        policy.profile
    ));

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

fn runtime_detection_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override =
            Some(match resolve_compose_backend_for_repo(repo_root, policy) {
                ComposeBackend::Docker => BackendId::docker_compose(),
                ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
            });
    }
    detection
}

fn container_manager_error(error: ContainerManagerError) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "container manager backend selection".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error.to_string(),
    }
}

fn colima_profile_entry(repo_root: &Path, profile: &str) -> Option<ColimaProfileEntry> {
    list_colima_profiles(repo_root)
        .ok()?
        .into_iter()
        .find(|entry| entry.name == profile)
}

fn list_colima_profiles(repo_root: &Path) -> Result<Vec<ColimaProfileEntry>, ContainerExecError> {
    let output = run_command_capture(repo_root, "colima", &["list", "--json"], "colima list")?;
    parse_colima_profiles(&String::from_utf8_lossy(&output.stdout))
}

fn parse_colima_profiles(stdout: &str) -> Result<Vec<ColimaProfileEntry>, ContainerExecError> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: JsonValue =
                serde_json::from_str(line).map_err(|error| ContainerExecError::Failure {
                    command: "colima list".to_owned(),
                    code: None,
                    stdout: stdout.to_owned(),
                    stderr: format!("failed to parse `colima list --json` row: {error}"),
                })?;
            Ok(ColimaProfileEntry {
                name: value
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `name` in `colima list --json` row".to_owned(),
                    })?
                    .to_owned(),
                status: value
                    .get("status")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `status` in `colima list --json` row".to_owned(),
                    })?
                    .to_owned(),
                cpus: value
                    .get("cpus")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `cpus` in `colima list --json` row".to_owned(),
                    })?,
                memory: value
                    .get("memory")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `memory` in `colima list --json` row".to_owned(),
                    })?,
                disk: value
                    .get("disk")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `disk` in `colima list --json` row".to_owned(),
                    })?,
            })
        })
        .collect()
}

fn restart_and_verify_colima_profile(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &mut Vec<String>,
    progress: ColimaRecoveryProgress<'_>,
) -> Result<(), ContainerExecError> {
    // Preparation reads, parses and writes the profile's managed configuration;
    // `colima start` has not begun while it runs.
    progress(ColimaRecoveryPhase::Configuring);
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    progress(ColimaRecoveryPhase::Restarting);
    let start = colima_start_command(policy);
    let start_args: Vec<&str> = start.args.iter().map(|value| value.as_str()).collect();
    run_command_capture_with_timeout(
        repo_root,
        &start.program,
        &start_args,
        &start.label,
        COLIMA_START_TIMEOUT,
    )?;
    steps.push(format!("[ok] started Colima profile `{}`", policy.profile));

    progress(ColimaRecoveryPhase::VerifyingHealth);
    let status = colima_status_command(policy);
    let status_args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_with_timeout(
        repo_root,
        &status.program,
        &status_args,
        &status.label,
        COLIMA_STATUS_TIMEOUT,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !parse_colima_running(&stdout, &stderr) {
        return Err(ContainerExecError::Failure {
            command: status.label,
            code: output.status.code(),
            stdout: stdout.into_owned(),
            stderr: stderr.into_owned(),
        });
    }
    steps.push(format!(
        "[ok] verified Colima profile `{}` is healthy",
        policy.profile
    ));
    Ok(())
}

fn stop_colima_profile_for_repair(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), ContainerExecError> {
    let stop = colima_stop_command(policy);
    let stop_args: Vec<&str> = stop.args.iter().map(|value| value.as_str()).collect();
    match run_command_capture_with_timeout(
        repo_root,
        &stop.program,
        &stop_args,
        &stop.label,
        COLIMA_STOP_TIMEOUT,
    ) {
        Ok(output) => Ok(output).map(|_| ()),
        Err(error) if stop_timeout_can_be_accepted(policy, repo_root, &error)? => Ok(()),
        Err(error) => Err(error),
    }
}

fn stop_timeout_can_be_accepted(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    error: &ContainerExecError,
) -> Result<bool, ContainerExecError> {
    if !error_is_timeout(error) {
        return Ok(false);
    }
    let deadline = Instant::now() + COLIMA_STOP_SETTLE_TIMEOUT;
    loop {
        match colima_running_probe_after_stop(policy, repo_root)? {
            Some(true) if Instant::now() < deadline => thread::sleep(Duration::from_millis(250)),
            Some(true) => return Ok(false),
            Some(false) | None => return Ok(true),
        }
    }
}

fn colima_running_probe_after_stop(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<Option<bool>, ContainerExecError> {
    let status = colima_status_command(policy);
    let args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        &status.program,
        &args,
        &status.label,
        Some(Instant::now() + COLIMA_STATUS_TIMEOUT),
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if docker_failure_looks_like_colima_runtime_state_loss(&stdout, &stderr) {
        return Ok(None);
    }
    if output.status.success() && parse_colima_running(&stdout, &stderr) {
        return Ok(Some(true));
    }
    if format!("{stdout}\n{stderr}")
        .to_ascii_lowercase()
        .contains("not running")
    {
        return Ok(Some(false));
    }
    Err(ContainerExecError::Failure {
        command: status.label,
        code: output.status.code(),
        stdout: stdout.into_owned(),
        stderr: format!("Colima stop status is unproven: {stderr}"),
    })
}

fn verify_profile_stopped(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &[String],
    allow_empty_runtime: bool,
) -> Result<(), ContainerExecError> {
    let reason = match colima_running_probe_after_stop(policy, repo_root) {
        Ok(Some(false)) => return Ok(()),
        Ok(None) if allow_empty_runtime => return Ok(()),
        Ok(Some(true)) => "profile is still running".to_owned(),
        Ok(None) => "profile status is unavailable".to_owned(),
        Err(error) => error.to_string(),
    };
    Err(ContainerExecError::Failure {
        command: "colima status".to_owned(),
        code: None,
        stdout: steps.join("\n"),
        stderr: format!("{reason}; refusing profile recovery. Verify pidfile ownership before manually running `colima stop -p {} --force`; Effigy did not run it.", policy.profile),
    })
}

fn force_terminate_colima_profile_processes(
    policy: &EffectiveContainerPolicy,
    graceful_ok: bool,
    steps: &mut Vec<String>,
) -> Result<(), ContainerExecError> {
    let stopped =
        super::colima_cleanup::terminate(&policy.profile, graceful_ok).map_err(|error| {
            ContainerExecError::Failure {
                command: "Colima profile pidfile cleanup".to_owned(),
                code: None,
                stdout: steps.join("\n"),
                stderr: error,
            }
        })?;
    steps.extend(stopped);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EffectiveComposeSource;
    use effigy_manifest::{
        with_test_user_config_home, ManifestContainerDriver, ManifestContainerOnTaskExit,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use std::path::PathBuf;

    #[test]
    fn colima_runtime_graceful_stop_never_delegates_force() {
        let stop = colima_stop_command(&test_policy());
        assert_eq!(stop.args, ["stop", "--profile", "effigy"]);
    }

    #[test]
    fn default_runtime_profile_honors_user_global_preference() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join(".effigy-home");
        std::fs::create_dir_all(&home).expect("mkdir home");
        std::fs::write(
            home.join("config.toml"),
            "[containers]\nprofile = \"devbox\"\n",
        )
        .expect("write config");
        let profile = with_test_user_config_home(&home, default_runtime_profile);
        assert_eq!(profile, "devbox");
    }

    #[test]
    fn runtime_detection_for_policy_prefers_repo_backend_override_over_user_global_preference() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("repo");
        let home = temp.path().join(".effigy-home");
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        std::fs::create_dir_all(&home).expect("mkdir home");
        std::fs::write(
            home.join("config.toml"),
            "[containers]\nbackend = \"containerd\"\n",
        )
        .expect("write config");
        // A v0.14 per-environment record, read until Prepare imports it.
        std::fs::create_dir_all(repo_root.join(".effigy/runtime/compose")).expect("mkdir");
        std::fs::write(
            repo_root.join(".effigy/runtime/compose/.effigy-runtime.json"),
            "[containers.web]\nbackend = \"docker-compose\"\n",
        )
        .expect("write legacy runtime backend record");

        let detection = with_test_user_config_home(&home, || {
            runtime_detection_for_policy(&repo_root, &test_policy())
        });

        assert_eq!(
            detection.backend_override,
            Some(BackendId::docker_compose())
        );
    }

    #[test]
    fn parse_colima_profiles_reads_disk_bytes() {
        let entries = parse_colima_profiles(
            r#"{"name":"effigy","status":"Running","cpus":2,"memory":34359738368,"disk":322122547200}"#,
        )
        .expect("parse profiles");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "effigy");
        assert_eq!(entries[0].disk, 300 * 1024 * 1024 * 1024);
    }

    #[test]
    fn runtime_readiness_waits_through_delayed_containerd_and_buildkit_startup() {
        let mut attempts = 0;
        wait_for_runtime_readiness(
            Duration::from_secs(1),
            Duration::from_millis(1),
            |_| {
                attempts += 1;
                match attempts {
                    1 => Err("nerdctl info cannot reach containerd yet".to_owned()),
                    2 => Err("buildctl workers cannot reach BuildKit yet".to_owned()),
                    _ => Ok(()),
                }
            },
            |_| {},
        )
        .expect("runtime should become ready");
        assert_eq!(attempts, 3);
    }

    #[test]
    fn runtime_readiness_permanent_failure_is_bounded_and_keeps_last_diagnostic() {
        let started = std::time::Instant::now();
        let error = wait_for_runtime_readiness(
            Duration::from_millis(10),
            Duration::from_millis(1),
            |_| Err("containerd endpoint unavailable".to_owned()),
            |_| {},
        )
        .expect_err("permanent runtime failure should time out");

        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(error.contains("did not become ready"));
        assert!(error.contains("containerd endpoint unavailable"));
    }

    #[test]
    fn warm_colima_profile_is_reused_without_starting_units() {
        let mut wait_timeouts = Vec::new();
        let started = ensure_running_profile_runtime_ready(
            "effigy",
            Duration::from_secs(30),
            |timeout| {
                wait_timeouts.push(timeout);
                Ok(())
            },
            || panic!("a ready profile never starts units"),
        )
        .expect("warm profile should be reusable");

        assert!(!started);
        assert_eq!(wait_timeouts, vec![Duration::from_secs(30)]);
    }

    #[test]
    fn stopped_runtime_units_are_started_without_restarting_the_vm() {
        let mut attempts = 0;
        let mut unit_starts = 0;
        let started = ensure_running_profile_runtime_ready(
            "effigy",
            Duration::from_secs(30),
            |timeout| {
                attempts += 1;
                if attempts == 1 {
                    Err("containerd socket missing".to_owned())
                } else {
                    assert_eq!(timeout, COLIMA_UNIT_START_TIMEOUT);
                    Ok(())
                }
            },
            || {
                unit_starts += 1;
                Ok(())
            },
        )
        .expect("unit start recovers the runtime");

        assert!(started);
        assert_eq!(unit_starts, 1);
        assert_eq!(attempts, 2);
    }

    #[test]
    fn runtime_that_survives_the_bounded_unit_start_is_refused_not_restarted() {
        let mut attempts = 0;
        let mut unit_starts = 0;
        let error = ensure_running_profile_runtime_ready(
            "effigy",
            Duration::from_secs(30),
            |_| {
                attempts += 1;
                Err("nerdctl info cannot reach containerd".to_owned())
            },
            || {
                unit_starts += 1;
                Ok(())
            },
        )
        .expect_err("a runtime that stays unavailable is refused");

        assert_eq!(unit_starts, 1, "the bounded unit start runs exactly once");
        assert_eq!(attempts, 2, "both runtime probes are bounded observations");
        let rendered = error.to_string();
        assert!(
            rendered.contains("`effigy`"),
            "names the profile: {rendered}"
        );
        assert!(
            rendered.contains("did not stop or restart the profile"),
            "the refusal states what it did not do: {rendered}"
        );
        assert!(
            rendered.contains("effigy container profile repair"),
            "names the explicit in-place recovery: {rendered}"
        );
        assert!(
            rendered.contains("nerdctl info cannot reach containerd"),
            "keeps the last runtime diagnostic: {rendered}"
        );
    }

    #[test]
    fn failed_unit_start_is_refused_without_a_vm_restart() {
        let error = ensure_running_profile_runtime_ready(
            "effigy",
            Duration::from_secs(30),
            |_| Err("buildctl cannot reach BuildKit".to_owned()),
            || Err("systemctl start containerd buildkit timed out".to_owned()),
        )
        .expect_err("a refused unit start does not fall back to a VM restart");

        let rendered = error.to_string();
        assert!(
            rendered.contains("systemctl start containerd buildkit timed out"),
            "names the unit-start failure: {rendered}"
        );
        assert!(
            rendered.contains("effigy container profile repair"),
            "names the explicit in-place recovery: {rendered}"
        );
    }

    #[test]
    fn runtime_readiness_probe_is_profile_scoped_nerdctl_info() {
        let args = colima_runtime_probe_args(&test_policy());
        let rendered = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec!["nerdctl", "--profile", "effigy", "--", "info"]
        );
    }

    #[test]
    fn buildkit_readiness_probe_uses_profile_scoped_buildctl_workers() {
        let args = colima_buildkit_probe_args(&test_policy());
        let rendered = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec![
                "ssh",
                "--profile",
                "effigy",
                "--",
                "sudo",
                "buildctl",
                "debug",
                "workers"
            ]
        );
    }

    /// Fake `colima` for the managed-profile entry point: `listed` is the
    /// profile's presence in `colima list`, `running` its VM state, and every
    /// invocation is recorded in order.
    const MANAGED_PROFILE_FAKE_COLIMA: &str = r#"#!/bin/sh
set -eu
root="$EFFIGY_MANAGED_PROFILE_FIXTURE"
printf '%s\n' "$*" >> "$root/colima-calls"
case "$1" in
  status)
    if [ -f "$root/status-unknown" ]; then
      printf 'status unavailable\n' >&2
      exit 1
    elif [ -f "$root/running" ]; then
      printf 'INFO[0000] colima is running\n'
    else
      printf 'time="now" level=fatal msg="colima [profile=effigy] is not running"\n'
      exit 1
    fi
    ;;
  list)
    if [ -f "$root/listed" ]; then
      if [ -f "$root/running" ]; then state=Running; else state=Stopped; fi
      if [ -f "$root/small" ]; then memory=8589934592; disk=1073741824; else memory=34359738368; disk=322122547200; fi
      printf '{"name":"effigy","status":"%s","cpus":2,"memory":%s,"disk":%s}\n' "$state" "$memory" "$disk"
    fi
    ;;
  start) : > "$root/running"; : > "$root/listed"; printf 'started\n' ;;
  stop) if [ ! -f "$root/stop-stays-running" ]; then rm -f "$root/running"; fi; printf 'stopped\n' ;;
  nerdctl)
    if [ -f "$root/offline" ]; then
      printf '%s\n' 'failed to solve: rust:1.88-bookworm: failed to resolve source metadata for docker.io/library/rust:1.88-bookworm: dial tcp: lookup registry-1.docker.io on 192.168.5.3:53: read udp: connection refused' >&2
      exit 1
    fi
    exit 0
    ;;
  ssh) exit 0 ;;
  *) printf 'unexpected colima invocation: %s\n' "$*" >&2; exit 1 ;;
esac
"#;

    struct EnvRestore(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl EnvRestore {
        fn set(vars: &[(&'static str, std::ffi::OsString)]) -> Self {
            let saved = vars
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            for (key, value) in vars {
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            Self(saved)
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => unsafe {
                        std::env::set_var(key, value);
                    },
                    None => unsafe {
                        std::env::remove_var(key);
                    },
                }
            }
        }
    }

    struct ManagedProfileFixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        home: std::path::PathBuf,
        _env: EnvRestore,
    }

    impl ManagedProfileFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path().canonicalize().expect("canonical fixture");
            let bin = root.join("bin");
            let home = root.join("home");
            std::fs::create_dir_all(&bin).expect("mkdir bin");
            std::fs::create_dir_all(&home).expect("mkdir home");
            let colima = bin.join("colima");
            std::fs::write(&colima, MANAGED_PROFILE_FAKE_COLIMA).expect("write fake colima");
            let mut permissions = std::fs::metadata(&colima).expect("metadata").permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
            std::fs::set_permissions(&colima, permissions).expect("chmod fake colima");
            let path = format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let env = EnvRestore::set(&[
                ("PATH", path.into()),
                ("HOME", home.clone().into_os_string()),
                ("EFFIGY_COMPOSE_BACKEND", "colima".into()),
                (
                    "EFFIGY_MANAGED_PROFILE_FIXTURE",
                    root.clone().into_os_string(),
                ),
                ("EFFIGY_INTERNAL_HOST_MEMORY_BYTES", "137438953472".into()),
            ]);
            Self {
                _dir: dir,
                root,
                home,
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            let mut policy = test_policy();
            policy.repo_root = self.root.clone();
            policy
        }

        fn state(&self, name: &str) {
            std::fs::write(self.root.join(name), "").expect("fixture state");
        }

        fn has_state(&self, name: &str) -> bool {
            self.root.join(name).is_file()
        }

        fn write_colima_config(&self, body: &str) {
            let profile = self.root.join("home/.colima/effigy");
            std::fs::create_dir_all(&profile).expect("mkdir profile");
            std::fs::write(profile.join("colima.yaml"), body).expect("write colima config");
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.root.join("colima-calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }
    }

    #[test]
    fn colima_sizing_warnings_require_a_selected_workspace_service() {
        use crate::policy::render::{Draft, Request};

        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("small");
        let mut policy = fixture.policy();
        let compose = fixture.root.join("compose.yml");
        policy.compose_files = vec![compose.clone()];
        let mut draft = Draft::default();
        draft.insert(
            compose,
            b"services:\n  app:\n    image: example\n  db:\n    image: postgres\n".to_vec(),
        );
        draft.finish(&mut policy, "fixture").expect("render");

        // A catalog service or default identity alone does not prove a workspace.
        policy.workspace_user = Some("dev".into());
        policy.workspace_home = Some("/home/dev".into());
        assert!(colima_profile_warnings(&policy, &fixture.root).is_empty());
        policy.host_processes.push(crate::EffectiveHostProcess {
            name: "listener".into(),
            run: "serve".into(),
            cwd: fixture.root.clone(),
            env: vec![],
            depends_on: vec![],
            listener: Some(crate::EffectiveManagedHostListener {
                bind: "127.0.0.1:3000".into(),
                readiness_path: "/".into(),
                readiness_status: 200,
                readiness_timeout_secs: 1,
                declared_route_domain: "app.test".into(),
                route_domain: "app.test".into(),
                route_tls: false,
            }),
            restart: Default::default(),
            restart_delay_ms: 0,
            shutdown_signal: Default::default(),
            shutdown_grace_secs: 1,
        });
        assert!(colima_profile_warnings(&policy, &fixture.root).is_empty());
        policy.primary_service = "db".into();
        assert!(colima_profile_warnings(&policy, &fixture.root).is_empty());

        policy.primary_service = "app".into();
        policy.render.as_mut().unwrap().request = Request::Named(Some(Default::default()));
        let warnings = colima_profile_warnings(&policy, &fixture.root);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("memory and")));
        assert!(warnings.iter().any(|warning| warning.contains("disk for")));
        policy.primary_service = "missing".into();
        assert!(colima_profile_warnings(&policy, &fixture.root).is_empty());

        // An unrendered direct Compose policy is unknown, even with an identity.
        policy.render = None;
        policy.compose_source = EffectiveComposeSource::Direct;
        assert!(colima_profile_warnings(&policy, &fixture.root).is_empty());
        policy.profile = "default".into();
        let warnings = colima_profile_warnings(&policy, &fixture.root);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("shared with unrelated"));
    }

    #[test]
    fn colima_runtime_explicit_repair_and_reset_use_only_graceful_colima_stop() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let policy = fixture.policy();
        recover_colima_runtime(&policy, &fixture.root).expect("repair profile");
        reset_colima_runtime(&policy, &fixture.root).expect("reset profile");
        let calls = fixture.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|call| *call == "stop --profile effigy")
                .count(),
            2,
            "{calls:#?}"
        );
        assert!(
            !calls.iter().any(|call| call.contains("--force")),
            "{calls:#?}"
        );
        assert!(!fixture.has_state("running"));
    }

    /// The recovery reports each phase it puts in flight, in order, so the
    /// owning command plane can name the command actually running instead of a
    /// generic recovery wait (Q-017). The fixture's fake `colima` is the only
    /// executable on the path, so nothing here touches a real runtime.
    #[test]
    fn a_recovery_and_a_reset_report_each_phase_they_put_in_flight() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let policy = fixture.policy();

        let mut recovered = Vec::new();
        recover_colima_runtime_with_progress(&policy, &fixture.root, &mut |phase| {
            recovered.push(phase)
        })
        .expect("repair profile");
        assert_eq!(
            recovered,
            vec![
                ColimaRecoveryPhase::Stopping,
                ColimaRecoveryPhase::TerminatingProcesses,
                ColimaRecoveryPhase::VerifyingStopped,
                ColimaRecoveryPhase::Configuring,
                ColimaRecoveryPhase::Restarting,
                ColimaRecoveryPhase::VerifyingHealth,
            ],
            "configuration preparation is its own phase, before the start"
        );

        fixture.state("running");
        let mut reset = Vec::new();
        reset_colima_runtime_with_progress(&policy, &fixture.root, &mut |phase| reset.push(phase))
            .expect("reset profile");
        assert_eq!(
            reset,
            vec![
                ColimaRecoveryPhase::Stopping,
                ColimaRecoveryPhase::TerminatingProcesses,
                ColimaRecoveryPhase::VerifyingStopped,
            ]
        );
    }

    #[test]
    fn colima_runtime_refuses_running_or_unknown_status() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("stop-stays-running");
        let policy = fixture.policy();
        let error = recover_colima_runtime(&policy, &fixture.root).unwrap_err();
        assert!(error.to_string().contains("still running"));
        assert!(!fixture
            .calls()
            .iter()
            .any(|call| call.starts_with("start ")));
        fixture.state("status-unknown");
        let error = reset_colima_runtime(&policy, &fixture.root).unwrap_err();
        assert!(error.to_string().contains("status is unproven"));
    }

    /// An offline registry lookup used to take the shared VM down and start
    /// it again, which stops every other checkout's stack. A detected problem
    /// is now reported with the manual command and nothing else (effigy#371).
    #[test]
    fn an_offline_registry_failure_never_stops_the_shared_profile() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("offline");
        let policy = fixture.policy();
        let args = ["up", "-d"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();

        let failure = crate::exec::run_compose_capture(&fixture.root, &policy, &args, "compose up")
            .expect_err("the registry failure is reported as it stands");
        assert!(
            failure.to_string().contains("registry-1.docker.io"),
            "the runtime's own cause reaches the caller: {failure}"
        );
        let calls = fixture.calls();
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "Effigy never stops the shared profile: {calls:#?}"
        );
        assert!(
            !calls.iter().any(|call| call.starts_with("start ")),
            "Effigy never restarts the shared profile: {calls:#?}"
        );
    }

    #[test]
    fn absent_managed_profile_is_created_with_the_managed_arguments() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        let policy = fixture.policy();

        let report = ensure_colima_running_report(&policy, &fixture.root).expect("create profile");

        assert!(report.started);
        assert!(report.profile_created_or_restarted);
        assert!(fixture.has_state("running"));
        let calls = fixture.calls();
        let starts = calls
            .iter()
            .filter(|call| call.starts_with("start "))
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 1, "one managed start: {calls:#?}");
        let start = starts[0];
        for expected in [
            "--profile effigy",
            "--ssh-agent",
            "--dns 1.1.1.1",
            "--dns 8.8.8.8",
            "--memory 32",
            "--disk 300",
        ] {
            assert!(start.contains(expected), "{expected} missing from {start}");
        }
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "a missing profile is created, never stopped: {calls:#?}"
        );
    }

    #[test]
    fn stopped_managed_profile_is_started_without_being_recreated() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        let policy = fixture.policy();

        let report = ensure_colima_running_report(&policy, &fixture.root).expect("start profile");

        assert!(report.started);
        assert!(!report.profile_created_or_restarted);
        assert!(fixture.has_state("running"));
        let calls = fixture.calls();
        assert!(
            calls
                .iter()
                .any(|call| call.starts_with("start --profile effigy")),
            "{calls:#?}"
        );
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "{calls:#?}"
        );
    }

    #[test]
    fn running_managed_profile_with_drifted_config_warns_without_touching_the_vm() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.write_colima_config("forwardAgent: false\n");
        let policy = fixture.policy();

        let report = ensure_colima_running_report(&policy, &fixture.root).expect("reuse profile");

        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with("start ") || call.starts_with("stop ")),
            "a running shared profile is never stopped or restarted: {calls:#?}"
        );
        // The managed settings are written so the operator's own
        // `colima restart effigy` applies them.
        let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("managed config");
        assert!(config.contains("forwardAgent: true"), "{config}");
    }

    #[test]
    fn configured_running_managed_profile_is_reused_without_a_restart() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.write_colima_config("forwardAgent: true\n");
        let policy = fixture.policy();

        let report = ensure_colima_running_report(&policy, &fixture.root).expect("reuse profile");

        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert!(
            !calls
                .iter()
                .any(|call| call.starts_with("start ") || call.starts_with("stop ")),
            "a configured running profile is not restarted: {calls:#?}"
        );
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: vec![],
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            runtime_host_key: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }
}
