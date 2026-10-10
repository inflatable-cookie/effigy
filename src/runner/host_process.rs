//! Host-side processes that follow container lifecycle.
//!
//! Each `[[containers.<name>.host_processes]]` entry resolves to an
//! `EffectiveHostProcess`. Effigy runs one detached supervisor per entry.
//! Listener processes additionally write a versioned endpoint report; the
//! supervisor proves the reported socket belongs to that exact child
//! generation, waits for HTTP readiness, and publishes an owned route. State,
//! logs, reports, specs, and process identities live under the checkout's
//! runtime directory, with listener state separated by container profile.
//! Shutdown withdraws the exact route generation before stopping the recorded
//! child process group.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use effigy_cli::{InternalHostProcessStopArgs, InternalHostProcessSuperviseArgs};
#[cfg(test)]
use effigy_containers::HostProcessSignal;
use effigy_containers::{
    EffectiveContainerPolicy, EffectiveHostProcess, EffectiveManagedHostListener,
};
use effigy_gateway::identity::{read_live_process_identity, GatewayStartIdentity};
use effigy_gateway::legacy::{process_listening_endpoints, GatewayEndpoint, GatewayTransport};
use effigy_gateway::registration::{
    check_managed_listener_route_claim, deregister_managed_listener_route, project_scope,
    register_managed_listener_route,
};
use effigy_gateway::routes::{ManagedListenerRouteOwner, RouteTable};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::error::RunnerError;

use super::managed_listener_readiness::{probe_http_readiness, HttpReadinessOutcome};

const HOST_PROCESS_DIR: &str = ".effigy/runtime/host-processes";
const HOST_PROCESS_SPEC_SCHEMA: &str = "effigy.managed.host-process-spec.v1";
const HOST_PROCESS_RECORD_SCHEMA: &str = "effigy.managed.host-process-record.v1";
const HOST_LISTENER_REPORT_SCHEMA: &str = "effigy.managed.host-listener-report.v1";
const HOST_LISTENER_STATE_SCHEMA: &str = "effigy.managed.host-listener-state.v1";
const HOST_LISTENER_BIND_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_BIND";
const HOST_LISTENER_REPORT_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE";
const HOST_LISTENER_GENERATION_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_GENERATION";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessRuntimeSpec {
    schema: String,
    project_path: String,
    runtime_generation: String,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    listener: Option<EffectiveManagedHostListener>,
    owner: String,
    dependencies: Vec<HostProcessDependency>,
    listener_state_file: PathBuf,
    gateway_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessDependency {
    name: String,
    owner: String,
    state_file: PathBuf,
    route_table_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessRecord {
    schema: String,
    pid: u32,
    boot_identity: String,
    start_identity: GatewayStartIdentity,
}

struct HostProcessLifecycleLock(std::fs::File);

impl HostProcessLifecycleLock {
    fn acquire(dir: &Path) -> Result<Self, RunnerError> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = dir.join("lifecycle.lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if result != 0 {
            return Err(RunnerError::task_invocation(format!(
                "cannot lock managed host process lifecycle {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self(file))
    }
}

impl Drop for HostProcessLifecycleLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostListenerReport {
    schema: String,
    generation: String,
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostListenerState {
    schema: String,
    owner: String,
    runtime_generation: String,
    generation: String,
    status: String,
    address: Option<String>,
    internal_url: Option<String>,
    public_url: Option<String>,
    route_domain: Option<String>,
    supervisor_pid: u32,
    supervisor_boot_identity: String,
    supervisor_start_identity: GatewayStartIdentity,
    child_pid: Option<u32>,
    child_boot_identity: Option<String>,
    child_start_identity: Option<GatewayStartIdentity>,
    listener_pid: Option<u32>,
    listener_boot_identity: Option<String>,
    listener_start_identity: Option<GatewayStartIdentity>,
    route_owner: Option<ManagedListenerRouteOwner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diagnostic: Option<HostListenerDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct ManagedHostListenerResult {
    pub(in crate::runner) name: String,
    pub(in crate::runner) status: String,
    pub(in crate::runner) runtime_generation: String,
    pub(in crate::runner) generation: String,
    pub(in crate::runner) address: String,
    pub(in crate::runner) internal_url: String,
    pub(in crate::runner) public_url: String,
    pub(in crate::runner) route_domain: String,
    pub(in crate::runner) route_tls: bool,
    pub(in crate::runner) state_file: String,
}

/// Reject listener configurations and checkout paths before container or
/// child startup can create runtime effects.
pub(in crate::runner) fn validate_host_process_preflight(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(());
    }
    let checkout_root = repo_root.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!("failed to resolve checkout: {error}"))
    })?;
    let ordered = order_host_processes(&policy.host_processes)?;
    for process in &ordered {
        resolve_host_process_cwd(&checkout_root, process)?;
    }
    let listener_root = private_gateway_root(policy)?;
    if let Some(root) = listener_root {
        let route_table = root.join("routes.json");
        let project_path = checkout_root.display().to_string();
        for process in ordered.iter().filter(|process| process.listener.is_some()) {
            let listener = process.listener.as_ref().expect("filtered listener");
            check_managed_listener_route_claim(
                &route_table,
                &project_path,
                &listener.route_domain,
                &managed_host_process_owner(&project_path, policy, process)?,
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
    }
    Ok(())
}

fn resolve_host_process_cwd(
    checkout_root: &Path,
    process: &EffectiveHostProcess,
) -> Result<PathBuf, RunnerError> {
    if process.cwd.as_os_str().is_empty() {
        return Ok(checkout_root.to_path_buf());
    }
    let requested = checkout_root.join(&process.cwd);
    let cwd = requested.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!(
            "managed host process `{}` working directory {} is unavailable: {error}",
            process.name,
            requested.display()
        ))
    })?;
    if !cwd.starts_with(checkout_root) {
        return Err(RunnerError::task_invocation(format!(
            "managed host process `{}` working directory escapes its checkout",
            process.name
        )));
    }
    Ok(cwd)
}

/// Spawn one detached supervisor per entry in `policy.host_processes`.
///
/// Idempotent in the sense that each call writes a fresh `<name>.pid`.
/// Stale PID files from previous runs are reaped before spawning.
pub(in crate::runner) fn start_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<ManagedHostListenerResult>, RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(Vec::new());
    }

    validate_host_process_preflight(repo_root, policy)?;

    let listener_root = private_gateway_root(policy)?;
    let route_table = listener_root
        .as_deref()
        .map(|root| root.join("routes.json"));
    let checkout_root = repo_root.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!("failed to resolve checkout: {error}"))
    })?;
    let project_path = checkout_root.display().to_string();
    let ordered = order_host_processes(&policy.host_processes)?;
    let dir = host_process_dir_for_policy(&checkout_root, policy);
    create_host_process_dir(&checkout_root, &dir)?;
    let lifecycle_lock = HostProcessLifecycleLock::acquire(&dir)?;
    if let Some(route_table) = route_table.as_deref() {
        for process in policy
            .host_processes
            .iter()
            .filter(|process| process.listener.is_some())
        {
            let owner = managed_host_process_owner(&project_path, policy, process)?;
            check_managed_listener_route_claim(
                route_table,
                &project_path,
                &process
                    .listener
                    .as_ref()
                    .expect("filtered listener")
                    .route_domain,
                &owner,
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
    }
    let mut started = Vec::new();
    let mut listener_results = Vec::new();
    let by_name = policy
        .host_processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<HashMap<_, _>>();
    let start_result = (|| -> Result<(), RunnerError> {
        for hp in ordered {
            reap_stale_supervisor(&dir, hp)?;
            let env = hp.env.iter().cloned().collect::<BTreeMap<_, _>>();
            let mut dependencies = Vec::new();
            for dependency in &hp.depends_on {
                let dependency_process = by_name.get(dependency.as_str()).ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "host process `{}` depends on missing process `{dependency}`",
                        hp.name
                    ))
                })?;
                let state_path = host_process_state_path(&dir, &dependency_process.name);
                let state = wait_for_listener_state(
                    &state_path,
                    dependency,
                    &managed_host_process_owner(&project_path, policy, dependency_process)?,
                    dependency_process
                        .listener
                        .as_ref()
                        .map(|listener| Duration::from_secs(listener.readiness_timeout_secs))
                        .unwrap_or(Duration::from_secs(60)),
                    route_table.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener dependency has no gateway route table",
                        )
                    })?,
                )?;
                if state.internal_url.is_none() || state.public_url.is_none() {
                    return Err(RunnerError::task_invocation(format!(
                    "managed host listener `{dependency}` is ready without current endpoint URLs"
                )));
                }
                dependencies.push(HostProcessDependency {
                    name: dependency.clone(),
                    owner: managed_host_process_owner(&project_path, policy, dependency_process)?,
                    state_file: state_path,
                    route_table_file: route_table.clone().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener dependency has no gateway route table",
                        )
                    })?,
                });
            }
            let spec = HostProcessRuntimeSpec {
                schema: HOST_PROCESS_SPEC_SCHEMA.to_owned(),
                project_path: project_path.clone(),
                runtime_generation: random_token()?,
                cwd: resolve_host_process_cwd(&checkout_root, hp)?,
                env,
                listener: hp.listener.clone(),
                owner: managed_host_process_owner(&project_path, policy, hp)?,
                dependencies,
                listener_state_file: host_process_state_path(&dir, &hp.name),
                gateway_root: listener_root.clone(),
            };
            let spec_path = host_process_spec_path(&dir, &hp.name);
            write_private_json(&spec_path, &spec)?;
            spawn_supervisor(repo_root, &policy.name, &dir, hp, &spec_path)?;
            started.push(hp.name.clone());
            wait_for_supervisor_record(
                &dir.join(format!("{}.pid", sanitize(&hp.name))),
                &hp.name,
                Duration::from_secs(5),
            )?;
            if let Some(listener) = &hp.listener {
                let state = wait_for_listener_state(
                    &host_process_state_path(&dir, &hp.name),
                    &hp.name,
                    &managed_host_process_owner(&project_path, policy, hp)?,
                    Duration::from_secs(listener.readiness_timeout_secs.saturating_add(5)),
                    route_table.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "managed host listener has no gateway route table",
                        )
                    })?,
                )?;
                listener_results.push(ManagedHostListenerResult {
                    name: hp.name.clone(),
                    status: state.status,
                    runtime_generation: state.runtime_generation,
                    generation: state.generation,
                    address: state.address.unwrap_or_default(),
                    internal_url: state.internal_url.unwrap_or_default(),
                    public_url: state.public_url.unwrap_or_default(),
                    route_domain: state.route_domain.unwrap_or_default(),
                    route_tls: listener.route_tls,
                    state_file: host_process_state_path(&dir, &hp.name)
                        .display()
                        .to_string(),
                });
            }
        }
        Ok(())
    })();
    if let Err(error) = start_result {
        drop(lifecycle_lock);
        let stopped = stop_host_processes_for_container(repo_root, policy);
        let missing = started
            .iter()
            .filter(|name| {
                !stopped.iter().any(|stopped_name| stopped_name == *name)
                    && dir.join(format!("{}.pid", sanitize(name))).exists()
            })
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Err(error);
        }
        return Err(RunnerError::task_invocation(format!(
            "{error}\nmanaged host startup rollback could not confirm stop for: {}",
            missing.join(", ")
        )));
    }
    Ok(listener_results)
}

/// Stop every supervisor recorded for this container. Best-effort:
/// never returns an error — host-process shutdown should not block
/// container shutdown.
pub(in crate::runner) fn stop_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Vec<String> {
    if policy.host_processes.is_empty() {
        return Vec::new();
    }
    let dir = host_process_dir_for_policy(repo_root, policy);
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(_lifecycle_lock) = HostProcessLifecycleLock::acquire(&dir) else {
        return Vec::new();
    };
    let mut ordered = order_host_processes(&policy.host_processes)
        .unwrap_or_else(|_| policy.host_processes.iter().collect::<Vec<_>>());
    ordered.reverse();
    let mut stopped = Vec::new();
    for hp in ordered {
        let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
        if !pid_path.exists() {
            continue;
        }
        let signal_name = hp.shutdown_signal.as_str();
        let mut cmd = ProcessCommand::new(match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => continue,
        });
        cmd.arg("__host-process-stop")
            .arg("--pid-file")
            .arg(&pid_path)
            .arg("--signal")
            .arg(signal_name)
            .arg("--grace-secs")
            .arg(hp.shutdown_grace_secs.to_string())
            .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Synchronous so the next compose-down step sees a clean
        // process tree.
        if cmd.status().is_ok_and(|status| status.success()) {
            stopped.push(hp.name.clone());
        }
    }
    stopped
}

fn spawn_supervisor(
    repo_root: &Path,
    container_name: &str,
    dir: &Path,
    hp: &EffectiveHostProcess,
    spec_file: &Path,
) -> Result<(), RunnerError> {
    let executable = std::env::current_exe().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to resolve current executable for host-process supervisor: {error}"
        ))
    })?;
    let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
    let log_path = dir.join(format!("{}.log", sanitize(&hp.name)));
    let mut child = ProcessCommand::new(executable);
    child
        .arg("__host-process-supervise")
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--container")
        .arg(container_name)
        .arg("--name")
        .arg(&hp.name)
        .arg("--run")
        .arg(&hp.run)
        .arg("--pid-file")
        .arg(&pid_path)
        .arg("--log-file")
        .arg(&log_path)
        .arg("--spec-file")
        .arg(spec_file)
        .arg("--restart")
        .arg(hp.restart.as_str())
        .arg("--restart-delay-ms")
        .arg(hp.restart_delay_ms.to_string())
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Detach into a new session so the supervisor outlives the
    // bring-up command.
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setsid` is
    // async-signal-safe. Failure is ignored when the child is already a
    // session leader, and the callback returns `Ok(())` without allocating,
    // formatting, logging, or reading the environment.
    unsafe {
        child.pre_exec(|| {
            let _ = nix::unistd::setsid();
            Ok(())
        });
    }
    child
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "__host-process-supervise".to_owned(),
            error,
        })?;
    Ok(())
}

fn reap_stale_supervisor(dir: &Path, hp: &EffectiveHostProcess) -> Result<(), RunnerError> {
    let pid_path = dir.join(format!("{}.pid", sanitize(&hp.name)));
    match fs::symlink_metadata(&pid_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if hp.listener.is_some() {
                let state_path = host_process_state_path(dir, &hp.name);
                match fs::symlink_metadata(&state_path) {
                    Ok(metadata) => {
                        if metadata.file_type().is_symlink() || !metadata.is_file() {
                            return Err(RunnerError::task_invocation(format!(
                                "managed host listener `{}` has an unknown state record without its supervisor identity",
                                hp.name
                            )));
                        }
                        let state: HostListenerState = read_private_json(&state_path)?;
                        if state.schema != HOST_LISTENER_STATE_SCHEMA
                            || !matches!(state.status.as_str(), "stopped" | "failed")
                            || state.address.is_some()
                            || state.internal_url.is_some()
                            || state.public_url.is_some()
                            || state.route_owner.is_some()
                            || state.child_pid.is_some()
                            || state.child_boot_identity.is_some()
                            || state.child_start_identity.is_some()
                            || state.listener_pid.is_some()
                            || state.listener_boot_identity.is_some()
                            || state.listener_start_identity.is_some()
                        {
                            return Err(RunnerError::task_invocation(format!(
                                "managed host listener `{}` has an unpaired or unavailable generation record; refusing to overwrite it",
                                hp.name
                            )));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(RunnerError::task_invocation_failed_read(&state_path, error))
                    }
                }
            }
            return Ok(());
        }
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&pid_path, error)),
    }
    let record: HostProcessRecord = read_private_json(&pid_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "host process `{}` has an unknown supervisor record at {}: {error}",
            hp.name,
            pid_path.display()
        ))
    })?;
    if record.schema != HOST_PROCESS_RECORD_SCHEMA {
        return Err(RunnerError::task_invocation(format!(
            "host process `{}` has unsupported supervisor record schema `{}`",
            hp.name, record.schema
        )));
    }
    let stopped = stop_recorded_supervisor(
        &pid_path,
        &record,
        parse_signal_name(hp.shutdown_signal.as_str()),
        hp.shutdown_grace_secs,
    )?;
    if !stopped {
        return Err(RunnerError::task_invocation(format!(
            "host process `{}` supervisor record is held because its exact process generation is absent",
            hp.name
        )));
    }
    cleanup_stopped_listener_routes(&pid_path, &record)?;
    remove_host_process_record_if_matches(&pid_path, &record)?;
    Ok(())
}

fn wait_for_supervisor_record(
    path: &Path,
    process_name: &str,
    timeout: Duration,
) -> Result<HostProcessRecord, RunnerError> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host process record for `{process_name}` is not a regular file"
                    )));
                }
                let record: HostProcessRecord = read_private_json(path)?;
                if record.schema != HOST_PROCESS_RECORD_SCHEMA {
                    return Err(RunnerError::task_invocation(format!(
                        "managed host process `{process_name}` wrote an unsupported supervisor record"
                    )));
                }
                verify_supervisor_identity(&record)?;
                return Ok(record);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(RunnerError::task_invocation_failed_read(path, error)),
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "timed out waiting for managed host process `{process_name}` to record its supervisor generation"
            )));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn host_process_dir(repo_root: &Path, container_name: &str) -> PathBuf {
    repo_root
        .join(HOST_PROCESS_DIR)
        .join(sanitize(container_name))
}

fn managed_host_process_dir(repo_root: &Path, container_name: &str, profile: &str) -> PathBuf {
    host_process_dir(repo_root, container_name).join(sanitize(profile))
}

fn host_process_dir_for_policy(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    if policy
        .host_processes
        .iter()
        .any(|process| process.listener.is_some())
    {
        managed_host_process_dir(repo_root, &policy.name, &policy.profile)
    } else {
        host_process_dir(repo_root, &policy.name)
    }
}

fn create_host_process_dir(repo_root: &Path, dir: &Path) -> Result<(), RunnerError> {
    let relative = dir.strip_prefix(repo_root).map_err(|_| {
        RunnerError::task_invocation(
            "managed host process runtime directory is outside its canonical checkout",
        )
    })?;
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(RunnerError::task_invocation(
                "managed host process runtime directory contains an unsafe path component",
            ));
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(RunnerError::task_invocation(format!(
                    "managed host process runtime path {} is not a real directory",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current).map_err(|error| {
                            RunnerError::task_invocation_failed_read(&current, error)
                        })?;
                        if metadata.file_type().is_symlink() || !metadata.is_dir() {
                            return Err(RunnerError::task_invocation(format!(
                                "managed host process runtime path {} is not a real directory",
                                current.display()
                            )));
                        }
                    }
                    Err(error) => {
                        return Err(RunnerError::task_invocation_failed_write(&current, error));
                    }
                }
            }
            Err(error) => {
                return Err(RunnerError::task_invocation_failed_read(&current, error));
            }
        }
    }
    if fs::canonicalize(dir).ok().as_deref() != Some(dir) {
        return Err(RunnerError::task_invocation(format!(
            "managed host process runtime directory {} is not canonical",
            dir.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| RunnerError::task_invocation_failed_write(dir, error))?;
    }
    Ok(())
}

fn host_process_state_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.listener.json", sanitize(name)))
}

fn host_process_spec_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.spec.json", sanitize(name)))
}

fn private_gateway_root(policy: &EffectiveContainerPolicy) -> Result<Option<PathBuf>, RunnerError> {
    if !policy
        .host_processes
        .iter()
        .any(|process| process.listener.is_some())
    {
        return Ok(None);
    }
    let Some(root) = std::env::var_os(effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV) else {
        return Err(RunnerError::task_invocation(format!(
            "managed host listener routes require `{}` to select a caller-owned private gateway state root",
            effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV
        )));
    };
    let root = PathBuf::from(root);
    effigy_gateway::private_state::validate_root(&root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Some(root))
}

fn managed_host_process_owner(
    project_path: &str,
    policy: &EffectiveContainerPolicy,
    process: &EffectiveHostProcess,
) -> Result<String, RunnerError> {
    let scope = project_scope(project_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .unwrap_or_else(|| "main".to_owned());
    Ok(format!(
        "{scope}|{}|{}|{}",
        policy.name, policy.profile, process.name
    ))
}

fn order_host_processes(
    processes: &[EffectiveHostProcess],
) -> Result<Vec<&EffectiveHostProcess>, RunnerError> {
    let by_name = processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::with_capacity(processes.len());
    let mut visiting = HashSet::new();
    let mut complete = HashSet::new();
    fn visit<'a>(
        process: &'a EffectiveHostProcess,
        by_name: &HashMap<&'a str, &'a EffectiveHostProcess>,
        visiting: &mut HashSet<String>,
        complete: &mut HashSet<String>,
        ordered: &mut Vec<&'a EffectiveHostProcess>,
    ) -> Result<(), RunnerError> {
        if complete.contains(&process.name) {
            return Ok(());
        }
        if !visiting.insert(process.name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "managed host process dependency cycle includes `{}`",
                process.name
            )));
        }
        for dependency in &process.depends_on {
            let target = by_name.get(dependency.as_str()).ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "host process `{}` depends on missing process `{dependency}`",
                    process.name
                ))
            })?;
            if target.listener.is_none() {
                return Err(RunnerError::task_invocation(format!(
                    "host process `{}` depends on `{dependency}`, which has no managed listener",
                    process.name
                )));
            }
            visit(target, by_name, visiting, complete, ordered)?;
        }
        visiting.remove(&process.name);
        complete.insert(process.name.clone());
        ordered.push(process);
        Ok(())
    }
    for process in processes {
        visit(
            process,
            &by_name,
            &mut visiting,
            &mut complete,
            &mut ordered,
        )?;
    }
    Ok(ordered)
}

fn dependency_env_prefix(name: &str) -> String {
    let normalized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("EFFIGY_MANAGED_HOST_{normalized}")
}

fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), RunnerError> {
    let rendered = serde_json::to_vec_pretty(value)
        .map_err(|error| RunnerError::task_invocation_failed_render(path, error))?;
    let temp = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        random_token()?
    ));
    fs::write(&temp, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&temp, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
            .map_err(|error| RunnerError::task_invocation_failed_write(&temp, error))?;
    }
    fs::rename(&temp, path).map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

fn wait_for_listener_state(
    path: &Path,
    process_name: &str,
    owner: &str,
    timeout: Duration,
    route_table_path: &Path,
) -> Result<HostListenerState, RunnerError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(RunnerError::task_invocation(format!(
                    "managed host listener state for `{process_name}` is not a regular file"
                )));
            }
            let raw = fs::read(path)
                .map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
            let state: HostListenerState = serde_json::from_slice(&raw)
                .map_err(|error| RunnerError::task_invocation_failed_parse(path, error))?;
            if state.schema != HOST_LISTENER_STATE_SCHEMA {
                return Err(RunnerError::task_invocation(format!(
                    "managed host listener state for `{process_name}` has unsupported schema `{}`",
                    state.schema
                )));
            }
            if state.owner != owner {
                return Err(RunnerError::task_invocation(format!(
                    "managed host listener state for `{process_name}` belongs to a different checkout/profile owner"
                )));
            }
            match state.status.as_str() {
                "ready" => {
                    let route_owner = state.route_owner.as_ref().ok_or_else(|| {
                        RunnerError::task_invocation(format!(
                            "managed host listener state for `{process_name}` is ready without an owned route identity"
                        ))
                    })?;
                    effigy_gateway::managed_listener::verify_managed_listener_owner(route_owner)
                        .map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "managed host listener `{process_name}` ownership is unavailable: {error}"
                            ))
                        })?;
                    let public_scheme = if route_owner.route_tls {
                        "https"
                    } else {
                        "http"
                    };
                    let expected_public = state
                        .route_domain
                        .as_deref()
                        .map(|domain| format!("{public_scheme}://{domain}"));
                    if state.internal_url.as_deref()
                        != Some(format!("http://{}", route_owner.address).as_str())
                        || state.address.as_deref() != Some(route_owner.address.as_str())
                        || state.runtime_generation != route_owner.runtime_generation
                        || route_owner.generation != state.generation
                        || state.public_url != expected_public
                    {
                        return Err(RunnerError::task_invocation(format!(
                            "managed host listener `{process_name}` has inconsistent endpoint URLs"
                        )));
                    }
                    if managed_route_matches(route_table_path, &state, owner)? {
                        return Ok(state);
                    }
                }
                "failed" => {
                    let detail = state
                        .diagnostic
                        .as_ref()
                        .map(|diagnostic| format!(" ({})", diagnostic.summary()))
                        .unwrap_or_default();
                    return Err(RunnerError::task_invocation(format!(
                        "managed host listener `{process_name}` failed before readiness{detail}"
                    )));
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "timed out after {}s waiting for managed host listener `{process_name}`",
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn dependency_states(
    dependencies: &[HostProcessDependency],
) -> Result<Option<Vec<HostListenerState>>, RunnerError> {
    let mut states = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let metadata = match fs::symlink_metadata(&dependency.state_file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(RunnerError::task_invocation_failed_read(
                    &dependency.state_file,
                    error,
                ))
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "managed dependency state for `{}` is not a regular file",
                dependency.name
            )));
        }
        let state: HostListenerState = read_private_json(&dependency.state_file)?;
        if state.schema != HOST_LISTENER_STATE_SCHEMA || state.owner != dependency.owner {
            return Err(RunnerError::task_invocation(format!(
                "managed dependency state for `{}` has an unsupported schema or owner",
                dependency.name
            )));
        }
        if state.status != "ready" {
            return Ok(None);
        }
        let Some(owner) = state.route_owner.as_ref() else {
            return Ok(None);
        };
        if owner.owner != dependency.owner
            || owner.runtime_generation != state.runtime_generation
            || owner.generation != state.generation
            || state.address.as_deref() != Some(owner.address.as_str())
            || state.internal_url.as_deref() != Some(format!("http://{}", owner.address).as_str())
            || state.public_url.as_deref()
                != Some(
                    format!(
                        "{}://{}",
                        if owner.route_tls { "https" } else { "http" },
                        state.route_domain.as_deref().unwrap_or_default()
                    )
                    .as_str(),
                )
        {
            return Err(RunnerError::task_invocation(format!(
                "managed dependency state for `{}` has inconsistent current addresses",
                dependency.name
            )));
        }
        if effigy_gateway::managed_listener::verify_managed_listener_owner(owner).is_err() {
            return Ok(None);
        }
        if !managed_route_matches(&dependency.route_table_file, &state, &dependency.owner)? {
            return Ok(None);
        }
        states.push(state);
    }
    Ok(Some(states))
}

fn managed_route_matches(
    route_table_path: &Path,
    state: &HostListenerState,
    stable_owner: &str,
) -> Result<bool, RunnerError> {
    let Some(owner) = state.route_owner.as_ref() else {
        return Ok(false);
    };
    let Some(domain) = state.route_domain.as_deref() else {
        return Ok(false);
    };
    let table = RouteTable::load(route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(table.lookup(domain).is_some_and(|route| {
        route.target.as_deref() == Some(owner.address.as_str())
            && route.tls == owner.route_tls
            && route.managed_listener.as_ref() == Some(owner)
            && owner.owner == stable_owner
            && owner.runtime_generation == state.runtime_generation
            && owner.generation == state.generation
    }))
}

type DependencyEnvironment = (BTreeMap<String, String>, Vec<String>);

fn dependency_environment(
    spec: &HostProcessRuntimeSpec,
    states: &[HostListenerState],
) -> DependencyEnvironment {
    let mut env = BTreeMap::new();
    let mut generations = Vec::with_capacity(states.len());
    for (dependency, state) in spec.dependencies.iter().zip(states) {
        let prefix = dependency_env_prefix(&dependency.name);
        env.insert(
            format!("{prefix}_INTERNAL_URL"),
            state.internal_url.clone().unwrap_or_default(),
        );
        env.insert(
            format!("{prefix}_PUBLIC_URL"),
            state.public_url.clone().unwrap_or_default(),
        );
        env.insert(
            format!("{prefix}_STATE_FILE"),
            dependency.state_file.display().to_string(),
        );
        env.insert(format!("{prefix}_GENERATION"), state.generation.clone());
        generations.push(format!(
            "{}:{}:{}:{}",
            dependency.name,
            state.runtime_generation,
            state.generation,
            state.address.as_deref().unwrap_or_default()
        ));
    }
    (env, generations)
}

fn wait_for_dependency_environment(
    spec: &HostProcessRuntimeSpec,
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<Option<DependencyEnvironment>, RunnerError> {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if let Some(states) = dependency_states(&spec.dependencies)? {
            return Ok(Some(dependency_environment(spec, &states)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Debug)]
enum HostChildWait {
    Exited(std::process::ExitStatus),
    DependencyChanged,
    Shutdown,
}

fn wait_for_host_child(
    child: &mut std::process::Child,
    spec: &HostProcessRuntimeSpec,
    expected_dependencies: &[String],
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<HostChildWait, RunnerError> {
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            RunnerError::task_invocation(format!(
                "managed host child completion is uncertain; keeping its generation held: {error}"
            ))
        })? {
            return Ok(HostChildWait::Exited(status));
        }
        if shutdown.load(Ordering::SeqCst) {
            return Ok(HostChildWait::Shutdown);
        }
        let Some(states) = dependency_states(&spec.dependencies)? else {
            return Ok(HostChildWait::DependencyChanged);
        };
        let (_, current) = dependency_environment(spec, &states);
        if current != expected_dependencies {
            return Ok(HostChildWait::DependencyChanged);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn random_token() -> Result<String, RunnerError> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot obtain a managed listener generation token: {error}"
            ))
        })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

// ---------- Internal subcommand entrypoints ----------

pub(in crate::runner) fn run_internal_host_process_supervise(
    args: InternalHostProcessSuperviseArgs,
) -> Result<String, RunnerError> {
    if let Some(parent) = args.pid_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    if let Some(parent) = args.log_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let spec: HostProcessRuntimeSpec = read_private_json(&args.spec_file)?;
    if spec.schema != HOST_PROCESS_SPEC_SCHEMA || !spec.cwd.is_absolute() {
        return Err(RunnerError::task_invocation(
            "managed host process has an unsupported or invalid runtime spec",
        ));
    }
    let supervisor_identity = read_live_process_identity(std::process::id()).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record supervisor identity: {error}"))
    })?;
    let supervisor_record = HostProcessRecord {
        schema: HOST_PROCESS_RECORD_SCHEMA.to_owned(),
        pid: std::process::id(),
        boot_identity: supervisor_identity.boot_identity.clone(),
        start_identity: supervisor_identity.start_identity.clone(),
    };
    write_private_json(&args.pid_file, &supervisor_record)?;
    let restart = parse_restart_policy(&args.restart);
    let restart_delay = Duration::from_millis(args.restart_delay_ms);

    // Install signal handlers. When a stop signal lands, set the
    // shutdown flag so the loop exits, and forward the signal to the
    // child if one is running.
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let child_pid: Arc<AtomicI32> = Arc::new(AtomicI32::new(0));
    let child_identity = Arc::new(std::sync::Mutex::new(None));
    install_supervisor_signal_handlers(
        shutdown.clone(),
        child_pid.clone(),
        child_identity.clone(),
        spec.listener.is_none(),
    );

    if spec.listener.is_some() {
        let result = supervise_managed_host_listener(
            &args,
            &spec,
            &supervisor_record,
            shutdown,
            child_pid,
            child_identity,
        );
        if result.is_ok() {
            let _ = fs::remove_file(&args.pid_file);
        }
        return result.map(|()| String::new());
    }

    let log_path = args.log_file.clone();
    let mut last_outcome: Option<i32> = None;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Some((dependency_env, dependency_generations)) =
            wait_for_dependency_environment(&spec, &shutdown)?
        else {
            break;
        };
        let log_handle = open_log(&log_path)?;
        let mut command = ProcessCommand::new("/bin/sh");
        command
            .arg("-c")
            .arg(&args.run)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_handle.try_clone().map_err(|error| {
                RunnerError::task_invocation_failed_write(&log_path, error)
            })?))
            .stderr(Stdio::from(log_handle))
            .env(
                "EFFIGY_HOST_PROCESS_CONTAINER",
                args.container_name.as_str(),
            )
            .env("EFFIGY_HOST_PROCESS_NAME", args.process_name.as_str());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        for (key, value) in &dependency_env {
            command.env(key, value);
        }
        configure_child_process_group(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] failed to spawn `{}`: {error}",
                        args.run
                    ),
                );
                if matches!(restart, RestartPolicy::Never) {
                    break;
                }
                last_outcome = Some(127);
                wait_or_break(&shutdown, restart_delay);
                continue;
            }
        };
        let pid = child.id() as i32;
        let live_identity = read_live_process_identity(pid as u32).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot record host process `{}` identity: {error}",
                args.process_name
            ))
        })?;
        let root_identity = ChildProcessIdentity {
            pid: pid as u32,
            boot_identity: live_identity.boot_identity,
            start_identity: live_identity.start_identity,
        };
        *child_identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(root_identity.clone());
        child_pid.store(pid, Ordering::SeqCst);
        append_log_line(
            &log_path,
            &format!(
                "[effigy host-process] started `{}` (pid {pid})",
                args.process_name
            ),
        );
        let status =
            match wait_for_host_child(&mut child, &spec, &dependency_generations, &shutdown) {
                Ok(HostChildWait::Exited(status)) => Ok(Some(status)),
                Ok(HostChildWait::DependencyChanged) => {
                    terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))
                        .map(|_| None)
                }
                Ok(HostChildWait::Shutdown) => {
                    terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))
                        .map(Some)
                }
                Err(error) => {
                    let _ = terminate_owned_child_group(
                        &mut child,
                        &root_identity,
                        Duration::from_secs(2),
                    );
                    return Err(error);
                }
            };
        child_pid.store(0, Ordering::SeqCst);
        match status {
            Ok(Some(status)) => {
                *child_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                let code = status.code().unwrap_or(-1);
                last_outcome = Some(code);
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] `{}` exited (code {code})",
                        args.process_name
                    ),
                );
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let success = status.success();
                let should_restart = match restart {
                    RestartPolicy::Always => true,
                    RestartPolicy::OnFailure => !success,
                    RestartPolicy::Never => false,
                };
                if !should_restart {
                    break;
                }
                wait_or_break(&shutdown, restart_delay);
            }
            Ok(None) => {
                last_outcome = Some(0);
                continue;
            }
            Err(error) => {
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] wait failed for `{}`: {error}",
                        args.process_name
                    ),
                );
                return Err(RunnerError::task_invocation(format!(
                    "host process `{}` completion is uncertain; its generation remains held: {error}",
                    args.process_name
                )));
            }
        }
    }

    let _ = fs::remove_file(&args.pid_file);
    let _ = last_outcome;
    Ok(String::new())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildProcessIdentity {
    pid: u32,
    boot_identity: String,
    start_identity: GatewayStartIdentity,
}

struct OwnedHostListener {
    address: SocketAddr,
    listener_pid: u32,
    listener_boot_identity: String,
    listener_start_identity: GatewayStartIdentity,
}

struct ListenerReportCleanup(PathBuf);

impl Drop for ListenerReportCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn supervise_managed_host_listener(
    args: &InternalHostProcessSuperviseArgs,
    spec: &HostProcessRuntimeSpec,
    supervisor: &HostProcessRecord,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    child_pid: Arc<AtomicI32>,
    child_identity: Arc<std::sync::Mutex<Option<ChildProcessIdentity>>>,
) -> Result<(), RunnerError> {
    let listener = spec.listener.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener supervisor has no listener declaration")
    })?;
    let report_dir = spec
        .listener_state_file
        .parent()
        .ok_or_else(|| RunnerError::task_invocation("listener state path has no parent"))?;
    let log_path = args.log_file.clone();
    let mut outcome: Option<i32> = None;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec, supervisor, "stopped", None, None, None, None, None, None, None,
                ),
            )?;
            break;
        }
        let Some((dependency_env, dependency_generations)) =
            wait_for_dependency_environment(spec, &shutdown)?
        else {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec, supervisor, "stopped", None, None, None, None, None, None, None,
                ),
            )?;
            break;
        };
        let generation = random_token()?;
        let mut diagnostic = HostListenerDiagnostic::default();
        let report_path = report_dir.join(format!(
            "{}.{}.listener-report.json",
            sanitize(&args.process_name),
            generation
        ));
        let _report_cleanup = ListenerReportCleanup(report_path.clone());
        write_listener_state(
            &spec.listener_state_file,
            listener_state(
                spec,
                supervisor,
                "starting",
                Some(&generation),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        )?;
        let log_handle = open_log(&log_path)?;
        let mut command = ProcessCommand::new("/bin/sh");
        command
            .arg("-c")
            .arg(&args.run)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_handle.try_clone().map_err(|error| {
                RunnerError::task_invocation_failed_write(&log_path, error)
            })?))
            .stderr(Stdio::from(log_handle))
            .env(
                "EFFIGY_HOST_PROCESS_CONTAINER",
                args.container_name.as_str(),
            )
            .env("EFFIGY_HOST_PROCESS_NAME", args.process_name.as_str())
            .env(HOST_LISTENER_BIND_ENV, &listener.bind)
            .env(HOST_LISTENER_REPORT_ENV, &report_path)
            .env(HOST_LISTENER_GENERATION_ENV, &generation);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        for (key, value) in &dependency_env {
            command.env(key, value);
        }
        configure_child_process_group(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                write_listener_state(
                    &spec.listener_state_file,
                    listener_state(
                        spec,
                        supervisor,
                        "failed",
                        Some(&generation),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ),
                )?;
                append_log_line(
                    &log_path,
                    &format!("[effigy host-process] managed listener launch failed: {error}"),
                );
                return Ok(());
            }
        };
        let pid = child.id();
        let live = match read_live_process_identity(pid) {
            Ok(identity) => identity,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunnerError::task_invocation(format!(
                    "cannot record managed listener child identity: {error}"
                )));
            }
        };
        *child_identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ChildProcessIdentity {
            pid,
            boot_identity: live.boot_identity.clone(),
            start_identity: live.start_identity.clone(),
        });
        child_pid.store(pid as i32, Ordering::SeqCst);
        let root_identity = ChildProcessIdentity {
            pid,
            boot_identity: live.boot_identity.clone(),
            start_identity: live.start_identity.clone(),
        };
        write_listener_state(
            &spec.listener_state_file,
            listener_state(
                spec,
                supervisor,
                "starting",
                Some(&generation),
                None,
                None,
                None,
                Some(&root_identity),
                None,
                None,
            ),
        )?;
        append_log_line(
            &log_path,
            &format!(
                "[effigy host-process] started managed listener `{}` generation {generation} (pid {pid})",
                args.process_name
            ),
        );

        let owned = match wait_for_owned_listener(
            OwnedHostListenerWait {
                supervisor,
                root_pid: pid,
                root_boot_identity: &live.boot_identity,
                root_start_identity: &live.start_identity,
                report_path: &report_path,
                generation: &generation,
                config: listener,
                shutdown: &shutdown,
            },
            &mut diagnostic,
        ) {
            Ok(owned) => owned,
            Err(error) => {
                terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
                child_pid.store(0, Ordering::SeqCst);
                *child_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                append_log_line(
                    &log_path,
                    &format!("[effigy host-process] listener failed: {error}"),
                );
                write_listener_state(
                    &spec.listener_state_file,
                    with_diagnostic(
                        listener_state(
                            spec,
                            supervisor,
                            if shutdown.load(Ordering::SeqCst) {
                                "stopped"
                            } else {
                                "failed"
                            },
                            Some(&generation),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        ),
                        &diagnostic,
                    ),
                )?;
                return Ok(());
            }
        };
        let current_dependency_generations = match dependency_states(&spec.dependencies) {
            Ok(states) => states.map(|states| dependency_environment(spec, &states).1),
            Err(error) => {
                terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
                child_pid.store(0, Ordering::SeqCst);
                *child_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                return Err(error);
            }
        };
        if current_dependency_generations.as_deref() != Some(dependency_generations.as_slice()) {
            terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
            child_pid.store(0, Ordering::SeqCst);
            *child_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    if shutdown.load(Ordering::SeqCst) {
                        "stopped"
                    } else {
                        "unavailable"
                    },
                    Some(&generation),
                    None,
                    None,
                    None,
                    Some(&root_identity),
                    Some(&owned),
                    None,
                ),
            )?;
            if shutdown.load(Ordering::SeqCst) {
                break;
            }
            continue;
        }
        if let Err(error) = prepare_managed_listener_route(spec, listener) {
            terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
            child_pid.store(0, Ordering::SeqCst);
            *child_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            append_log_line(
                &log_path,
                &format!("[effigy host-process] listener route preparation failed: {error}"),
            );
            diagnostic.route = RouteObservation::PrepareFailed;
            write_listener_state(
                &spec.listener_state_file,
                with_diagnostic(
                    listener_state(
                        spec,
                        supervisor,
                        "failed",
                        Some(&generation),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ),
                    &diagnostic,
                ),
            )?;
            return Ok(());
        }
        let owner = ManagedListenerRouteOwner {
            owner: spec.owner.clone(),
            runtime_generation: spec.runtime_generation.clone(),
            generation: generation.clone(),
            supervisor_pid: supervisor.pid,
            supervisor_boot_identity: supervisor.boot_identity.clone(),
            supervisor_start_identity: supervisor.start_identity.clone(),
            root_pid: pid,
            root_boot_identity: live.boot_identity.clone(),
            root_start_identity: live.start_identity.clone(),
            listener_pid: owned.listener_pid,
            listener_boot_identity: owned.listener_boot_identity.clone(),
            listener_start_identity: owned.listener_start_identity.clone(),
            address: owned.address.to_string(),
            route_tls: listener.route_tls,
            availability_file: spec.listener_state_file.display().to_string(),
        };
        if shutdown.load(Ordering::SeqCst) {
            terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    "stopped",
                    Some(&generation),
                    None,
                    None,
                    None,
                    Some(&root_identity),
                    Some(&owned),
                    None,
                ),
            )?;
            break;
        }
        let internal_url = format!("http://{}", owned.address);
        let public_scheme = if listener.route_tls { "https" } else { "http" };
        let public_url = format!("{public_scheme}://{}", listener.route_domain);
        write_listener_state(
            &spec.listener_state_file,
            listener_state(
                spec,
                supervisor,
                "ready",
                Some(&generation),
                Some(&owned.address),
                Some(&internal_url),
                Some(&public_url),
                Some(&root_identity),
                Some(&owned),
                Some(&owner),
            ),
        )?;
        if let Err(error) = effigy_gateway::managed_listener::verify_managed_listener_owner(&owner)
        {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    "unavailable",
                    Some(&generation),
                    None,
                    None,
                    None,
                    Some(&root_identity),
                    Some(&owned),
                    Some(&owner),
                ),
            )?;
            terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
            child_pid.store(0, Ordering::SeqCst);
            *child_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            append_log_line(
                &log_path,
                &format!(
                    "[effigy host-process] listener ownership changed before publication: {error}"
                ),
            );
            diagnostic.route = RouteObservation::OwnershipChanged;
            write_listener_state(
                &spec.listener_state_file,
                with_diagnostic(
                    listener_state(
                        spec,
                        supervisor,
                        "failed",
                        Some(&generation),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ),
                    &diagnostic,
                ),
            )?;
            return Ok(());
        }
        if let Err(error) = publish_managed_listener_route(spec, listener, &owner) {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    "unavailable",
                    Some(&generation),
                    None,
                    None,
                    None,
                    Some(&root_identity),
                    Some(&owned),
                    Some(&owner),
                ),
            )?;
            deregister_managed_listener_route(
                &listener_route_table_path(spec)?,
                &spec.project_path,
                &listener.route_domain,
                &spec.owner,
                &generation,
            )
            .map_err(|cleanup_error| RunnerError::task_invocation(cleanup_error.to_string()))?;
            terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
            child_pid.store(0, Ordering::SeqCst);
            *child_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            append_log_line(
                &log_path,
                &format!("[effigy host-process] route publication failed: {error}"),
            );
            diagnostic.route = RouteObservation::PublicationFailed;
            write_listener_state(
                &spec.listener_state_file,
                with_diagnostic(
                    listener_state(
                        spec,
                        supervisor,
                        "failed",
                        Some(&generation),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ),
                    &diagnostic,
                ),
            )?;
            return Ok(());
        }

        let child_wait =
            match wait_for_host_child(&mut child, spec, &dependency_generations, &shutdown) {
                Ok(wait) => wait,
                Err(error) => {
                    let _ = write_listener_state(
                        &spec.listener_state_file,
                        listener_state(
                            spec,
                            supervisor,
                            "unavailable",
                            Some(&generation),
                            None,
                            None,
                            None,
                            Some(&root_identity),
                            Some(&owned),
                            Some(&owner),
                        ),
                    );
                    let _ = deregister_managed_listener_route(
                        &listener_route_table_path(spec)?,
                        &spec.project_path,
                        &listener.route_domain,
                        &spec.owner,
                        &generation,
                    );
                    let _ = terminate_owned_child_group(
                        &mut child,
                        &root_identity,
                        Duration::from_secs(2),
                    );
                    return Err(error);
                }
            };
        let stopping = matches!(&child_wait, HostChildWait::Shutdown);
        let status = match child_wait {
            HostChildWait::Exited(status) => Some(status),
            HostChildWait::DependencyChanged | HostChildWait::Shutdown => {
                write_listener_state(
                    &spec.listener_state_file,
                    listener_state(
                        spec,
                        supervisor,
                        "unavailable",
                        Some(&generation),
                        None,
                        None,
                        None,
                        Some(&root_identity),
                        Some(&owned),
                        Some(&owner),
                    ),
                )?;
                let _ = deregister_managed_listener_route(
                    &listener_route_table_path(spec)?,
                    &spec.project_path,
                    &listener.route_domain,
                    &spec.owner,
                    &generation,
                )
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                terminate_owned_child_group(&mut child, &root_identity, Duration::from_secs(2))?;
                child_pid.store(0, Ordering::SeqCst);
                *child_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                write_listener_state(
                    &spec.listener_state_file,
                    listener_state(
                        spec,
                        supervisor,
                        if stopping { "stopped" } else { "unavailable" },
                        Some(&generation),
                        None,
                        None,
                        None,
                        Some(&root_identity),
                        Some(&owned),
                        Some(&owner),
                    ),
                )?;
                if stopping {
                    break;
                }
                wait_or_break(&shutdown, Duration::from_millis(args.restart_delay_ms));
                continue;
            }
        };
        child_pid.store(0, Ordering::SeqCst);
        *child_identity
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        let status = status.expect("exited child has a process status");
        outcome = Some(status.code().unwrap_or(-1));
        write_listener_state(
            &spec.listener_state_file,
            listener_state(
                spec,
                supervisor,
                "unavailable",
                Some(&generation),
                None,
                None,
                None,
                Some(&root_identity),
                Some(&owned),
                Some(&owner),
            ),
        )?;
        let _ = deregister_managed_listener_route(
            &listener_route_table_path(spec)?,
            &spec.project_path,
            &listener.route_domain,
            &spec.owner,
            &generation,
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

        let remaining_group_members = effigy_process::process_group_member_ids(root_identity.pid)
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "managed child group is uncertain after its leader exited; retaining the unavailable generation: {error}"
                ))
            })?;
        if !remaining_group_members.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "managed child leader exited while process group {} still has members {remaining_group_members:?}; route withdrawn and generation retained",
                root_identity.pid
            )));
        }

        if shutdown.load(Ordering::SeqCst) {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    "stopped",
                    Some(&generation),
                    None,
                    None,
                    None,
                    Some(&root_identity),
                    Some(&owned),
                    Some(&owner),
                ),
            )?;
            break;
        }
        let should_restart = match parse_restart_policy(&args.restart) {
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => !status.success(),
            RestartPolicy::Never => false,
        };
        if !should_restart {
            write_listener_state(
                &spec.listener_state_file,
                listener_state(
                    spec,
                    supervisor,
                    "stopped",
                    Some(&generation),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            )?;
            break;
        }
        wait_or_break(&shutdown, Duration::from_millis(args.restart_delay_ms));
    }
    if shutdown.load(Ordering::SeqCst) {
        // The listener route was withdrawn and its exact child group reaped
        // before the shutdown branch exits. Persist a terminal state without
        // endpoint claims so a later explicit start can distinguish completed
        // teardown from an unavailable generation held for recovery.
        write_listener_state(
            &spec.listener_state_file,
            listener_state(
                spec, supervisor, "stopped", None, None, None, None, None, None, None,
            ),
        )?;
    }
    let _ = outcome;
    Ok(())
}

// The serialized state intentionally mirrors each independent availability field;
// keeping this projection explicit makes it harder to conflate a missing address,
// child identity, discovered listener, or route claim during failure handling.
#[allow(clippy::too_many_arguments)]
fn listener_state(
    spec: &HostProcessRuntimeSpec,
    supervisor: &HostProcessRecord,
    status: &str,
    generation: Option<&str>,
    address: Option<&SocketAddr>,
    internal_url: Option<&str>,
    public_url: Option<&str>,
    child: Option<&ChildProcessIdentity>,
    listener: Option<&OwnedHostListener>,
    route_owner: Option<&ManagedListenerRouteOwner>,
) -> HostListenerState {
    HostListenerState {
        schema: HOST_LISTENER_STATE_SCHEMA.to_owned(),
        owner: spec.owner.clone(),
        runtime_generation: spec.runtime_generation.clone(),
        generation: generation.unwrap_or_default().to_owned(),
        status: status.to_owned(),
        address: address.map(ToString::to_string),
        internal_url: internal_url.map(str::to_owned),
        public_url: public_url.map(str::to_owned),
        route_domain: spec
            .listener
            .as_ref()
            .map(|listener| listener.route_domain.clone()),
        supervisor_pid: supervisor.pid,
        supervisor_boot_identity: supervisor.boot_identity.clone(),
        supervisor_start_identity: supervisor.start_identity.clone(),
        child_pid: child.map(|identity| identity.pid),
        child_boot_identity: child.map(|identity| identity.boot_identity.clone()),
        child_start_identity: child.map(|identity| identity.start_identity.clone()),
        listener_pid: listener.map(|identity| identity.listener_pid),
        listener_boot_identity: listener.map(|identity| identity.listener_boot_identity.clone()),
        listener_start_identity: listener.map(|identity| identity.listener_start_identity.clone()),
        route_owner: route_owner.cloned(),
        diagnostic: None,
    }
}

/// Attaches the last startup observation to a terminal generation state.
fn with_diagnostic(
    mut state: HostListenerState,
    diagnostic: &HostListenerDiagnostic,
) -> HostListenerState {
    state.diagnostic = Some(*diagnostic);
    state
}

struct OwnedHostListenerWait<'a> {
    supervisor: &'a HostProcessRecord,
    root_pid: u32,
    root_boot_identity: &'a str,
    root_start_identity: &'a GatewayStartIdentity,
    report_path: &'a Path,
    generation: &'a str,
    config: &'a EffectiveManagedHostListener,
    shutdown: &'a Arc<std::sync::atomic::AtomicBool>,
}

/// Records the latest startup observation for one child generation so a
/// failed generation keeps evidence after its report file and child are gone.
/// Tokens describe what was last seen; they never authorize ownership,
/// signaling, publication or release, and they are not read back as identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostListenerDiagnostic {
    report: ReportObservation,
    claimed_address: Option<SocketAddr>,
    ownership: OwnershipObservation,
    candidates_inspected: Option<u32>,
    observed_listener_pid: Option<u32>,
    http_probe: Option<HttpReadinessOutcome>,
    http_status: Option<u16>,
    route: RouteObservation,
}

impl Default for HostListenerDiagnostic {
    fn default() -> Self {
        Self {
            report: ReportObservation::NotObserved,
            claimed_address: None,
            ownership: OwnershipObservation::NotReached,
            candidates_inspected: None,
            observed_listener_pid: None,
            http_probe: None,
            http_status: None,
            route: RouteObservation::NotReached,
        }
    }
}

impl HostListenerDiagnostic {
    fn summary(&self) -> String {
        let probe = match (self.http_probe, self.http_status) {
            (Some(outcome), Some(status)) => format!("{}:{status}", outcome.as_str()),
            (Some(outcome), None) => outcome.as_str().to_owned(),
            (None, _) => "not_attempted".to_owned(),
        };
        format!(
            "report={}, ownership={}, http_probe={probe}, route={}",
            self.report.as_str(),
            self.ownership.as_str(),
            self.route.as_str()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportObservation {
    NotObserved,
    Absent,
    Unreadable,
    Unsafe,
    SchemaOrGenerationMismatch,
    InvalidAddress,
    BindMismatch,
    Accepted,
}

impl ReportObservation {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotObserved => "not_observed",
            Self::Absent => "absent",
            Self::Unreadable => "unreadable",
            Self::Unsafe => "unsafe",
            Self::SchemaOrGenerationMismatch => "schema_or_generation_mismatch",
            Self::InvalidAddress => "invalid_address",
            Self::BindMismatch => "bind_mismatch",
            Self::Accepted => "accepted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OwnershipObservation {
    NotReached,
    NoOwnerObserved,
    ExclusiveOwnerObserved,
    SupervisorChanged,
    ChildUnavailable,
    ChildChanged,
    SupervisorUnavailable,
    AncestryChanged,
    AncestryUnavailable,
    IdentityUnavailable,
    IdentityChanged,
    ListenerOutsideGeneration,
    SocketInspectionFailed,
    ExclusivityUnproven,
}

impl OwnershipObservation {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotReached => "not_reached",
            Self::NoOwnerObserved => "no_owner_observed",
            Self::ExclusiveOwnerObserved => "exclusive_owner_observed",
            Self::SupervisorChanged => "supervisor_changed",
            Self::ChildUnavailable => "child_unavailable",
            Self::ChildChanged => "child_changed",
            Self::SupervisorUnavailable => "supervisor_unavailable",
            Self::AncestryChanged => "ancestry_changed",
            Self::AncestryUnavailable => "ancestry_unavailable",
            Self::IdentityUnavailable => "identity_unavailable",
            Self::IdentityChanged => "identity_changed",
            Self::ListenerOutsideGeneration => "listener_outside_generation",
            Self::SocketInspectionFailed => "socket_inspection_failed",
            Self::ExclusivityUnproven => "exclusivity_unproven",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RouteObservation {
    NotReached,
    PrepareFailed,
    OwnershipChanged,
    PublicationFailed,
}

impl RouteObservation {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotReached => "not_reached",
            Self::PrepareFailed => "prepare_failed",
            Self::OwnershipChanged => "ownership_changed",
            Self::PublicationFailed => "publication_failed",
        }
    }
}

/// Records an identity failure as unavailable or changed, never both.
fn observe_identity(
    diagnostic: &mut HostListenerDiagnostic,
    unavailable: OwnershipObservation,
    changed: OwnershipObservation,
    result: Result<(), IdentityFailure>,
) -> Result<(), RunnerError> {
    result.map_err(|failure| match failure {
        IdentityFailure::Unavailable(error) => {
            diagnostic.ownership = unavailable;
            error
        }
        IdentityFailure::Changed(error) => {
            diagnostic.ownership = changed;
            error
        }
    })
}

/// Keeps a diagnostic observation at the point it is seen, then passes the
/// original verification error through unchanged.
fn observe_ownership<T>(
    diagnostic: &mut HostListenerDiagnostic,
    observation: OwnershipObservation,
    result: Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    result.inspect_err(|_| diagnostic.ownership = observation)
}

fn wait_for_owned_listener(
    wait: OwnedHostListenerWait<'_>,
    diagnostic: &mut HostListenerDiagnostic,
) -> Result<OwnedHostListener, RunnerError> {
    let OwnedHostListenerWait {
        supervisor,
        root_pid,
        root_boot_identity,
        root_start_identity,
        report_path,
        generation,
        config,
        shutdown,
    } = wait;
    let timeout = Duration::from_secs(config.readiness_timeout_secs);
    let deadline = Instant::now() + timeout;
    let preferred = config.bind.parse::<SocketAddr>().map_err(|error| {
        RunnerError::task_invocation(format!("managed listener bind address is invalid: {error}"))
    })?;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Err(RunnerError::task_invocation(
                "managed listener startup was stopped before readiness",
            ));
        }
        observe_identity(
            diagnostic,
            OwnershipObservation::SupervisorUnavailable,
            OwnershipObservation::SupervisorChanged,
            checked_supervisor_identity(supervisor),
        )?;
        observe_identity(
            diagnostic,
            OwnershipObservation::ChildUnavailable,
            OwnershipObservation::ChildChanged,
            checked_child_identity(root_pid, root_boot_identity, root_start_identity),
        )?;
        if !effigy_process::process_is_descendant_of(root_pid, supervisor.pid) {
            diagnostic.ownership = OwnershipObservation::AncestryChanged;
            return Err(RunnerError::task_invocation(
                "managed host child is no longer beneath its recorded supervisor",
            ));
        }
        let metadata = match fs::symlink_metadata(report_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostic.report = if error.kind() == std::io::ErrorKind::NotFound {
                    ReportObservation::Absent
                } else {
                    ReportObservation::Unreadable
                };
                if Instant::now() >= deadline {
                    return Err(listener_readiness_timeout(timeout));
                }
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 4096 {
            diagnostic.report = ReportObservation::Unsafe;
            return Err(RunnerError::task_invocation(
                "managed listener report is not a small regular file",
            ));
        }
        let report: HostListenerReport = match read_private_json(report_path) {
            Ok(report) => report,
            Err(error) => {
                diagnostic.report = ReportObservation::Unreadable;
                return Err(error);
            }
        };
        if report.schema != HOST_LISTENER_REPORT_SCHEMA || report.generation != generation {
            diagnostic.report = ReportObservation::SchemaOrGenerationMismatch;
            return Err(RunnerError::task_invocation(
                "managed listener report has an unsupported schema or stale generation",
            ));
        }
        let address = match report.address.parse::<SocketAddr>() {
            Ok(address) => address,
            Err(_) => {
                diagnostic.report = ReportObservation::InvalidAddress;
                return Err(RunnerError::task_invocation(
                    "managed listener report address is not a socket address",
                ));
            }
        };
        diagnostic.claimed_address = Some(address);
        if !address.ip().is_loopback()
            || address.port() == 0
            || address.ip() != preferred.ip()
            || (preferred.port() != 0 && address.port() != preferred.port())
        {
            diagnostic.report = ReportObservation::BindMismatch;
            return Err(RunnerError::task_invocation(
                "managed listener report does not match its declared loopback bind preference",
            ));
        }
        diagnostic.report = ReportObservation::Accepted;
        let mut candidates = vec![root_pid];
        let descendants = effigy_process::process_descendant_ids(root_pid).map_err(|error| {
            diagnostic.ownership = OwnershipObservation::AncestryUnavailable;
            RunnerError::task_invocation(format!(
                "managed listener process ancestry is unavailable: {error}"
            ))
        })?;
        candidates.extend(descendants);
        candidates.sort_unstable();
        candidates.dedup();
        diagnostic.candidates_inspected = u32::try_from(candidates.len()).ok();
        diagnostic.ownership = OwnershipObservation::NoOwnerObserved;
        diagnostic.observed_listener_pid = None;
        let mut found = None;
        for pid in candidates {
            let identity = match read_live_process_identity(pid) {
                Ok(identity) => identity,
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(libc::ESRCH) =>
                {
                    continue;
                }
                Err(error) => {
                    diagnostic.ownership = OwnershipObservation::IdentityUnavailable;
                    return Err(RunnerError::task_invocation(format!(
                        "managed listener process identity is unavailable for PID {pid}: {error}"
                    )));
                }
            };
            let endpoints = match process_listening_endpoints(pid) {
                Ok(endpoints) => endpoints,
                Err(error) => {
                    diagnostic.ownership = OwnershipObservation::SocketInspectionFailed;
                    return Err(RunnerError::task_invocation(format!(
                        "cannot inspect listener sockets for PID {pid}: {error}"
                    )));
                }
            };
            if endpoints.contains(&GatewayEndpoint {
                transport: GatewayTransport::Tcp,
                addr: address,
            }) {
                if pid != root_pid && !effigy_process::process_is_descendant_of(pid, root_pid) {
                    diagnostic.ownership = OwnershipObservation::ListenerOutsideGeneration;
                    return Err(RunnerError::task_invocation(
                        "managed listener process is outside its recorded child generation",
                    ));
                }
                observe_identity(
                    diagnostic,
                    OwnershipObservation::ChildUnavailable,
                    OwnershipObservation::ChildChanged,
                    checked_child_identity(root_pid, root_boot_identity, root_start_identity),
                )?;
                let current = match read_live_process_identity(pid) {
                    Ok(current) => current,
                    Err(error) => {
                        diagnostic.ownership = OwnershipObservation::IdentityUnavailable;
                        return Err(RunnerError::task_invocation(format!(
                            "managed listener owner identity changed during discovery: {error}"
                        )));
                    }
                };
                if current.boot_identity != identity.boot_identity
                    || current.start_identity != identity.start_identity
                {
                    diagnostic.ownership = OwnershipObservation::IdentityChanged;
                    return Err(RunnerError::task_invocation(
                        "managed listener owner PID was reused during discovery",
                    ));
                }
                observe_ownership(
                    diagnostic,
                    OwnershipObservation::ExclusivityUnproven,
                    effigy_gateway::legacy::verify_process_listener_exclusive(
                        pid,
                        &GatewayEndpoint {
                            transport: GatewayTransport::Tcp,
                            addr: address,
                        },
                    )
                    .map_err(|error| {
                        RunnerError::task_invocation(format!(
                            "cannot prove exclusive managed listener ownership: {error}"
                        ))
                    }),
                )?;
                diagnostic.ownership = OwnershipObservation::ExclusiveOwnerObserved;
                diagnostic.observed_listener_pid = Some(pid);
                found = Some(OwnedHostListener {
                    address,
                    listener_pid: pid,
                    listener_boot_identity: identity.boot_identity,
                    listener_start_identity: identity.start_identity,
                });
                break;
            }
        }
        if let Some(found) = found {
            let probe = probe_http_readiness(found.address, &config.route_domain, config);
            diagnostic.http_probe = Some(probe.outcome);
            diagnostic.http_status = probe.status;
            if probe.outcome == HttpReadinessOutcome::Ready {
                observe_identity(
                    diagnostic,
                    OwnershipObservation::SupervisorUnavailable,
                    OwnershipObservation::SupervisorChanged,
                    checked_supervisor_identity(supervisor),
                )?;
                observe_identity(
                    diagnostic,
                    OwnershipObservation::ChildUnavailable,
                    OwnershipObservation::ChildChanged,
                    checked_child_identity(root_pid, root_boot_identity, root_start_identity),
                )?;
                if !effigy_process::process_is_descendant_of(root_pid, supervisor.pid) {
                    diagnostic.ownership = OwnershipObservation::AncestryChanged;
                    return Err(RunnerError::task_invocation(
                        "managed host child ancestry changed during readiness",
                    ));
                }
                return Ok(found);
            }
        }
        if Instant::now() >= deadline {
            return Err(listener_readiness_timeout(timeout));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn listener_readiness_timeout(timeout: Duration) -> RunnerError {
    RunnerError::task_invocation(format!(
        "timed out after {}s waiting for an owned ready managed host listener",
        timeout.as_secs()
    ))
}

/// A recorded process identity that could not be confirmed. `Unavailable`
/// means the process could not be read now; `Changed` means it was read and
/// does not match the recorded identity. Diagnostics keep them distinct.
enum IdentityFailure {
    Unavailable(RunnerError),
    Changed(RunnerError),
}

impl From<IdentityFailure> for RunnerError {
    fn from(failure: IdentityFailure) -> Self {
        match failure {
            IdentityFailure::Unavailable(error) | IdentityFailure::Changed(error) => error,
        }
    }
}

fn checked_child_identity(
    pid: u32,
    boot_identity: &str,
    start_identity: &GatewayStartIdentity,
) -> Result<(), IdentityFailure> {
    let current = read_live_process_identity(pid).map_err(|error| {
        IdentityFailure::Unavailable(RunnerError::task_invocation(format!(
            "managed host child identity is unavailable: {error}"
        )))
    })?;
    if current.boot_identity != boot_identity || &current.start_identity != start_identity {
        return Err(IdentityFailure::Changed(RunnerError::task_invocation(
            "managed host child PID was reused during listener discovery",
        )));
    }
    Ok(())
}

fn verify_child_identity(
    pid: u32,
    boot_identity: &str,
    start_identity: &GatewayStartIdentity,
) -> Result<(), RunnerError> {
    Ok(checked_child_identity(pid, boot_identity, start_identity)?)
}

fn checked_supervisor_identity(record: &HostProcessRecord) -> Result<(), IdentityFailure> {
    let current = read_live_process_identity(record.pid).map_err(|error| {
        IdentityFailure::Unavailable(RunnerError::task_invocation(format!(
            "managed host supervisor identity is unavailable: {error}"
        )))
    })?;
    if current.boot_identity != record.boot_identity
        || current.start_identity != record.start_identity
    {
        return Err(IdentityFailure::Changed(RunnerError::task_invocation(
            "managed host supervisor PID was reused during listener discovery",
        )));
    }
    Ok(())
}

fn verify_supervisor_identity(record: &HostProcessRecord) -> Result<(), RunnerError> {
    Ok(checked_supervisor_identity(record)?)
}

fn publish_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
    owner: &ManagedListenerRouteOwner,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no private gateway state root")
    })?;
    effigy_gateway::private_state::validate_root(root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let route_table_path = root.join("routes.json");
    let address = owner.address.parse::<SocketAddr>().map_err(|_| {
        RunnerError::task_invocation("managed host listener owner has an invalid address")
    })?;
    register_managed_listener_route(
        &route_table_path,
        &spec.project_path,
        &listener.route_domain,
        &address.to_string(),
        listener.route_tls,
        owner.clone(),
        None,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn prepare_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no private gateway state root")
    })?;
    effigy_gateway::private_state::validate_root(root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if listener.route_tls {
        super::gateway_command::ensure_gateway_tls_cert(&listener.route_domain)?;
    }
    Ok(())
}

fn listener_route_table_path(spec: &HostProcessRuntimeSpec) -> Result<PathBuf, RunnerError> {
    spec.gateway_root
        .as_deref()
        .map(|root| root.join("routes.json"))
        .ok_or_else(|| RunnerError::task_invocation("private gateway root is unavailable"))
}

fn write_listener_state(path: &Path, state: HostListenerState) -> Result<(), RunnerError> {
    write_private_json(path, &state)
}

fn read_private_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, RunnerError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1_048_576 {
        return Err(RunnerError::task_invocation(format!(
            "managed host process record {} is not a bounded regular file",
            path.display()
        )));
    }
    let raw =
        fs::read(path).map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    serde_json::from_slice(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(path, error))
}

fn configure_child_process_group(command: &mut ProcessCommand) {
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from)
        });
    }
}

fn signal_owned_child_group(
    pid: u32,
    boot_identity: &str,
    start_identity: &GatewayStartIdentity,
    signal: Signal,
) -> Result<(), RunnerError> {
    verify_child_identity(pid, boot_identity, start_identity)?;
    kill(Pid::from_raw(-(pid as i32)), signal).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to signal owned managed host child process group: {error}"
        ))
    })
}

fn signal_owned_child_generation(
    identity: &ChildProcessIdentity,
    signal: Signal,
) -> Result<(), RunnerError> {
    match read_live_process_identity(identity.pid) {
        Ok(current)
            if current.boot_identity == identity.boot_identity
                && current.start_identity == identity.start_identity =>
        {
            return signal_owned_child_group(
                identity.pid,
                &identity.boot_identity,
                &identity.start_identity,
                signal,
            );
        }
        Ok(_) => {
            return Err(RunnerError::task_invocation(
                "managed host child PID was reused; refusing to signal its process group",
            ));
        }
        Err(error) if process_identity_is_absent(&error) => {}
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "managed host child identity is uncertain; refusing to signal its process group: {error}"
            )));
        }
    }
    let members = effigy_process::process_group_member_ids(identity.pid).map_err(|error| {
        RunnerError::task_invocation(format!(
            "managed host child process group is uncertain; refusing to signal: {error}"
        ))
    })?;
    if !members.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "managed host child leader is absent while process group {} still has members {members:?}; refusing to signal an unverified group",
            identity.pid
        )));
    }
    Ok(())
}

fn process_identity_is_absent(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

fn terminate_owned_child_group(
    child: &mut std::process::Child,
    identity: &ChildProcessIdentity,
    grace: Duration,
) -> Result<std::process::ExitStatus, RunnerError> {
    let mut status = child.try_wait().map_err(|error| {
        RunnerError::task_invocation(format!(
            "managed host child completion is uncertain; leaving generation held: {error}"
        ))
    })?;
    if status.is_none() {
        signal_owned_child_group(
            identity.pid,
            &identity.boot_identity,
            &identity.start_identity,
            Signal::SIGTERM,
        )?;
    }
    let mut deadline = Instant::now() + grace.max(Duration::from_millis(100));
    let mut forced = false;
    loop {
        if status.is_none() {
            status = child.try_wait().map_err(|error| {
                RunnerError::task_invocation(format!(
                    "managed host child wait became uncertain; leaving generation held: {error}"
                ))
            })?;
        }
        let members = effigy_process::process_group_member_ids(identity.pid).map_err(|error| {
            RunnerError::task_invocation(format!(
                "managed host child process group is uncertain; leaving generation held: {error}"
            ))
        })?;
        if members.is_empty() {
            if let Some(status) = status.take() {
                return Ok(status);
            }
        }
        if Instant::now() >= deadline {
            if forced {
                return Err(RunnerError::task_invocation(
                    "managed host child group did not exit after force-stop; leaving generation held",
                ));
            }
            signal_owned_child_generation(identity, Signal::SIGKILL)?;
            forced = true;
            deadline = Instant::now() + Duration::from_secs(2);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub(in crate::runner) fn run_internal_host_process_stop(
    args: InternalHostProcessStopArgs,
) -> Result<String, RunnerError> {
    match fs::symlink_metadata(&args.pid_file) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => {
            return Err(RunnerError::task_invocation_failed_read(
                &args.pid_file,
                error,
            ))
        }
    }
    let record: HostProcessRecord = read_private_json(&args.pid_file)?;
    if record.schema != HOST_PROCESS_RECORD_SCHEMA {
        return Err(RunnerError::task_invocation(format!(
            "unsupported host process supervisor record schema `{}`",
            record.schema
        )));
    }
    let stopped = stop_recorded_supervisor(
        &args.pid_file,
        &record,
        parse_signal_name(&args.signal),
        args.grace_secs,
    )?;
    if !stopped {
        return Err(RunnerError::task_invocation(
            "recorded host process supervisor is absent; retaining its route and identity records",
        ));
    }
    cleanup_stopped_listener_routes(&args.pid_file, &record)?;
    remove_host_process_record_if_matches(&args.pid_file, &record)?;
    Ok(String::new())
}

fn cleanup_stopped_listener_routes(
    pid_file: &Path,
    record: &HostProcessRecord,
) -> Result<(), RunnerError> {
    let spec_path = pid_file.with_extension("spec.json");
    match fs::symlink_metadata(&spec_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&spec_path, error)),
    }
    let spec: HostProcessRuntimeSpec = read_private_json(&spec_path)?;
    let Some(listener) = spec.listener.as_ref() else {
        return Ok(());
    };
    let state_path_exists = match fs::symlink_metadata(&spec.listener_state_file) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(RunnerError::task_invocation_failed_read(
                &spec.listener_state_file,
                error,
            ))
        }
    };
    let state = if state_path_exists {
        let state: HostListenerState = read_private_json(&spec.listener_state_file)?;
        if state.schema == HOST_LISTENER_STATE_SCHEMA
            && state.owner == spec.owner
            && state.runtime_generation == spec.runtime_generation
            && state.supervisor_pid == record.pid
            && state.supervisor_boot_identity == record.boot_identity
            && state.supervisor_start_identity == record.start_identity
        {
            let mut unavailable = state.clone();
            unavailable.status = "unavailable".to_owned();
            unavailable.address = None;
            unavailable.internal_url = None;
            unavailable.public_url = None;
            write_listener_state(&spec.listener_state_file, unavailable)?;
            Some(state)
        } else {
            None
        }
    } else {
        None
    };
    if let Some(state) = state.as_ref() {
        if let Some(pid) = state.child_pid {
            let identity = ChildProcessIdentity {
                pid,
                boot_identity: state.child_boot_identity.clone().ok_or_else(|| {
                    RunnerError::task_invocation(
                        "managed child cleanup is held because its boot identity is missing",
                    )
                })?,
                start_identity: state.child_start_identity.clone().ok_or_else(|| {
                    RunnerError::task_invocation(
                        "managed child cleanup is held because its start identity is missing",
                    )
                })?,
            };
            stop_recorded_child_group(&identity, Duration::from_secs(2))?;
        } else if state.status == "ready" || state.route_owner.is_some() {
            return Err(RunnerError::task_invocation(
                "managed child cleanup is held because a published generation has no child identity",
            ));
        }
    }
    let route_table_path = listener_route_table_path(&spec)?;
    let table = RouteTable::load(&route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Some(route) = table.lookup(&listener.route_domain) {
        if let Some(owner) = route.managed_listener.as_ref().filter(|owner| {
            owner.owner == spec.owner
                && owner.runtime_generation == spec.runtime_generation
                && owner.supervisor_pid == record.pid
                && owner.supervisor_boot_identity == record.boot_identity
                && owner.supervisor_start_identity == record.start_identity
        }) {
            let _ = deregister_managed_listener_route(
                &route_table_path,
                &spec.project_path,
                &listener.route_domain,
                &spec.owner,
                &owner.generation,
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
    }
    if let Some(mut state) = state {
        state.status = "stopped".to_owned();
        state.address = None;
        state.internal_url = None;
        state.public_url = None;
        state.route_owner = None;
        state.child_pid = None;
        state.child_boot_identity = None;
        state.child_start_identity = None;
        state.listener_pid = None;
        state.listener_boot_identity = None;
        state.listener_start_identity = None;
        write_listener_state(&spec.listener_state_file, state)?;
    }
    Ok(())
}

fn stop_recorded_child_group(
    identity: &ChildProcessIdentity,
    grace: Duration,
) -> Result<(), RunnerError> {
    let mut deadline = Instant::now() + grace.max(Duration::from_millis(100));
    let mut forced = false;
    let mut signaled = false;
    loop {
        let members = effigy_process::process_group_member_ids(identity.pid).map_err(|error| {
            RunnerError::task_invocation(format!(
                "managed child process group is uncertain; leaving its generation held: {error}"
            ))
        })?;
        if members.is_empty() {
            return Ok(());
        }
        if !signaled {
            signal_owned_child_generation(identity, Signal::SIGTERM)?;
            signaled = true;
        }
        if Instant::now() >= deadline {
            if forced {
                return Err(RunnerError::task_invocation(
                    "managed child group did not exit; retaining its identity record",
                ));
            }
            signal_owned_child_generation(identity, Signal::SIGKILL)?;
            forced = true;
            deadline = Instant::now() + Duration::from_secs(2);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn stop_recorded_supervisor(
    _pid_path: &Path,
    record: &HostProcessRecord,
    signal: Signal,
    grace_secs: u64,
) -> Result<bool, RunnerError> {
    let pid = record.pid;
    if pid == 0 {
        return Err(RunnerError::task_invocation(
            "host process supervisor record has PID zero",
        ));
    }
    let live = match read_live_process_identity(pid) {
        Ok(live) => live,
        Err(error) if process_identity_is_absent(&error) => return Ok(false),
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "host process supervisor identity is unavailable; leaving its record held: {error}"
            )))
        }
    };
    if live.boot_identity != record.boot_identity || live.start_identity != record.start_identity {
        return Err(RunnerError::task_invocation(
            "host process supervisor PID was reused; refusing to signal or remove its record",
        ));
    }
    let nix_pid = Pid::from_raw(pid as i32);
    kill(nix_pid, signal).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to signal owned host process supervisor: {error}"
        ))
    })?;
    let mut deadline = Instant::now() + Duration::from_secs(grace_secs.max(1));
    let mut forced = false;
    loop {
        match read_live_process_identity(record.pid) {
            Ok(current)
                if current.boot_identity == record.boot_identity
                    && current.start_identity == record.start_identity => {}
            Ok(_) => {
                return Err(RunnerError::task_invocation(
                    "host process supervisor PID changed during shutdown; leaving its record held",
                ));
            }
            Err(error) if process_identity_is_absent(&error) => break,
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "host process supervisor identity became unknown during shutdown: {error}"
                )));
            }
        }
        if Instant::now() >= deadline {
            if forced {
                return Err(RunnerError::task_invocation(
                    "host process supervisor did not exit after its recorded generation was force-stopped; leaving its record held",
                ));
            }
            let current = match read_live_process_identity(record.pid) {
                Ok(current) => current,
                Err(error) if process_identity_is_absent(&error) => break,
                Err(error) => {
                    return Err(RunnerError::task_invocation(format!(
                        "host process supervisor identity became unknown during shutdown: {error}"
                    )))
                }
            };
            if current.boot_identity != record.boot_identity
                || current.start_identity != record.start_identity
            {
                return Err(RunnerError::task_invocation(
                    "host process supervisor PID changed during shutdown; leaving its record held",
                ));
            }
            kill(nix_pid, Signal::SIGKILL).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to force stop owned host process supervisor: {error}"
                ))
            })?;
            forced = true;
            deadline = Instant::now() + Duration::from_secs(2);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(true)
}

fn remove_host_process_record_if_matches(
    pid_path: &Path,
    expected: &HostProcessRecord,
) -> Result<(), RunnerError> {
    match read_private_json::<HostProcessRecord>(pid_path) {
        Ok(current) if current == *expected => fs::remove_file(pid_path)
            .map_err(|error| RunnerError::task_invocation_failed_write(pid_path, error)),
        Ok(_) => Err(RunnerError::task_invocation(
            "host process supervisor record changed during cleanup; retaining the successor record",
        )),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone, Copy)]
enum RestartPolicy {
    OnFailure,
    Always,
    Never,
}

fn parse_restart_policy(value: &str) -> RestartPolicy {
    match value {
        "always" => RestartPolicy::Always,
        "never" => RestartPolicy::Never,
        _ => RestartPolicy::OnFailure,
    }
}

fn parse_signal_name(name: &str) -> Signal {
    match name.to_ascii_uppercase().as_str() {
        "SIGINT" => Signal::SIGINT,
        "SIGHUP" => Signal::SIGHUP,
        "SIGKILL" => Signal::SIGKILL,
        _ => Signal::SIGTERM,
    }
}

fn open_log(path: &Path) -> Result<std::fs::File, RunnerError> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

fn append_log_line(path: &Path, line: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        use std::io::Write as _;
        let _ = writeln!(f, "{line}");
    }
}

fn wait_or_break(shutdown: &Arc<std::sync::atomic::AtomicBool>, delay: Duration) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100).min(delay));
    }
}

fn install_supervisor_signal_handlers(
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    child_pid: Arc<AtomicI32>,
    child_identity: Arc<std::sync::Mutex<Option<ChildProcessIdentity>>>,
    signal_child_on_stop: bool,
) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    std::thread::spawn(move || {
        let mut signals = match Signals::new([SIGTERM, SIGINT, SIGHUP]) {
            Ok(s) => s,
            Err(_) => return,
        };
        for signal in signals.forever() {
            shutdown.store(true, Ordering::SeqCst);
            let pid = child_pid.load(Ordering::SeqCst);
            if pid > 0 {
                let nix_signal = match signal {
                    SIGINT => Signal::SIGINT,
                    SIGHUP => Signal::SIGHUP,
                    _ => Signal::SIGTERM,
                };
                let identity = child_identity
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                if signal_child_on_stop {
                    if let Some(identity) = identity.filter(|identity| identity.pid == pid as u32) {
                        let _ = signal_owned_child_group(
                            identity.pid,
                            &identity.boot_identity,
                            &identity.start_identity,
                            nix_signal,
                        );
                        let _ = nix_signal_child_group_kill_after(
                            child_pid.clone(),
                            child_identity.clone(),
                            identity,
                        );
                    }
                }
            }
        }
    });
}

fn nix_signal_child_group_kill_after(
    child_pid: Arc<AtomicI32>,
    child_identity: Arc<std::sync::Mutex<Option<ChildProcessIdentity>>>,
    expected: ChildProcessIdentity,
) -> Result<(), ()> {
    // Escalation remains tied to the exact child generation captured by the
    // stop callback, so it cannot signal a later restart that reused the PID.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        if child_pid.load(Ordering::SeqCst) == expected.pid as i32
            && child_identity
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                == Some(&expected)
        {
            let _ = signal_owned_child_group(
                expected.pid,
                &expected.boot_identity,
                &expected.start_identity,
                Signal::SIGKILL,
            );
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize("dev_web.tunnel-1"), "dev_web.tunnel-1");
        assert_eq!(sanitize("a b/c"), "a-b-c");
    }

    #[test]
    fn parse_signal_name_defaults_to_sigterm() {
        assert!(matches!(parse_signal_name("SIGINT"), Signal::SIGINT));
        assert!(matches!(parse_signal_name("sighup"), Signal::SIGHUP));
        assert!(matches!(parse_signal_name("nonsense"), Signal::SIGTERM));
    }

    #[test]
    fn parse_restart_policy_falls_back_to_on_failure() {
        assert!(matches!(
            parse_restart_policy("always"),
            RestartPolicy::Always
        ));
        assert!(matches!(
            parse_restart_policy("never"),
            RestartPolicy::Never
        ));
        assert!(matches!(
            parse_restart_policy("garbage"),
            RestartPolicy::OnFailure
        ));
    }

    #[test]
    fn host_process_dir_lives_under_runtime() {
        let dir = host_process_dir(Path::new("/tmp/repo"), "web");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web"));
    }

    #[test]
    fn managed_host_process_dir_is_profile_scoped() {
        let dir = managed_host_process_dir(Path::new("/checkout"), "web", "dev");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web/dev"));
    }

    /// `HostProcessSignal` ↔ name round-trip stays in sync with the
    /// supervisor's signal parser.
    #[test]
    fn host_process_signal_names_match_parser() {
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigterm.as_str()),
            Signal::SIGTERM
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigint.as_str()),
            Signal::SIGINT
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sighup.as_str()),
            Signal::SIGHUP
        ));
        assert!(matches!(
            parse_signal_name(HostProcessSignal::Sigkill.as_str()),
            Signal::SIGKILL
        ));
    }

    #[test]
    fn process_identity_absence_includes_esrch() {
        assert!(process_identity_is_absent(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        assert!(process_identity_is_absent(
            &std::io::Error::from_raw_os_error(libc::ESRCH)
        ));
        assert!(!process_identity_is_absent(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
    }

    fn legacy_listener_state_json(status: &str) -> serde_json::Value {
        serde_json::json!({
            "schema": HOST_LISTENER_STATE_SCHEMA,
            "owner": "main|web|dev|app",
            "runtime_generation": "runtime",
            "generation": "generation",
            "status": status,
            "address": null,
            "internal_url": null,
            "public_url": null,
            "route_domain": "app.test",
            "supervisor_pid": 1,
            "supervisor_boot_identity": "boot",
            "supervisor_start_identity": {
                "platform": "macos",
                "start_seconds": 1,
                "start_microseconds": 0
            },
            "child_pid": null,
            "child_boot_identity": null,
            "child_start_identity": null,
            "listener_pid": null,
            "listener_boot_identity": null,
            "listener_start_identity": null,
            "route_owner": null
        })
    }

    /// States written before the diagnostic field existed still parse, and an
    /// absent diagnostic is not serialized, so the existing shape is unchanged.
    #[test]
    fn host_process_listener_state_without_diagnostic_keeps_existing_shape() {
        let state: HostListenerState =
            serde_json::from_value(legacy_listener_state_json("failed")).unwrap();
        assert!(state.diagnostic.is_none());
        let rendered = serde_json::to_value(&state).unwrap();
        assert!(rendered.get("diagnostic").is_none());
    }

    #[test]
    fn host_process_listener_diagnostic_serializes_stable_bounded_tokens() {
        let diagnostic = HostListenerDiagnostic {
            report: ReportObservation::Accepted,
            claimed_address: Some("127.0.0.1:41234".parse().unwrap()),
            ownership: OwnershipObservation::ExclusiveOwnerObserved,
            candidates_inspected: Some(2),
            observed_listener_pid: Some(77),
            http_probe: Some(HttpReadinessOutcome::StatusMismatch),
            http_status: Some(503),
            route: RouteObservation::NotReached,
        };
        let value = serde_json::to_value(diagnostic).unwrap();
        assert_eq!(value["report"], "accepted");
        assert_eq!(value["ownership"], "exclusive_owner_observed");
        assert_eq!(value["http_probe"], "status_mismatch");
        assert_eq!(value["http_status"], 503);
        assert_eq!(value["route"], "not_reached");
        assert_eq!(value["claimed_address"], "127.0.0.1:41234");
        assert_eq!(value["candidates_inspected"], 2);
        assert_eq!(value["observed_listener_pid"], 77);
        assert_eq!(value.as_object().unwrap().len(), 8);
        let round_trip: HostListenerDiagnostic = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, diagnostic);
    }

    /// An identity that cannot be read stays unavailable; a readable identity
    /// that differs from the recorded one is reported as changed.
    #[test]
    fn host_process_identity_failures_keep_unavailable_and_changed_apart() {
        let current = read_live_process_identity(std::process::id()).expect("current identity");
        let changed =
            checked_child_identity(std::process::id(), "not-this-boot", &current.start_identity);
        assert!(matches!(changed, Err(IdentityFailure::Changed(_))));
        let unavailable = checked_child_identity(
            i32::MAX as u32,
            &current.boot_identity,
            &current.start_identity,
        );
        assert!(matches!(unavailable, Err(IdentityFailure::Unavailable(_))));
        assert!(checked_child_identity(
            std::process::id(),
            &current.boot_identity,
            &current.start_identity
        )
        .is_ok());
    }

    #[test]
    fn host_process_listener_diagnostic_summary_names_last_observation() {
        let diagnostic = HostListenerDiagnostic {
            report: ReportObservation::Accepted,
            ownership: OwnershipObservation::ExclusiveOwnerObserved,
            http_probe: Some(HttpReadinessOutcome::StatusMismatch),
            http_status: Some(503),
            ..HostListenerDiagnostic::default()
        };
        assert_eq!(
            diagnostic.summary(),
            "report=accepted, ownership=exclusive_owner_observed, http_probe=status_mismatch:503, route=not_reached"
        );
        assert_eq!(
            HostListenerDiagnostic::default().summary(),
            "report=not_observed, ownership=not_reached, http_probe=not_attempted, route=not_reached"
        );
    }

    #[test]
    fn stopping_absent_supervisor_returns_false_and_preserves_record() {
        let record = HostProcessRecord {
            schema: HOST_PROCESS_RECORD_SCHEMA.to_owned(),
            pid: i32::MAX as u32,
            boot_identity: "test-boot".to_owned(),
            start_identity: GatewayStartIdentity::Macos {
                start_seconds: 1,
                start_microseconds: 0,
            },
        };
        let result = stop_recorded_supervisor(Path::new("unused"), &record, Signal::SIGTERM, 1);
        assert!(matches!(result, Ok(false)));
    }
}
