//! Host-side processes that follow container lifecycle.
//!
//! Each `[[containers.<name>.host_processes]]` entry resolves to an
//! `EffectiveHostProcess`. Effigy runs one detached supervisor per entry.
//! A listener process serves a reserved loopback address. The supervisor waits
//! until that address answers its readiness request, publishes the route, and
//! keeps it published while the address answers and the child is alive. State,
//! logs and specs live under the checkout's runtime directory, with listener
//! state separated by container profile. Shutdown withdraws the route before
//! stopping the recorded child process group.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use effigy_containers::{
    EffectiveContainerPolicy, EffectiveHostProcess, EffectiveManagedHostListener,
    HostProcessRestart, HostProcessSignal,
};
use effigy_gateway::identity::{read_live_process_identity, GatewayStartIdentity};
use effigy_gateway::registration::deregister_managed_listener_route;
use effigy_gateway::routes::{ManagedListenerRouteOwner, RouteTable};
use nix::sys::signal::Signal;

use super::error::RunnerError;

use super::managed_listener_readiness::{probe_http_readiness, HttpReadinessOutcome};
use effigy_kernel::human::Category;
use effigy_kernel::run::RunError;
use effigy_kernel::supervise::EngineSession;

mod kernel_run;
pub(in crate::runner) use kernel_run::{
    host_process_listener_state_path, host_process_log_path, host_process_status,
    owns_supervise_payload, restart_host_processes_for_container,
    start_host_processes_for_container, stop_host_processes_for_container, CustodyStop,
    HostProcessStopReport,
};

const HOST_PROCESS_DIR: &str = ".effigy/runtime/host-processes";
const HOST_PROCESS_SPEC_SCHEMA: &str = "effigy.managed.host-process-spec.v1";
const HOST_LISTENER_STATE_SCHEMA: &str = "effigy.managed.host-listener-state.v1";
const HOST_LISTENER_BIND_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_BIND";
/// A shipped listener adapter reads these two and may write the report file.
/// Effigy only supplies them: it never reads the report, and readiness stays
/// the reserved port answering its probe. Planned removal is tracked in the
/// lane's lead backlog, once the Underlay bundle drops its adapters.
const HOST_LISTENER_REPORT_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_REPORT_FILE";
const HOST_LISTENER_GENERATION_ENV: &str = "EFFIGY_MANAGED_HOST_LISTENER_GENERATION";
/// How often a published listener's reserved port is polled, and how many
/// consecutive misses withdraw its route. A miss never restarts the child; the
/// child's own exit does.
const LISTENER_PROBE_INTERVAL: Duration = Duration::from_secs(5);
const LISTENER_PROBE_MISSES: u32 = 3;
/// How often a listener whose port has not opened yet reports progress. A cold
/// build can take minutes, and no deadline applies while it runs.
const LISTENER_START_PROGRESS_INTERVAL: Duration = Duration::from_secs(30);
/// The longest wait between replacement generations: a crash loop backs off to
/// this and stays there (effigy#341).
const RESTART_BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Bounded wait for a supervisor to publish its own activation record. This is
/// a supervision admission bound, never a readiness budget: readiness starts
/// when the listener's reserved port first answers (papercut b1bf877f).
pub(super) const SUPERVISOR_ACTIVATION_PUBLICATION_BOUND: Duration = Duration::from_secs(10);
#[cfg(debug_assertions)]
const START_PUBLICATION_DELAY_ENV: &str = "EFFIGY_TEST_HOST_PROCESS_START_PUBLICATION_DELAY_MS";

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
    /// The environment this process belongs to (052 §10.2 identity).
    #[serde(default)]
    environment: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    process_name: String,
    #[serde(default)]
    container_name: String,
    #[serde(default)]
    run: String,
    #[serde(default)]
    log_file: PathBuf,
    #[serde(default)]
    restart: HostProcessRestart,
    #[serde(default)]
    restart_delay_ms: u64,
    #[serde(default)]
    shutdown_signal: HostProcessSignal,
    #[serde(default)]
    shutdown_grace_secs: u64,
    #[serde(default)]
    checkout: Option<effigy_kernel::id::CheckoutId>,
    /// The state home this run's checkout journal was opened under.
    #[serde(default)]
    state_home: PathBuf,
}

/// The stop report a supervisor writes after it settles its own run: the
/// exact custody `StopReport` the kernel produced, so the environment's stop
/// can name the PID and the signals that were sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostProcessStopRecord {
    schema: String,
    run: String,
    custody: Option<CustodyStop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProcessDependency {
    name: String,
    owner: String,
    state_file: PathBuf,
}

/// One supervisor's latest listener observation. Unknown fields stay readable
/// so a record written by an earlier build is read once and rewritten without
/// its retired identity proofs.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The child generation's leader, so a stop that outlives its supervisor
    /// can still settle the group it started.
    child_pid: Option<u32>,
    child_boot_identity: Option<String>,
    child_start_identity: Option<GatewayStartIdentity>,
    route_owner: Option<ManagedListenerRouteOwner>,
    /// Why the last generation failed, in one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diagnostic: Option<String>,
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
    pub(in crate::runner) declared_route_domain: String,
    pub(in crate::runner) route_domain: String,
    pub(in crate::runner) route_tls: bool,
    pub(in crate::runner) state_file: String,
}

/// One restarted host process: its new run, whether it was restarted because
/// it depends on a named process, and the listener it republished.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct RestartedHostProcess {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    pub dependent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listener: Option<ManagedHostListenerResult>,
}

/// The environment's answer to `container <NAME> restart <process>...`.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct HostProcessRestartReport {
    pub environment: String,
    /// The processes this restart replaced, in restart order.
    pub processes: Vec<RestartedHostProcess>,
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
    listener_gateway_root(policy)?;
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
                )));
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

/// Gateway state root that receives managed listener routes: the caller's
/// validated private root when `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` is set,
/// otherwise the shared operator gateway.
fn listener_gateway_root(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<PathBuf>, RunnerError> {
    if !policy
        .host_processes
        .iter()
        .any(|process| process.listener.is_some())
    {
        return Ok(None);
    }
    super::gateway_command::gateway_dir().map(Some)
}

/// Re-check a recorded listener root before use. Private roots keep their
/// ownership and permission checks; the shared gateway directory is the
/// operator's own state and is not a caller-owned private root.
fn validate_listener_gateway_root(root: &Path) -> Result<(), RunnerError> {
    if root == super::gateway_command::standard_gateway_dir()? {
        return Ok(());
    }
    effigy_gateway::private_state::validate_root(root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn managed_host_process_owner(
    project_path: &str,
    policy: &EffectiveContainerPolicy,
    process: &EffectiveHostProcess,
) -> Result<String, RunnerError> {
    Ok(format!("{project_path}|{}|{}", policy.name, process.name))
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

/// Wait until the supervisor publishes the activation record for this spec.
/// That record is the barrier between supervision admission and readiness, so
/// the wait below starts from a record that belongs to this activation. A
/// previous activation of the same process keeps its own
/// `runtime_generation`, so a stale record never satisfies the wait.
fn wait_for_listener_activation(
    path: &Path,
    process_name: &str,
    runtime_generation: &str,
    timeout: Duration,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(raw) = fs::read(path) {
            if let Ok(state) = serde_json::from_slice::<HostListenerState>(&raw) {
                if state.schema == HOST_LISTENER_STATE_SCHEMA
                    && state.runtime_generation == runtime_generation
                {
                    return Ok(());
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "timed out after {}s waiting for managed host listener `{process_name}` to publish its activation",
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// The console progress line for a listener that is alive and has not opened
/// its port yet: the phase has no deadline, and the line says what ends it.
fn listener_start_progress(process_name: &str, elapsed: Duration) -> String {
    format!(
        "[host process] listener `{process_name}` is still starting after {}s: the port is closed and the child is running, so its readiness timeout has not started; `effigy container <env> down` stops it",
        elapsed.as_secs()
    )
}

/// Wait until the listener's supervisor publishes this generation's declared
/// state: `ready`, or the failure it observed. There is no deadline of the
/// caller's: a child that is still building has not opened its port, and the
/// supervisor's own readiness budget starts when it does. The supervisor is
/// what ends the wait, so a supervisor that exited without publishing is the
/// failure this wait names.
///
/// `on_wait` runs about once a second while the wait continues, so the caller
/// can publish what it waits for: the entry's own tab names the probe and its
/// elapsed time instead of a generic spinner (effigy#458).
fn wait_for_listener_state(
    path: &Path,
    process_name: &str,
    owner: &str,
    route_table_path: &Path,
    mut on_wait: impl FnMut(Instant),
) -> Result<HostListenerState, RunnerError> {
    let mut next_liveness_check = Instant::now();
    let started = Instant::now();
    let mut next_progress = started;
    let mut next_wait_report = started;
    loop {
        let now = Instant::now();
        if now >= next_wait_report {
            next_wait_report = now + Duration::from_secs(1);
            on_wait(now);
        }
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
                            "managed host listener state for `{process_name}` is ready without a published route"
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
                    return Err(RunnerError::ManagedHostReadiness {
                        process: process_name.to_owned(),
                        diagnostic: state.diagnostic.clone().unwrap_or_default(),
                    });
                }
                _ => {
                    // The supervisor's own progress line goes to the process
                    // log; this one goes to the console of whoever is waiting,
                    // so a slow start is never a silent prompt (effigy#370).
                    if Instant::now() >= next_progress {
                        next_progress = Instant::now() + LISTENER_START_PROGRESS_INTERVAL;
                        eprintln!(
                            "{}",
                            listener_start_progress(process_name, started.elapsed())
                        );
                    }
                    if Instant::now() >= next_liveness_check {
                        next_liveness_check = Instant::now() + Duration::from_secs(1);
                        if let Err(error) = read_live_process_identity(state.supervisor_pid) {
                            // Only a vanished supervisor proves nothing more
                            // will be published. An identity this user cannot
                            // read leaves the wait running.
                            if error.kind() == std::io::ErrorKind::NotFound
                                || error.raw_os_error() == Some(libc::ESRCH)
                            {
                                return Err(RunnerError::task_invocation(format!(
                                    "managed host listener `{process_name}` stopped starting: its supervisor (pid {}) exited before publishing readiness; its log names what it did",
                                    state.supervisor_pid
                                )));
                            }
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// What one dependency's published listener record says right now.
enum DependencyRecord {
    Present(Box<HostListenerState>),
    /// The record is gone: the dependency was stopped or never started.
    Missing,
    /// The record is there but cannot be read as this owner's state.
    Unreadable,
}

fn read_dependency_record(dependency: &HostProcessDependency) -> DependencyRecord {
    match fs::read(&dependency.state_file) {
        Ok(raw) => match serde_json::from_slice::<HostListenerState>(&raw) {
            Ok(state)
                if state.schema == HOST_LISTENER_STATE_SCHEMA
                    && state.owner == dependency.owner =>
            {
                DependencyRecord::Present(Box::new(state))
            }
            Ok(_) | Err(_) => DependencyRecord::Unreadable,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DependencyRecord::Missing,
        Err(_) => DependencyRecord::Unreadable,
    }
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
    }))
}

type DependencyEnvironment = (BTreeMap<String, String>, Vec<String>);

/// A dependent's view of one dependency generation.
fn dependency_generation(dependency: &HostProcessDependency, state: &HostListenerState) -> String {
    format!(
        "{}:{}:{}",
        dependency.name, state.runtime_generation, state.generation
    )
}

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
        generations.push(dependency_generation(dependency, state));
    }
    (env, generations)
}

/// The dependency generations a dependent is currently running against.
/// `None` means one dependency's record could not be read now, so nothing is
/// concluded from it.
fn dependency_generations(spec: &HostProcessRuntimeSpec) -> Option<Vec<String>> {
    let mut generations = Vec::with_capacity(spec.dependencies.len());
    for dependency in &spec.dependencies {
        match read_dependency_record(dependency) {
            DependencyRecord::Present(state) => {
                generations.push(dependency_generation(dependency, &state));
            }
            // A dependency that is gone was stopped or replaced.
            DependencyRecord::Missing => {
                generations.push(format!("{}:missing", dependency.name));
            }
            // An unreadable record is never a state change.
            DependencyRecord::Unreadable => return None,
        }
    }
    Some(generations)
}

fn wait_for_dependency_environment(
    spec: &HostProcessRuntimeSpec,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<DependencyEnvironment>, RunnerError> {
    loop {
        if stop() {
            return Ok(None);
        }
        let mut states = Vec::with_capacity(spec.dependencies.len());
        for dependency in &spec.dependencies {
            match read_dependency_record(dependency) {
                DependencyRecord::Present(state)
                    if state.status == "ready" && state.route_owner.is_some() =>
                {
                    states.push(*state);
                }
                _ => {
                    states.clear();
                    break;
                }
            }
        }
        if states.len() == spec.dependencies.len() {
            return Ok(Some(dependency_environment(spec, &states)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Debug, Clone, Copy)]
enum HostChildWait {
    Exited(i32),
    DependencyChanged,
    Shutdown,
}

/// A dependent is replaced only when its dependency's supervisor replaced its
/// child generation. A momentary probe miss or an unreadable dependency record
/// is not a change.
fn dependency_transition(
    spec: &HostProcessRuntimeSpec,
    expected_dependencies: &[String],
) -> Result<bool, RunnerError> {
    let Some(current) = dependency_generations(spec) else {
        return Ok(false);
    };
    Ok(current != expected_dependencies)
}

fn wait_for_host_child(
    child: &mut effigy_kernel::custody::OwnedProcess,
    spec: &HostProcessRuntimeSpec,
    expected_dependencies: &[String],
    stop: &mut dyn FnMut() -> bool,
) -> Result<HostChildWait, RunnerError> {
    loop {
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            return Ok(HostChildWait::Exited(exit.code));
        }
        if stop() {
            return Ok(HostChildWait::Shutdown);
        }
        if dependency_transition(spec, expected_dependencies)? {
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

/// The budget for one cleanup step of a host-process run: admission, a stop or
/// the settle. Each step takes its own, so a supervisor that has been up for
/// hours still has a whole budget when its workload needs stopping or its run
/// needs closing (effigy#357), and a long compile never spends the settle's
/// clock (effigy#358).
const HOST_PROCESS_CLEANUP_BUDGET: Duration = Duration::from_secs(120);

/// One cleanup step, with its own budget, minted when that step runs: Effigy
/// journals, stops and settles by the operation in front of it, never against a
/// clock a long-lived child has already spent.
fn cleanup_budget() -> effigy_kernel::ctx::CleanupContext {
    crate::runner::task_engine::cleanup_context(HOST_PROCESS_CLEANUP_BUDGET)
}

/// A refusal this supervisor reports to its client and returns, with its
/// category crossing the event pipe unchanged. A step that could not proceed
/// reads as unavailable, never as an invalid request (effigy#332).
fn refuse(session: &EngineSession, category: Category, detail: impl std::fmt::Display) -> RunError {
    let detail = detail.to_string();
    session.refused(category, &detail);
    RunError::reported(category, detail)
}

/// The supervisor's kernel entry (contract 052 §10.2). It admits one run
/// owned by the host process's own resource, spawns each child generation
/// through `Custody` (so the kernel receipt is the group's identity), waits for
/// declared readiness, honours the restart policy and settles once.
pub(in crate::runner) fn supervise(mut session: EngineSession) -> Result<(), RunError> {
    let spec: HostProcessRuntimeSpec = match serde_json::from_slice(session.payload()) {
        Ok(spec) => spec,
        Err(_) => {
            return Err(refuse(
                &session,
                Category::Refused,
                "the host-process payload is unreadable",
            ));
        }
    };
    if spec.schema != HOST_PROCESS_SPEC_SCHEMA || !spec.cwd.is_absolute() {
        return Err(refuse(
            &session,
            Category::Refused,
            "the host-process spec is unsupported or invalid",
        ));
    }
    let Some(checkout) = spec.checkout else {
        return Err(refuse(
            &session,
            Category::Refused,
            "the host process has no checkout identity",
        ));
    };
    let resource = match effigy_kernel::id::ResourceId::host_process(
        checkout,
        &spec.environment,
        &spec.profile,
        &spec.process_name,
    ) {
        Ok(resource) => resource,
        Err(error) => {
            return Err(refuse(
                &session,
                Category::Refused,
                format!("its resource name is unusable ({error})"),
            ))
        }
    };
    let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(Path::new(&spec.project_path)),
    };
    let root =
        match effigy_kernel::root::StateRoot::checkout_effects(&cx, &spec.state_home, &location) {
            Ok(root) => root,
            Err(error) => {
                return Err(refuse(
                    &session,
                    Category::Unavailable,
                    format!("the state root is unusable ({error})"),
                ))
            }
        };
    let invocation = match kernel_run::invocation(checkout, &resource, &spec) {
        Ok(invocation) => invocation,
        Err(error) => {
            return Err(refuse(
                &session,
                Category::Refused,
                format!("its invocation is unusable ({error})"),
            ))
        }
    };
    let next = kernel_run::recovery_invocation(&spec.environment).map_err(|error| {
        effigy_kernel::run::RunError::step_failed("host-process recovery invocation", error)
    })?;
    let executable = crate::runner::task_engine::startup_executable().map_err(|error| {
        effigy_kernel::run::RunError::step_failed("host-process startup binary", error)
    })?;
    let options = effigy_kernel::run::BeginOpts {
        id: kernel_run::fresh_run_id().map_err(|error| {
            effigy_kernel::run::RunError::step_failed("host-process run identity", error)
        })?,
        wait: effigy_kernel::gate::Wait::FailFast,
        acknowledge: Vec::new(),
        next,
        custody: Some(effigy_kernel::supervise::LifecyclePlan {
            executable,
            placement: effigy_kernel::supervise::Placement::Detached,
        }),
    };
    let mut adapter = kernel_run::HostProcessSettlement;
    let mut run = match effigy_kernel::run::Run::begin(
        &cx,
        &root,
        &invocation,
        options,
        &cleanup_budget(),
        &mut adapter,
    ) {
        Ok(run) => run,
        Err(error) => {
            session.refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    session.admitted(&mut run)?;
    let supervisor_pid = std::process::id();
    redirect_supervisor_output(&spec.log_file).map_err(|error| {
        effigy_kernel::run::RunError::step_failed("supervisor output redirection", error)
    })?;
    let watch = session.owner_watch().ok();
    let started = supervise_generations(
        &mut run,
        &cx,
        &session,
        &spec,
        supervisor_pid,
        watch.as_ref(),
    );
    let (workload, custody) = match started {
        Ok(result) => result,
        Err(error) => {
            let _ = run.interrupt(
                &cleanup_budget(),
                effigy_kernel::outcome::WorkloadOutcome::Failed { code: 1 },
            );
            let _ = write_stop_report(&spec, &run, None);
            // A store or capability failure inside the supervisor names the
            // step and its cause. It is never reported as an invalid request
            // (effigy#332).
            return Err(effigy_kernel::run::RunError::step_failed(
                format!("host process `{}` supervision", spec.process_name),
                error,
            ));
        }
    };
    if write_stop_report(&spec, &run, custody.clone()).is_err() {
        append_log_line(
            &spec.log_file,
            "[effigy host-process] the stop report could not be recorded",
        );
    }
    let outcome = match workload {
        Some(outcome) => {
            run.finish(outcome)?;
            run.settle(&cleanup_budget(), &mut adapter)?
        }
        None => {
            run.interrupt(&cleanup_budget(), stop_workload())?;
            run.settle(&cleanup_budget(), &mut adapter)?
        }
    };
    session.closed(&outcome);
    Ok(())
}

fn stop_workload() -> effigy_kernel::outcome::WorkloadOutcome {
    effigy_kernel::outcome::WorkloadOutcome::Cancelled {
        cause: effigy_kernel::ctx::StopCause::Requested,
        signal: None,
    }
}

fn write_stop_report(
    spec: &HostProcessRuntimeSpec,
    run: &effigy_kernel::run::Run<'_>,
    custody: Option<CustodyStop>,
) -> Result<(), RunnerError> {
    let record = HostProcessStopRecord {
        schema: "effigy.managed.host-process-stop.v1".to_owned(),
        run: run.id().to_string(),
        custody,
    };
    write_private_json(&stop_report_path_for(spec), &record)
}

fn stop_report_path_for(spec: &HostProcessRuntimeSpec) -> PathBuf {
    spec.listener_state_file
        .parent()
        .map(|dir| dir.join(format!("{}.stop.json", sanitize(&spec.process_name))))
        .unwrap_or_else(|| spec.log_file.with_extension("stop.json"))
}

/// The supervisor's own stdout and stderr become the process log, so every
/// child spawned with `Terminal::Inherit` writes where log following expects.
fn redirect_supervisor_output(log_path: &Path) -> Result<(), RunnerError> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    use std::os::unix::io::AsRawFd as _;
    let log = open_log(log_path)?;
    let fd = log.as_raw_fd();
    for target in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY: dup2 of a live open log descriptor onto a standard
        // descriptor; the log stays owned by `log` until both are copied.
        if unsafe { libc::dup2(fd, target) } < 0 {
            return Err(RunnerError::task_invocation_failed_write(
                log_path,
                std::io::Error::last_os_error(),
            ));
        }
    }
    Ok(())
}

/// What one supervisor generation ended with.
enum GenerationEnd {
    /// A stop was requested; the supervisor settles.
    Stopped,
    /// The process ended by itself with `code`.
    Exited(i32),
    /// A dependency's endpoint generation changed: replace regardless of the
    /// declared restart policy, as dependents always have.
    DependencyChanged,
}

/// One supervisor's generations. `None` means a stop was requested; `Some`
/// is the natural outcome of the last generation.
fn supervise_generations(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    supervisor_pid: u32,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
) -> Result<
    (
        Option<effigy_kernel::outcome::WorkloadOutcome>,
        Option<CustodyStop>,
    ),
    RunnerError,
> {
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // The first custody intent moves the run from `admitted` to `starting`;
    // `activate_once` publishes `active` once a generation is running.
    let mut active = false;
    let mut outcome = None;
    let mut last = None;
    // A crash-looping child waits longer after each replacement generation, so
    // no restart limit is needed to keep it from spinning (effigy#341).
    let mut backoff = Duration::ZERO;
    loop {
        if stop_requested(run, watch) {
            shutdown.store(true, Ordering::SeqCst);
            break;
        }
        let generation_started = Instant::now();
        let end = if spec.listener.is_some() {
            supervise_managed_host_listener(
                run,
                cx,
                session,
                spec,
                supervisor_pid,
                watch,
                &mut active,
                &shutdown,
                &mut last,
            )?
        } else {
            supervise_plain_host_process(
                run,
                cx,
                session,
                spec,
                watch,
                &mut active,
                &shutdown,
                &mut last,
            )?
        };
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        // A generation that ran for longer than the backoff cap was healthy:
        // the next replacement starts from the declared delay again.
        if generation_started.elapsed() >= RESTART_BACKOFF_CAP {
            backoff = Duration::ZERO;
        }
        match end {
            GenerationEnd::Stopped => break,
            GenerationEnd::DependencyChanged => {
                wait_or_break(
                    &shutdown,
                    Duration::from_millis(spec.restart_delay_ms),
                    || stop_requested(run, watch),
                );
            }
            GenerationEnd::Exited(code) => {
                if !spec.restart.restarts_after(code) {
                    if spec.listener.is_some() {
                        write_terminal_listener_state(spec, supervisor_pid)?;
                    }
                    outcome = Some(if code == 0 {
                        effigy_kernel::outcome::WorkloadOutcome::Succeeded
                    } else {
                        effigy_kernel::outcome::WorkloadOutcome::Failed { code }
                    });
                    break;
                }
                wait_or_break(
                    &shutdown,
                    restart_backoff(&mut backoff, spec.restart_delay_ms),
                    || stop_requested(run, watch),
                );
            }
        }
    }
    Ok((outcome, last))
}

/// The replacement wait after one ended generation: `restart_delay_ms` for the
/// first, then doubled, capped at [`RESTART_BACKOFF_CAP`].
fn restart_backoff(backoff: &mut Duration, restart_delay_ms: u64) -> Duration {
    let base = Duration::from_millis(restart_delay_ms);
    let delay = if backoff.is_zero() {
        base
    } else {
        (*backoff * 2).min(RESTART_BACKOFF_CAP)
    };
    *backoff = delay.max(Duration::from_millis(1));
    delay
}

/// Emits the run's `Active` phase once the first generation is spawned, so
/// the owning client's detach has its barrier.
fn activate_once(
    active: &mut bool,
    run: &mut effigy_kernel::run::Run<'_>,
    session: &EngineSession,
) -> Result<(), RunnerError> {
    if *active {
        return Ok(());
    }
    run.advance(effigy_kernel::run::Phase::Active)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    session.active();
    *active = true;
    Ok(())
}

/// A stop is any fenced request, a routed interrupt, or the owning client's
/// death before the supervisor detached.
fn stop_requested(
    run: &effigy_kernel::run::Run<'_>,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
) -> bool {
    if !crate::runner::task_engine::routed_signals().is_empty() {
        return true;
    }
    let requests = run.requests().unwrap_or_default();
    if requests.iter().any(|request| {
        matches!(
            request.action,
            effigy_kernel::run::RequestAction::Stop | effigy_kernel::run::RequestAction::Escalate
        )
    }) {
        return true;
    }
    // A detach request precedes the owning pipe's close, so EOF after it is
    // the detached run, not a lost owner.
    let detached = requests
        .iter()
        .any(|request| matches!(request.action, effigy_kernel::run::RequestAction::Detach));
    !detached && watch.is_some_and(effigy_kernel::supervise::OwnerWatch::lost)
}

/// The plain (non-listener) generation loop: spawn, wait, stop or restart.
#[allow(clippy::too_many_arguments)]
fn supervise_plain_host_process(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
    active: &mut bool,
    shutdown: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    last: &mut Option<CustodyStop>,
) -> Result<GenerationEnd, RunnerError> {
    let dependencies = {
        let mut stop = || stop_requested(run, watch);
        wait_for_dependency_environment(spec, &mut stop)?
    };
    let Some((dependency_env, dependency_generations)) = dependencies else {
        shutdown.store(true, Ordering::SeqCst);
        return Ok(GenerationEnd::Stopped);
    };
    let dependency_env = dependency_env
        .into_iter()
        .collect::<Vec<(String, String)>>();
    let mut child = match spawn_generation(run, cx, spec, &dependency_env, None) {
        Ok(child) => child,
        Err(error) => {
            append_log_line(
                &spec.log_file,
                &format!(
                    "[effigy host-process] failed to spawn `{}`: {error}",
                    spec.run
                ),
            );
            return Ok(GenerationEnd::Exited(127));
        }
    };
    activate_once(active, run, session)?;
    append_log_line(
        &spec.log_file,
        &format!(
            "[effigy host-process] started `{}` (pid {})",
            spec.process_name,
            child.identity().identity.pid
        ),
    );
    let wait = {
        let mut stop = || stop_requested(run, watch);
        wait_for_host_child(&mut child, spec, &dependency_generations, &mut stop)?
    };
    let stopping = matches!(wait, HostChildWait::Shutdown)
        || stop_requested(run, watch)
        || shutdown.load(Ordering::SeqCst);
    if stopping {
        shutdown.store(true, Ordering::SeqCst);
    }
    if let Some(report) = stop_generation(run, spec, &mut child, stopping) {
        *last = Some(report.clone());
        append_log_line(
            &spec.log_file,
            &format!(
                "[effigy host-process] `{}` exited (code {})",
                spec.process_name,
                report.exit.unwrap_or(-1)
            ),
        );
    }
    Ok(match wait {
        _ if stopping => GenerationEnd::Stopped,
        HostChildWait::DependencyChanged => GenerationEnd::DependencyChanged,
        HostChildWait::Exited(code) => GenerationEnd::Exited(code),
        HostChildWait::Shutdown => GenerationEnd::Stopped,
    })
}

fn signal_for(kind: HostProcessSignal) -> Signal {
    match kind {
        HostProcessSignal::Sigterm => Signal::SIGTERM,
        HostProcessSignal::Sigint => Signal::SIGINT,
        HostProcessSignal::Sighup => Signal::SIGHUP,
        HostProcessSignal::Sigkill => Signal::SIGKILL,
    }
}

/// Stops one generation through kernel custody and returns its `StopReport`
/// projection. `shutdown_signal` is delivered first; the kernel's own TERM,
/// grace and KILL escalation follow it.
fn stop_generation(
    run: &mut effigy_kernel::run::Run<'_>,
    spec: &HostProcessRuntimeSpec,
    child: &mut effigy_kernel::custody::OwnedProcess,
    stopping: bool,
) -> Option<CustodyStop> {
    let receipt = child.identity();
    let pid = receipt.identity.pid;
    if stopping {
        let signal = signal_for(spec.shutdown_signal);
        if signal != Signal::SIGTERM {
            let _ = child.forward(signal as i32);
        }
    }
    let report = child.stop(&cleanup_budget());
    let exit = report.leader.verified().map(|exit| exit.code);
    let group_closed = matches!(
        report.group,
        effigy_kernel::observe::Observed::Verified(..)
            | effigy_kernel::observe::Observed::Absent(_)
    );
    let custody = CustodyStop {
        pid,
        exit,
        group_closed,
        signals: report.signals.iter().map(|signal| signal.signal).collect(),
        residue: report
            .residue
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        capture_truncated: matches!(
            report.capture,
            effigy_kernel::custody::CaptureState::Truncated(_)
        ),
    };
    if let Some(exit) = exit {
        let _ = child.complete(run, effigy_kernel::custody::Exit { code: exit });
    }
    Some(custody)
}

/// One kernel generation: argv, cwd, complete environment, grace and
/// terminal. Output joins the process log through the supervisor's own fds.
fn spawn_generation(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    spec: &HostProcessRuntimeSpec,
    dependency_env: &[(String, String)],
    listener: Option<(&str, &Path, &str)>,
) -> Result<effigy_kernel::custody::OwnedProcess, effigy_kernel::custody::SpawnError> {
    use effigy_kernel::custody::{Custody, Lifetime, ProcessSpec, Terminal};
    // A user application is a workload: the ambient environment it inherits
    // carries no session vault access, whatever this supervisor was started
    // with (effigy#338, K12).
    let environment = child_environment(
        crate::runner::secret_session::workload_ambient_environment(),
        spec,
        dependency_env,
        listener,
    );
    let process = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(spec.run.clone()),
        ],
        cwd: spec.cwd.clone(),
        env: environment,
        grace: Duration::from_secs(spec.shutdown_grace_secs.max(1)),
        lifetime: Lifetime::Open(effigy_kernel::ctx::OpenReason::DevServer),
        terminal: Terminal::Inherit { capture: false },
        label: spec.process_name.clone(),
    };
    Custody::spawn_auxiliary(run, process, cx)
}

/// The complete environment one generation receives, with its two inputs
/// supplied so the precedence is provable without mutating this process's
/// environment.
///
/// Highest first: the process's declared `env`, its dependency endpoints, the
/// reserved listener `HOST`/`PORT`, Effigy's listener and process identity
/// variables, then the ambient environment the supervisor was started with.
/// `HOST`/`PORT` from a reserved bind replace an ambient value because the
/// process must serve the port Effigy reserved; only a value the manifest
/// itself declares outranks them.
fn child_environment(
    ambient: impl Iterator<Item = (OsString, OsString)>,
    spec: &HostProcessRuntimeSpec,
    dependency_env: &[(String, String)],
    listener: Option<(&str, &Path, &str)>,
) -> BTreeMap<OsString, effigy_kernel::custody::EnvValue> {
    use effigy_kernel::custody::EnvValue;
    let mut environment: BTreeMap<OsString, EnvValue> = ambient
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect();
    for (key, value) in &spec.env {
        environment.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    for (key, value) in dependency_env {
        environment.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    environment.insert(
        OsString::from("EFFIGY_HOST_PROCESS_CONTAINER"),
        EnvValue::Plain(OsString::from(spec.container_name.clone())),
    );
    environment.insert(
        OsString::from("EFFIGY_HOST_PROCESS_NAME"),
        EnvValue::Plain(OsString::from(spec.process_name.clone())),
    );
    if let Some((bind, report_path, generation)) = listener {
        environment.insert(
            OsString::from(HOST_LISTENER_BIND_ENV),
            EnvValue::Plain(OsString::from(bind)),
        );
        // A shipped adapter reads these two. Effigy never reads the report
        // back: it is a write-only courtesy path for existing adapters.
        environment.insert(
            OsString::from(HOST_LISTENER_REPORT_ENV),
            EnvValue::Plain(report_path.as_os_str().to_owned()),
        );
        environment.insert(
            OsString::from(HOST_LISTENER_GENERATION_ENV),
            EnvValue::Plain(OsString::from(generation)),
        );
        // Effigy assigned this listener its concrete port (the OS picked it),
        // so the process receives it as `HOST`/`PORT` (effigy#321, effigy#346).
        // That address outranks an ambient value, because the process must
        // serve the port Effigy bound on its behalf; only a value the manifest
        // itself declares is the process's own decision and wins.
        if let Ok(address) = bind.parse::<SocketAddr>() {
            for (key, value) in [
                ("HOST", address.ip().to_string()),
                ("PORT", address.port().to_string()),
            ] {
                if !spec.env.contains_key(key) {
                    environment.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
                }
            }
        }
    }
    environment
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildProcessIdentity {
    pid: u32,
    boot_identity: String,
    start_identity: GatewayStartIdentity,
}

/// Fixture barrier for proofs about the activation-publication seam. Debug
/// builds only, and inert unless the suite asks for it, so production startup
/// never sleeps here.
#[cfg(debug_assertions)]
fn hold_start_publication() {
    let Some(millis) = std::env::var(START_PUBLICATION_DELAY_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    else {
        return;
    };
    thread::sleep(Duration::from_millis(millis.min(30_000)));
}

/// One child generation's published listener state. Every write is one of
/// these, so a generation's address, route claim and child identity cannot
/// drift apart between writes.
struct ListenerGeneration<'a> {
    spec: &'a HostProcessRuntimeSpec,
    supervisor_pid: u32,
    generation: String,
    child: Option<ChildProcessIdentity>,
    address: Option<SocketAddr>,
    route_owner: Option<ManagedListenerRouteOwner>,
    diagnostic: Option<String>,
}

impl<'a> ListenerGeneration<'a> {
    fn new(spec: &'a HostProcessRuntimeSpec, supervisor_pid: u32, generation: &str) -> Self {
        Self {
            spec,
            supervisor_pid,
            generation: generation.to_owned(),
            child: None,
            address: None,
            route_owner: None,
            diagnostic: None,
        }
    }

    fn record(&self, status: &str) -> HostListenerState {
        let route_domain = self
            .spec
            .listener
            .as_ref()
            .map(|listener| listener.route_domain.clone());
        let public_url = match (
            self.address,
            route_domain.as_deref(),
            self.route_owner.as_ref(),
        ) {
            (Some(_), Some(domain), Some(owner)) => Some(format!(
                "{}://{domain}",
                if owner.route_tls { "https" } else { "http" }
            )),
            _ => None,
        };
        HostListenerState {
            schema: HOST_LISTENER_STATE_SCHEMA.to_owned(),
            owner: self.spec.owner.clone(),
            runtime_generation: self.spec.runtime_generation.clone(),
            generation: self.generation.clone(),
            status: status.to_owned(),
            address: self.address.map(|address| address.to_string()),
            internal_url: self.address.map(|address| format!("http://{address}")),
            public_url,
            route_domain,
            supervisor_pid: self.supervisor_pid,
            child_pid: self.child.as_ref().map(|child| child.pid),
            child_boot_identity: self.child.as_ref().map(|child| child.boot_identity.clone()),
            child_start_identity: self
                .child
                .as_ref()
                .map(|child| child.start_identity.clone()),
            route_owner: self.route_owner.clone(),
            diagnostic: self.diagnostic.clone(),
        }
    }

    /// Write this generation's observation under `status`. The only statuses a
    /// generation writes are its start, readiness, withdrawal and failure.
    fn write(&self, status: &str) -> Result<(), RunnerError> {
        write_listener_state(&self.spec.listener_state_file, self.record(status))
    }
}

/// The report path one generation's shipped adapter is handed: in the
/// process's runtime directory, beside its log. Effigy never reads the file; it
/// is removed with the generation.
struct ListenerReportPath(PathBuf);

impl ListenerReportPath {
    fn for_generation(
        spec: &HostProcessRuntimeSpec,
        generation: &str,
    ) -> Result<Self, RunnerError> {
        let dir = spec
            .log_file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .or_else(|| spec.listener_state_file.parent())
            .ok_or_else(|| {
                RunnerError::task_invocation("managed listener runtime path has no directory")
            })?;
        Ok(Self(dir.join(format!(
            "{}.{}.listener-report.json",
            sanitize(&spec.process_name),
            generation
        ))))
    }
}

impl Drop for ListenerReportPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// A completed teardown makes no endpoint or child claim: the terminal record
/// must not retain a generation an explicit start would have to reconcile.
fn write_terminal_listener_state(
    spec: &HostProcessRuntimeSpec,
    supervisor_pid: u32,
) -> Result<(), RunnerError> {
    // A generation that failed keeps the one line that explains it. Only this
    // supervisor's own last generation counts: a later activation writes its
    // own `starting` record before it can fail.
    if fs::read(&spec.listener_state_file)
        .ok()
        .and_then(|raw| serde_json::from_slice::<HostListenerState>(&raw).ok())
        .is_some_and(|state| state.status == "failed" && state.supervisor_pid == supervisor_pid)
    {
        return Ok(());
    }
    ListenerGeneration::new(spec, supervisor_pid, "").write("stopped")
}

#[allow(clippy::too_many_arguments)]
fn supervise_managed_host_listener(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    session: &EngineSession,
    spec: &HostProcessRuntimeSpec,
    supervisor_pid: u32,
    watch: Option<&effigy_kernel::supervise::OwnerWatch>,
    active: &mut bool,
    shutdown: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    last: &mut Option<CustodyStop>,
) -> Result<GenerationEnd, RunnerError> {
    let listener = spec.listener.as_ref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener supervisor has no listener declaration")
    })?;
    let address = listener.bind.parse::<SocketAddr>().map_err(|error| {
        RunnerError::task_invocation(format!("managed listener bind address is invalid: {error}"))
    })?;
    let log_path = spec.log_file.clone();
    #[cfg(debug_assertions)]
    hold_start_publication();

    if stop_requested(run, watch) || shutdown.load(Ordering::SeqCst) {
        shutdown.store(true, Ordering::SeqCst);
        ListenerGeneration::new(spec, supervisor_pid, "").write("stopped")?;
        return Ok(GenerationEnd::Stopped);
    }
    // Publish this activation's record before waiting on dependencies or
    // spawning a child. Callers wait for it as the barrier between supervision
    // admission and readiness, so the dependency wait is not charged to it.
    let generation = random_token()?;
    let report = ListenerReportPath::for_generation(spec, &generation)?;
    let mut state = ListenerGeneration::new(spec, supervisor_pid, &generation);
    state.write("starting")?;
    let dependencies = {
        let mut stop = || stop_requested(run, watch);
        wait_for_dependency_environment(spec, &mut stop)?
    };
    let Some((dependency_env, dependency_generations)) = dependencies else {
        state.write("stopped")?;
        shutdown.store(true, Ordering::SeqCst);
        return Ok(GenerationEnd::Stopped);
    };
    let dependency_env = dependency_env
        .into_iter()
        .collect::<Vec<(String, String)>>();
    if let Err(error) = reserved_port_is_free(&listener.bind) {
        return finish_failed_generation(
            &mut state,
            &log_path,
            spec.process_name.as_str(),
            error.to_string(),
        );
    }
    let mut child = match spawn_generation(
        run,
        cx,
        spec,
        &dependency_env,
        Some((listener.bind.as_str(), &report.0, generation.as_str())),
    ) {
        Ok(child) => child,
        Err(error) => {
            return finish_failed_generation(
                &mut state,
                &log_path,
                spec.process_name.as_str(),
                format!("managed listener could not start: {error}"),
            )
        }
    };
    activate_once(active, run, session)?;
    let pid = child.identity().identity.pid;
    // The leader's exact identity lets a stop that outlives this supervisor
    // settle the group it started. An identity that cannot be read here only
    // costs that guard; the generation still serves.
    match read_live_process_identity(pid) {
        Ok(live) => {
            state.child = Some(ChildProcessIdentity {
                pid,
                boot_identity: live.boot_identity,
                start_identity: live.start_identity,
            });
        }
        Err(error) => append_log_line(
            &log_path,
            &format!(
                "[effigy host-process] managed listener child identity is unavailable: {error}"
            ),
        ),
    }
    state.write("starting")?;
    append_log_line(
        &log_path,
        &format!(
            "[effigy host-process] started managed listener `{}` generation {generation} (pid {pid})",
            spec.process_name
        ),
    );

    let ready = {
        let mut stop = || stop_requested(run, watch) || shutdown.load(Ordering::SeqCst);
        wait_for_ready_listener(address, listener, &log_path, &mut stop, &mut child)
    };
    if let Err(error) = ready {
        let stopped = matches!(error, ListenerStartupFailure::Stopped);
        if let Some(report) = stop_generation(run, spec, &mut child, true) {
            *last = Some(report);
        }
        state.child = None;
        append_log_line(
            &log_path,
            &format!("[effigy host-process] listener failed: {error}"),
        );
        if stopped {
            shutdown.store(true, Ordering::SeqCst);
            write_terminal_listener_state(spec, supervisor_pid)?;
            return Ok(GenerationEnd::Stopped);
        }
        state.diagnostic = Some(error.to_string());
        state.write("failed")?;
        return Ok(GenerationEnd::Exited(1));
    }

    // A dependency may have been replaced while this generation was becoming
    // ready; the dependent is replaced before it publishes.
    if dependency_transition(spec, &dependency_generations)? {
        state.write("unavailable")?;
        if let Some(report) = stop_generation(run, spec, &mut child, true) {
            *last = Some(report);
        }
        state.child = None;
        if stop_requested(run, watch) {
            write_terminal_listener_state(spec, supervisor_pid)?;
            shutdown.store(true, Ordering::SeqCst);
            return Ok(GenerationEnd::Stopped);
        }
        return Ok(GenerationEnd::DependencyChanged);
    }
    if let Err(error) = prepare_managed_listener_route(spec, listener) {
        let _ = stop_generation(run, spec, &mut child, true);
        state.child = None;
        return finish_failed_generation(
            &mut state,
            &log_path,
            spec.process_name.as_str(),
            format!("listener route preparation failed: {error}"),
        );
    }
    let owner = ManagedListenerRouteOwner {
        owner: spec.owner.clone(),
        runtime_generation: spec.runtime_generation.clone(),
        generation: generation.clone(),
        address: address.to_string(),
        route_tls: listener.route_tls,
    };
    state.address = Some(address);
    state.route_owner = Some(owner.clone());
    if let Err(error) = publish_managed_listener_route(spec, listener, &owner) {
        let _ = stop_generation(run, spec, &mut child, true);
        state.child = None;
        state.route_owner = None;
        return finish_failed_generation(
            &mut state,
            &log_path,
            spec.process_name.as_str(),
            format!("route publication failed: {error}"),
        );
    }
    state.write("ready")?;
    append_log_line(
        &log_path,
        &format!(
            "[effigy host-process] listener `{}` is ready at {address}",
            spec.process_name
        ),
    );

    // The route stays published while this run is alive and the port answers.
    // A miss only withdraws it; nothing restarts the child on a probe miss.
    let mut liveness = ListenerLiveness::new(address);
    let mut stopping = false;
    let exit = loop {
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            break Some(exit.code);
        }
        if stop_requested(run, watch) {
            shutdown.store(true, Ordering::SeqCst);
            stopping = true;
            break None;
        }
        if dependency_transition(spec, &dependency_generations)? {
            break None;
        }
        match liveness.poll() {
            Some(true) => {
                // The port answers again: republish the route this generation
                // had withdrawn. A failed republication keeps the generation
                // withdrawn and is retried while the port keeps answering, so
                // a gateway store that is briefly unavailable never costs the
                // child and never leaves it unreachable.
                match publish_managed_listener_route(spec, listener, &owner) {
                    Ok(()) => {
                        state.route_owner = Some(owner.clone());
                        state.write("ready")?;
                        append_log_line(
                            &log_path,
                            &format!(
                                "[effigy host-process] listener `{}` answers again at {address}; route republished",
                                spec.process_name
                            ),
                        );
                    }
                    Err(error) => {
                        // The answer did not publish a route, so the next probe
                        // of the still-answering port is the retry.
                        liveness.publication_failed();
                        state.route_owner = None;
                        state.write("unavailable")?;
                        append_log_line(
                            &log_path,
                            &format!(
                                "[effigy host-process] listener `{}` answers at {address} but its route could not be republished: {error}",
                                spec.process_name
                            ),
                        );
                    }
                }
            }
            Some(false) => {
                // Three consecutive misses: withdraw the route and keep the
                // child running. A later answer republishes it.
                withdraw_managed_listener_route(spec, listener, &owner)?;
                state.route_owner = None;
                state.write("unavailable")?;
                append_log_line(
                    &log_path,
                    &format!(
                        "[effigy host-process] listener `{}` is not answering at {address}; route withdrawn",
                        spec.process_name
                    ),
                );
            }
            None => {}
        }
        thread::sleep(Duration::from_millis(50));
    };

    // Withdraw before stopping, so a successor never shares a route with the
    // generation it replaces.
    withdraw_managed_listener_route(spec, listener, &owner)?;
    state.route_owner = None;
    state.write("unavailable")?;
    if let Some(report) = stop_generation(run, spec, &mut child, stopping) {
        *last = Some(report);
    }
    state.child = None;
    if stopping {
        write_terminal_listener_state(spec, supervisor_pid)?;
        return Ok(GenerationEnd::Stopped);
    }

    // The leader has been reaped by custody. Observation excludes it and any
    // zombie outright, so only another live process keeps this generation
    // unavailable.
    let remaining_group_members = effigy_process::surviving_group_member_ids(pid).map_err(
        |error| {
            RunnerError::task_invocation(format!(
                "managed child group is uncertain after its leader exited; retaining the unavailable generation: {error}"
            ))
        },
    )?;
    if !remaining_group_members.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "managed child leader exited while process group {pid} still has surviving members {remaining_group_members:?}; route withdrawn and generation retained"
        )));
    }
    if shutdown.load(Ordering::SeqCst) {
        write_terminal_listener_state(spec, supervisor_pid)?;
        return Ok(GenerationEnd::Stopped);
    }
    // A generation that may be replaced leaves `unavailable`; the supervisor
    // writes the terminal `stopped` record only when the policy ends here.
    Ok(match exit {
        Some(code) => GenerationEnd::Exited(code),
        None => GenerationEnd::DependencyChanged,
    })
}

/// Record one generation that failed before it could serve, and report the
/// generation as a failed exit so the declared restart policy applies.
fn finish_failed_generation(
    state: &mut ListenerGeneration<'_>,
    log_path: &Path,
    process_name: &str,
    message: String,
) -> Result<GenerationEnd, RunnerError> {
    append_log_line(
        log_path,
        &format!("[effigy host-process] listener `{process_name}` failed: {message}"),
    );
    state.diagnostic = Some(message);
    state.write("failed")?;
    Ok(GenerationEnd::Exited(1))
}

/// A reserved port that already answers belongs to another process. This is
/// the only "someone else owns it" check: fail the start naming the port,
/// rather than serving someone else's socket as this child's.
fn reserved_port_is_free(bind: &str) -> Result<(), RunnerError> {
    let Ok(address) = bind.parse::<SocketAddr>() else {
        return Ok(());
    };
    if address.port() == 0 {
        return Ok(());
    }
    match std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
        Ok(_) => Err(RunnerError::task_invocation(format!(
            "managed listener port {address} already answers before its child starts; another process holds it"
        ))),
        Err(_) => Ok(()),
    }
}

/// Wait until the reserved address answers the declared readiness request.
///
/// Two phases, because a port that has not opened yet is a build, not a
/// failure. While the port is closed and the child is alive the wait has no
/// deadline and reports progress every [`LISTENER_START_PROGRESS_INTERVAL`];
/// the child exiting is the failure. Once the port first accepts a connection,
/// `readiness_timeout_secs` bounds reaching the declared status.
fn wait_for_ready_listener(
    address: SocketAddr,
    config: &EffectiveManagedHostListener,
    log_path: &Path,
    stop: &mut dyn FnMut() -> bool,
    child: &mut effigy_kernel::custody::OwnedProcess,
) -> Result<(), ListenerStartupFailure> {
    let timeout = Duration::from_secs(config.readiness_timeout_secs);
    let started = Instant::now();
    let mut answered_at = None;
    let mut next_progress = started + LISTENER_START_PROGRESS_INTERVAL;
    loop {
        if stop() {
            return Err(ListenerStartupFailure::Stopped);
        }
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            return Err(RunnerError::task_invocation(if exit.code == 0 {
                format!(
                    "managed listener at {address} daemonised: its leader exited with code 0 \
                     before it answered {}, so a successor process holds the port and Effigy \
                     supervises only the group it started",
                    config.readiness_path
                )
            } else {
                format!(
                    "managed listener at {address} exited with code {} before it answered {}",
                    exit.code, config.readiness_path
                )
            })
            .into());
        }
        let probe = probe_http_readiness(address, &config.route_domain, config);
        if probe.outcome == HttpReadinessOutcome::Ready {
            return Ok(());
        }
        let now = Instant::now();
        if probe.outcome == HttpReadinessOutcome::ConnectFailed {
            if now >= next_progress {
                next_progress = now + LISTENER_START_PROGRESS_INTERVAL;
                append_log_line(
                    log_path,
                    &format!(
                        "[effigy host-process] listener at {address} is still starting after {}s: the port is closed and the child is running, so the {}s readiness timeout has not started",
                        started.elapsed().as_secs(),
                        timeout.as_secs()
                    ),
                );
            }
        } else if now.duration_since(*answered_at.get_or_insert(now)) >= timeout {
            return Err(listener_readiness_timeout(address, config, timeout, probe.outcome).into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn listener_readiness_timeout(
    address: SocketAddr,
    config: &EffectiveManagedHostListener,
    timeout: Duration,
    last: HttpReadinessOutcome,
) -> RunnerError {
    RunnerError::task_invocation(format!(
        "timed out after {}s waiting for the managed listener at {address} to answer {} with {} once its port opened ({})",
        timeout.as_secs(),
        config.readiness_path,
        config.readiness_status,
        last.as_str()
    ))
}

/// Polls the reserved port while one generation serves. Three consecutive
/// misses withdraw the route; the next answer republishes it.
struct ListenerLiveness {
    address: SocketAddr,
    misses: u32,
    next_probe: Instant,
    published: bool,
}

impl ListenerLiveness {
    fn new(address: SocketAddr) -> Self {
        Self {
            address,
            misses: 0,
            next_probe: Instant::now() + LISTENER_PROBE_INTERVAL,
            published: true,
        }
    }

    /// `Some(true)` republishes, `Some(false)` withdraws, `None` is no change.
    fn poll(&mut self) -> Option<bool> {
        let now = Instant::now();
        if now < self.next_probe {
            return None;
        }
        self.next_probe = now + LISTENER_PROBE_INTERVAL;
        if std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(300)).is_ok() {
            self.misses = 0;
            return (!self.published).then(|| {
                self.published = true;
                true
            });
        }
        self.misses += 1;
        if self.published && self.misses >= LISTENER_PROBE_MISSES {
            self.published = false;
            return Some(false);
        }
        None
    }

    /// The route is not published after all, whatever the last answer decided.
    /// A port that is still answering is the next retry, so a failed
    /// republication is never left withdrawn until the child next goes silent.
    fn publication_failed(&mut self) {
        self.published = false;
    }
}

/// Keeps a diagnostic observation at the point it is seen, then passes the
/// original verification error through unchanged.
enum ListenerStartupFailure {
    Stopped,
    Failed(RunnerError),
}

impl From<RunnerError> for ListenerStartupFailure {
    fn from(error: RunnerError) -> Self {
        Self::Failed(error)
    }
}

impl std::fmt::Display for ListenerStartupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => {
                formatter.write_str("managed listener startup was stopped before readiness")
            }
            Self::Failed(error) => error.fmt(formatter),
        }
    }
}

/// Withdraw this generation's route. A stale stop or exit cannot remove a
/// successor's route: the deregistration names the owner and generation.
fn withdraw_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
    owner: &ManagedListenerRouteOwner,
) -> Result<(), RunnerError> {
    deregister_managed_listener_route(
        &listener_route_table_path(spec)?,
        &spec.project_path,
        &listener.route_domain,
        &spec.owner,
        &owner.generation,
    )
    .map(|_| ())
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn publish_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
    owner: &ManagedListenerRouteOwner,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no gateway state root")
    })?;
    validate_listener_gateway_root(root)?;
    let route_table_path = root.join("routes.json");
    let address = owner.address.parse::<SocketAddr>().map_err(|_| {
        RunnerError::task_invocation("managed host listener owner has an invalid address")
    })?;
    // The same checkout path replaces its own route, whether the previous
    // generation is alive or dead. Another checkout's live claim is refused
    // inside the same locked transaction.
    effigy_gateway::registration::register_managed_listener_route(
        &route_table_path,
        &spec.project_path,
        &listener.route_domain,
        &address.to_string(),
        listener.route_tls,
        owner.clone(),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn prepare_managed_listener_route(
    spec: &HostProcessRuntimeSpec,
    listener: &EffectiveManagedHostListener,
) -> Result<(), RunnerError> {
    let root = spec.gateway_root.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("managed listener route has no gateway state root")
    })?;
    validate_listener_gateway_root(root)?;
    if listener.route_tls {
        super::gateway_command::ensure_gateway_tls_cert(&listener.route_domain)?;
    }
    Ok(())
}

fn listener_route_table_path(spec: &HostProcessRuntimeSpec) -> Result<PathBuf, RunnerError> {
    spec.gateway_root
        .as_deref()
        .map(|root| root.join("routes.json"))
        .ok_or_else(|| RunnerError::task_invocation("listener gateway root is unavailable"))
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

fn open_log(path: &Path) -> Result<std::fs::File, RunnerError> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

pub(in crate::runner) fn append_log_line(path: &Path, line: &str) {
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

/// The replacement wait after one ended generation. A stop request ends it at
/// once: the backoff is a wait between generations, not a deaf window
/// (effigy#370).
fn wait_or_break(
    shutdown: &Arc<std::sync::atomic::AtomicBool>,
    delay: Duration,
    stop: impl Fn() -> bool,
) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::SeqCst) || stop() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100).min(delay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::{HostProcessRestart, HostProcessSignal};
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    fn published_listener_state(runtime_generation: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "schema": HOST_LISTENER_STATE_SCHEMA,
            "owner": "fixture|web|app",
            "runtime_generation": runtime_generation,
            "generation": "generation",
            "status": status,
            "address": null,
            "internal_url": null,
            "public_url": null,
            "route_domain": null,
            "supervisor_pid": std::process::id(),
            "child_pid": null,
            "child_boot_identity": null,
            "child_start_identity": null,
            "route_owner": null,
        })
    }

    /// The activation record is the readiness barrier: a previous activation's
    /// terminal record never satisfies the wait for this activation.
    #[test]
    fn listener_activation_barrier_waits_for_this_activation_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.listener.json");
        write_private_json(
            &path,
            &published_listener_state("previous-activation", "failed"),
        )
        .unwrap();
        assert!(
            wait_for_listener_activation(&path, "app", "current-activation", Duration::ZERO)
                .is_err(),
            "a previous activation's state must not satisfy this activation's barrier"
        );
        write_private_json(
            &path,
            &published_listener_state("current-activation", "starting"),
        )
        .unwrap();
        wait_for_listener_activation(&path, "app", "current-activation", Duration::from_secs(1))
            .expect("a published activation record satisfies the barrier");
    }

    #[test]
    fn host_process_listener_root_accepts_shared_gateway_and_checks_private_roots() {
        use std::os::unix::fs::PermissionsExt;

        let shared =
            super::super::gateway_command::standard_gateway_dir().expect("shared gateway dir");
        validate_listener_gateway_root(&shared).expect("shared gateway is not a private root");

        let loose = tempfile::tempdir().expect("fixture dir");
        let loose_root = loose.path().canonicalize().expect("canonical fixture");
        fs::set_permissions(&loose_root, fs::Permissions::from_mode(0o755))
            .expect("loosen fixture");
        assert!(
            validate_listener_gateway_root(&loose_root).is_err(),
            "a non-shared root must still pass private-root validation"
        );
    }

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize("app-1.front"), "app-1.front");
        assert_eq!(sanitize("app/../x"), "app-..-x");
    }

    /// A declared restart policy and shutdown signal reach the supervisor as
    /// the values the manifest validated, so an unknown value fails at load and
    /// is never coerced here (effigy#341).
    #[test]
    fn the_run_spec_carries_the_declared_restart_policy_and_signal() {
        let mut declared = serde_json::to_value(runtime_spec(&[])).expect("spec json");
        declared["restart"] = serde_json::json!("always");
        declared["shutdown_signal"] = serde_json::json!("SIGINT");
        let spec: HostProcessRuntimeSpec =
            serde_json::from_value(declared).expect("a declared policy and signal load");
        assert_eq!(spec.restart, HostProcessRestart::Always);
        assert_eq!(signal_for(spec.shutdown_signal), Signal::SIGINT);

        let mut unknown = serde_json::to_value(runtime_spec(&[])).expect("spec json");
        unknown["shutdown_signal"] = serde_json::json!("SIGUSR2");
        assert!(
            serde_json::from_value::<HostProcessRuntimeSpec>(unknown).is_err(),
            "an unknown signal is refused, never read as SIGTERM"
        );
    }

    #[test]
    fn host_process_dir_lives_under_runtime() {
        let dir = host_process_dir(Path::new("/repo"), "web");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web"));
    }

    #[test]
    fn managed_host_process_dir_is_profile_scoped() {
        let dir = managed_host_process_dir(Path::new("/repo"), "web", "dev");
        assert!(dir.ends_with(".effigy/runtime/host-processes/web/dev"));
    }

    /// One checkout with real Git metadata: the environment journal is bound
    /// to it, and nothing outside the fixture is read or written.
    struct Fixture {
        _dir: tempfile::TempDir,
        checkout: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("fixture dir");
        let root = dir.path().canonicalize().expect("canonical fixture");
        let checkout = root.join("checkout");
        fs::create_dir(&checkout).expect("checkout dir");
        let status = std::process::Command::new("git")
            .args(["init", "-q", "--initial-branch=main"])
            .current_dir(&checkout)
            .status()
            .expect("git");
        assert!(status.success(), "fixture git init");
        effigy_core::worktree_scope::prepare_checkout_token(&checkout)
            .expect("checkout runtime identity");
        Fixture {
            _dir: dir,
            checkout,
        }
    }

    fn policy(checkout: &Path, processes: Vec<EffectiveHostProcess>) -> EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            "web",
            "fixture-project",
            "app",
            checkout.join("compose.yml"),
        );
        policy.repo_root = checkout.to_path_buf();
        policy.profile = "effigy".to_owned();
        policy.host_processes = processes;
        policy
    }

    fn host_process(name: &str, run: &str, restart: HostProcessRestart) -> EffectiveHostProcess {
        EffectiveHostProcess {
            name: name.to_owned(),
            run: run.to_owned(),
            cwd: PathBuf::new(),
            env: Vec::new(),
            depends_on: Vec::new(),
            listener: None,
            restart,
            restart_delay_ms: 10,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
        }
    }

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn group_members(pid: u32) -> Vec<u32> {
        effigy_process::process_group_member_ids(pid).unwrap_or_default()
    }

    /// A stop names the run, the recorded PID and the escalation the kernel
    /// performed, and the process that ignored SIGTERM is gone.
    #[test]
    fn host_process_owned_stop_names_pid_and_sigkill_after_grace() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "trap '' TERM; exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("host process starts");
        let dir = host_process_dir_for_policy(&fixture.checkout, &policy);
        let log = dir.join("app.log");
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].state, "running", "{status:?}");
        let pid = status[0].pid.expect("recorded leader pid");
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        let stop = report
            .processes
            .iter()
            .find(|process| process.name == "app")
            .expect("the stop report lists the process");
        assert_eq!(stop.pid, Some(pid), "{report:?}");
        let signal = stop.signal.as_deref().unwrap_or_default();
        assert!(
            signal.contains("SIGKILL"),
            "a process that ignored SIGTERM must be named with its kill: {report:?}"
        );
        wait_until("the process group to close", || {
            group_members(pid).is_empty()
        });
        let leftovers = fs::read_dir(&dir)
            .expect("runtime directory")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".pid"))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "no PID files remain: {leftovers:?}");
        assert!(log.exists(), "the host process log keeps its location");
    }

    /// A host process whose `run` is a nested task - it starts a long-lived
    /// server through an intermediate wrapper, the shape an app build and run
    /// takes - stops within its declared grace and reports closure verified.
    /// The leader exits on SIGTERM, the wrapper keeps its server alive, and the
    /// escalation closes the group instead of timing out on it (effigy#357).
    #[test]
    fn host_process_owned_stop_of_a_nested_task_closes_within_its_grace() {
        let fixture = fixture();
        let mut nested = host_process(
            "api",
            r#"printf 'nested task starting\n'
               /bin/sh -c 'trap "" TERM; printf "server up\n"; exec /bin/sleep 120' &
               exec /bin/sh -c 'printf "supervisor up\n"; while :; do /bin/sleep 1; done'"#,
            HostProcessRestart::Never,
        );
        nested.shutdown_grace_secs = 1;
        let grace = Duration::from_secs(nested.shutdown_grace_secs);
        let policy = policy(&fixture.checkout, vec![nested]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("host process starts");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("api.log");
        wait_until("the nested task's server to start", || {
            fs::read_to_string(&log).is_ok_and(|text| text.contains("server up"))
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        let elapsed = started.elapsed();
        assert!(
            elapsed <= grace + Duration::from_secs(5),
            "down took {elapsed:?}, over grace plus five seconds"
        );
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        let stop = report
            .processes
            .iter()
            .find(|process| process.name == "api")
            .expect("the stop report lists the process");
        assert_eq!(
            stop.detail, None,
            "a stop within its grace is verified closed: {report:?}"
        );
        let signal = stop.signal.as_deref().unwrap_or_default();
        assert!(
            signal.contains("SIGKILL"),
            "the wrapper that ignored SIGTERM is named with its kill: {report:?}"
        );
        wait_until("the process group to close", || {
            group_members(pid).is_empty()
        });
    }

    /// A crash-looping process spends most of its life in the restart
    /// backoff. A stop request is honoured there, so `down` settles it within
    /// seconds instead of reporting that the live run holds its gate
    /// (effigy#370).
    #[test]
    fn host_process_owned_stop_during_restart_backoff_settles() {
        let fixture = fixture();
        let mut crashing = host_process("crash", "exit 3", HostProcessRestart::Always);
        crashing.restart_delay_ms = 60_000;
        let policy = policy(&fixture.checkout, vec![crashing]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the crash-looping host process starts");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("crash.log");
        wait_until("the first generation to exit into its backoff", || {
            fs::read_to_string(&log).is_ok_and(|text| text.contains("exited (code 3)"))
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(10),
            "the backoff is not a deaf window: {elapsed:?}\n{report:?}"
        );
        assert!(
            report.complete(),
            "failed stop must be reported: {report:?}"
        );
        assert!(
            !report.lines().join("\n").contains("holds its gate"),
            "a stop during backoff is taken, not refused: {report:?}"
        );
        assert!(group_members(pid).is_empty(), "the leader is closed");
    }

    /// A process that exits by itself is recorded with its exit and its last
    /// log lines, and status reads that from the kernel journal.
    #[test]
    fn host_process_exit_observation_records_the_exit_and_the_log() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "printf 'boom from the child\\n'; exit 7",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("host process starts");
        wait_until("the host process run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state == "closed")
        });
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status[0].last_exit, Some(7), "{status:?}");
        let log = host_process_dir_for_policy(&fixture.checkout, &policy).join("app.log");
        let text = fs::read_to_string(&log).expect("process log");
        assert!(
            text.contains("boom from the child"),
            "the process log keeps the child's last lines: {text:?}"
        );
    }

    /// Client death mid-`up` (the owning pipe closes before any detach) still
    /// stops both children through kernel custody, verified by the PIDs the
    /// custody receipts recorded.
    #[test]
    fn host_process_owned_client_death_mid_up_stops_both_recorded_pids() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![
                host_process("a", "exec /bin/sleep 120", HostProcessRestart::Never),
                host_process("b", "exec /bin/sleep 120", HostProcessRestart::Never),
            ],
        );
        let clients = super::kernel_run::start_host_processes_owning(&fixture.checkout, &policy)
            .expect("both host processes start with an owning client");
        assert_eq!(clients.len(), 2, "both supervisors have an owning client");
        wait_until("both custody receipts to name a PID", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .all(|process| process.pid.is_some())
        });
        let pids = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .iter()
            .filter_map(|process| process.pid)
            .collect::<Vec<_>>();
        assert_eq!(pids.len(), 2, "both custody receipts name a PID");
        // Dropping the owning clients before any detach is client death mid-up:
        // kernel custody must settle both children.
        drop(clients);
        wait_until("both child groups to close after client death", || {
            pids.iter().all(|pid| group_members(*pid).is_empty())
        });
        wait_until("both kernel runs to settle after client death", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .all(|process| process.state == "closed")
        });
    }

    /// A loopback listener child that serves the reserved address and answers
    /// every request with 200, so a managed listener reaches its declared
    /// readiness without a container backend or any adapter convention.
    const LISTENER_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

    /// A fixture listener port outside the OS ephemeral range, seeded by the
    /// test's own checkout so concurrent tests probe different ports.
    fn fixture_listener_bind(seed: &str) -> String {
        let start = seed.bytes().fold(0u32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(u32::from(byte))
        });
        for offset in 0..1024 {
            let port = 20_000 + (start.wrapping_add(offset) % 4_000);
            let port = u16::try_from(port).expect("fixture listener port");
            if std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok() {
                return format!("127.0.0.1:{port}");
            }
        }
        panic!("no free fixture listener port outside the ephemeral range");
    }

    /// One declared host process with a managed listener route on `domain`,
    /// running `child` (Python source) with `timeout_secs` as its declared
    /// readiness budget.
    fn listener_host_process_running(
        name: &str,
        domain: &str,
        bind: &str,
        child: &str,
        timeout_secs: u64,
    ) -> EffectiveHostProcess {
        let mut process = host_process(
            name,
            &format!("exec python3 -c '{}'", child.trim()),
            HostProcessRestart::Never,
        );
        process.listener = Some(EffectiveManagedHostListener {
            bind: bind.to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: timeout_secs,
            declared_route_domain: domain.to_owned(),
            route_domain: domain.to_owned(),
            route_tls: false,
        });
        process
    }

    /// A listener policy whose route domain and listener port belong to one
    /// test, with its gateway state outside the checkout so the machine store
    /// never shares the checkout journal's directory.
    fn listener_policy(
        checkout: &Path,
        domain: &str,
        name: &str,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        listener_policy_running(checkout, domain, name, LISTENER_CHILD, 30)
    }

    /// [`listener_policy`] with the listener's own child and declared readiness
    /// budget, for the proofs about a child that has not opened its port yet.
    fn listener_policy_running(
        checkout: &Path,
        domain: &str,
        name: &str,
        child: &str,
        timeout_secs: u64,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        let gateway_home = checkout
            .parent()
            .expect("fixture root")
            .join(format!("gateway-{name}"));
        // The supervisor is a separate process, so it cannot see this thread's
        // test gateway home and re-checks the recorded root as a private root.
        // Private roots require mode 0700, exactly as a real private gateway
        // would.
        let gateway_dir = gateway_home.join(".effigy/gateway");
        fs::create_dir_all(&gateway_dir).expect("gateway state directory");
        use std::os::unix::fs::PermissionsExt;
        for directory in [gateway_home.join(".effigy"), gateway_dir] {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("private gateway permissions");
        }
        let guard = super::super::gateway_command::set_test_gateway_home(&gateway_home);
        let bind = fixture_listener_bind(&format!("{}|{domain}|{name}", checkout.display()));
        let mut policy = policy(
            checkout,
            vec![listener_host_process_running(
                "app",
                domain,
                &bind,
                child,
                timeout_secs,
            )],
        );
        policy.project_name = format!("reclaim-{name}");
        (guard, policy)
    }

    /// The listener state record one listener policy's supervisor writes.
    fn listener_state(policy: &EffectiveContainerPolicy) -> PathBuf {
        super::host_process_state_path(
            &host_process_dir_for_policy(policy.repo_root.as_path(), policy),
            "app",
        )
    }

    fn read_json(path: &Path) -> serde_json::Value {
        serde_json::from_slice(
            &fs::read(path)
                .unwrap_or_else(|error| panic!("listener state {}: {error}", path.display())),
        )
        .unwrap_or_else(|error| panic!("listener state {} is not JSON: {error}", path.display()))
    }

    fn route_table_path() -> PathBuf {
        super::super::gateway_command::standard_gateway_dir()
            .expect("test gateway dir")
            .join("routes.json")
    }

    fn published_owner(domain: &str) -> ManagedListenerRouteOwner {
        RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .and_then(|route| route.managed_listener.clone())
            .expect("published managed listener route")
    }

    /// Kill the recorded run abruptly: the supervisor first, so it cannot
    /// withdraw its route, then its child group. Only PIDs this test started.
    fn kill_recorded_run(state_path: &Path, leader: u32) {
        let state = read_json(state_path);
        let supervisor = state["supervisor_pid"]
            .as_u64()
            .expect("the listener state names its supervisor") as u32;
        kill(Pid::from_raw(supervisor as i32), Signal::SIGKILL)
            .expect("kill the recorded supervisor");
        kill(Pid::from_raw(-(leader as i32)), Signal::SIGKILL)
            .expect("kill the recorded child group");
        wait_until("the killed listener group to close", || {
            group_members(leader).is_empty()
        });
    }

    /// A child that answers every request with the wrong status opens its port
    /// and never becomes ready.
    const REFUSING_LISTENER_CHILD: &str = r#"
import http.server, os
bind = os.environ["EFFIGY_MANAGED_HOST_LISTENER_BIND"]
host, port = bind.rsplit(":", 1)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(503)
        self.send_header("Content-Length", "0")
        self.end_headers()
    def log_message(self, *args):
        pass
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;

    /// A cold build is phase (a): the port is closed and the child is alive.
    /// That phase has no deadline, so an app that opens its port long after its
    /// declared readiness budget still becomes ready, and the `up` that waits
    /// for it does not substitute a deadline of its own (Acow #756).
    #[test]
    fn host_process_listener_waits_for_a_port_that_opens_after_its_readiness_budget() {
        let fixture = fixture();
        let domain = "slow-build.host.test";
        let slow_build = format!("import time\ntime.sleep(8)\n{}", LISTENER_CHILD.trim());
        let (_gateway_home, policy) =
            listener_policy_running(&fixture.checkout, domain, "slow-build", &slow_build, 1);
        let started = Instant::now();
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("a child that opens its port after its readiness budget still becomes ready");
        assert!(
            started.elapsed() >= Duration::from_secs(8),
            "the fixture child must compile for longer than its declared budget: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "ready", "{state}");
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert_eq!(read_json(&listener_state(&policy))["status"], "stopped");
    }

    /// The declared budget starts when the port first accepts a connection: a
    /// listener that answers with the wrong status fails after it, naming the
    /// budget and the probe outcome.
    #[test]
    fn host_process_listener_readiness_budget_starts_when_the_port_opens() {
        let fixture = fixture();
        let domain = "refusing.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "refusing",
            REFUSING_LISTENER_CHILD,
            1,
        );
        let started = Instant::now();
        let error =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
                .expect_err(
                    "a listener that never answers the declared status fails before readiness",
                )
                .to_string();
        let elapsed = started.elapsed();
        assert!(error.contains("timed out after 1s"), "{error}");
        assert!(
            elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(20),
            "the budget starts when the port opens, so the start fails just after it: {elapsed:?}"
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("status_mismatch"),
            "{state}"
        );
    }

    /// The child exiting is the failure signal of phase (a): a child that dies
    /// before it ever binds fails the start at once, naming its exit code, and
    /// the declared readiness budget never comes into it.
    #[test]
    fn host_process_listener_child_exit_before_binding_names_the_exit_code() {
        let fixture = fixture();
        let domain = "died-before-binding.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "died-before-binding",
            "import sys\nsys.exit(3)",
            60,
        );
        let started = Instant::now();
        let error =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
                .expect_err("a listener child that exits before binding fails the start")
                .to_string();
        assert!(error.contains("exited with code 3"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the child's exit, not the readiness budget, ends the wait: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("exited with code 3"),
            "{state}"
        );
    }

    /// A child that leaves the port to a successor is reported as daemonised,
    /// not as a start that answered: Effigy supervises the group it started, so
    /// a listener that exits its leader fails the start with that reason
    /// (effigy#370).
    #[test]
    fn host_process_owned_a_daemonising_child_is_reported_as_such() {
        let fixture = fixture();
        let domain = "daemonised.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "daemonised",
            "import sys\nsys.exit(0)",
            60,
        );
        let started = Instant::now();
        let error =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
                .expect_err("a child that exits its leader before answering fails the start")
                .to_string();
        assert!(error.contains("daemonised"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the daemonising child is reported, not waited on: {:?}",
            started.elapsed()
        );
        let state = read_json(&listener_state(&policy));
        assert_eq!(state["status"], "failed", "{state}");
        assert!(
            state["diagnostic"]
                .as_str()
                .unwrap_or_default()
                .contains("daemonised"),
            "{state}"
        );
    }

    /// A stop requested while a child is still in phase (a) ends the wait and
    /// settles the run: the phase has no deadline, the stop does. The `up`
    /// console carries a progress line for the wait, and the stop that ends it
    /// is the one `down` and `container restart` run (effigy#370).
    #[test]
    fn host_process_owned_stop_during_a_starting_child_settles() {
        let progress = listener_start_progress("api", Duration::from_secs(30));
        assert!(
            progress.contains("still starting after 30s") && progress.contains("down` stops it"),
            "the waiting console names the wait and what ends it: {progress}"
        );
        let fixture = fixture();
        let domain = "stopped-while-starting.host.test";
        let (_gateway_home, policy) = listener_policy_running(
            &fixture.checkout,
            domain,
            "stopped-while-starting",
            "import time\ntime.sleep(120)",
            60,
        );
        let state_path = listener_state(&policy);
        let checkout = fixture.checkout.clone();
        let up_policy = policy.clone();
        let up = std::thread::spawn(move || {
            super::kernel_run::start_host_processes_for_container(&checkout, &up_policy)
        });
        wait_until("the listener to publish its starting generation", || {
            fs::read(&state_path)
                .ok()
                .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
                .is_some_and(|state| state["status"] == "starting")
        });
        let started = Instant::now();
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "a hung child does not delay the stop: {:?}",
            started.elapsed()
        );
        assert!(report.complete(), "{report:?}");
        let stopped = up
            .join()
            .expect("the up waiting on a starting child returns");
        let error = stopped
            .expect_err("an up whose listener was stopped before readiness does not report a start")
            .to_string();
        assert!(
            !error.contains("timed out") && !error.contains("holds its gate"),
            "the waiting up reports the stop, not a deadline: {error}"
        );
        assert_eq!(read_json(&state_path)["status"], "stopped");
    }

    /// A restart replaces named generations in place, so a listener keeps the
    /// address it published and every dependent's resolved `internal_url`
    /// still reaches it (effigy#367, effigy#370).
    #[test]
    fn host_process_owned_restart_keeps_a_listeners_published_address() {
        let fixture = fixture();
        let domain = "reclaim-restart.host.test";
        let (_gateway_home, mut policy) =
            listener_policy(&fixture.checkout, domain, "reclaim-restart");
        let mut consumer = host_process(
            "consumer",
            "printf '%s' \"$API_URL\" > consumer-url.txt",
            HostProcessRestart::Never,
        );
        consumer.depends_on = vec!["app".to_owned()];
        consumer.env = vec![(
            "API_URL".to_owned(),
            "${host_process.app.internal_url}".to_owned(),
        )];
        policy.host_processes.push(consumer);
        let state_path = listener_state(&policy);
        let url_path = fixture.checkout.join("consumer-url.txt");

        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the listener and its dependent start");
        let address = read_json(&state_path)["address"]
            .as_str()
            .expect("published address")
            .to_owned();
        wait_until("the dependent to record its resolved endpoint", || {
            fs::read_to_string(&url_path).is_ok_and(|text| text == format!("http://{address}"))
        });

        fs::remove_file(&url_path).expect("clear the dependent's record");
        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the named listener restarts");
        let restarted = report
            .processes
            .iter()
            .find(|process| process.name == "app")
            .expect("app is in the restart report");
        assert_eq!(
            restarted
                .listener
                .as_ref()
                .map(|listener| listener.address.as_str()),
            Some(address.as_str()),
            "a restart reuses the address the listener published: {report:?}"
        );
        assert_eq!(
            read_json(&state_path)["address"],
            address.as_str(),
            "the state record keeps the published address"
        );
        wait_until(
            "the restarted dependent to record the same endpoint",
            || fs::read_to_string(&url_path).is_ok_and(|text| text == format!("http://{address}")),
        );

        let stop = super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stop.complete(), "{stop:?}");
    }

    /// A restart of an environment that declares only host processes works:
    /// the replace step holds the environment for its change, and an
    /// environment with no container rows is not a refusal (effigy#370).
    #[test]
    fn host_process_owned_restart_works_for_a_host_process_only_environment() {
        use effigy_containers::environment::Operation;

        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        let started = Instant::now();
        let report = super::super::environment::holding_environment(&policy, Operation::Up, || {
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)?;
            super::kernel_run::restart_host_processes_for_container(
                &fixture.checkout,
                &policy,
                &["app".to_owned()],
            )
            .map(|report| report.processes)
        })
        .expect("a host-process-only environment restarts");
        assert_eq!(
            report
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["app"]
        );
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the restart does not wait out a gate: {:?}",
            started.elapsed()
        );
        assert_eq!(
            super::kernel_run::host_process_status(&fixture.checkout, &policy)[0].state,
            "running"
        );
        let stop = super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(stop.complete(), "{stop:?}");
    }

    /// A listener route's owner is its checkout path, environment and process
    /// name, so a supervisor restart publishes under the same owner.
    #[test]
    fn managed_listener_owner_is_stable_across_supervisor_restarts() {
        let fixture = fixture();
        let (_gateway_home, policy) =
            listener_policy(&fixture.checkout, "owner.host.test", "owner");
        let process = policy
            .host_processes
            .iter()
            .find(|process| process.listener.is_some())
            .expect("listener declaration");
        let project = fixture.checkout.display().to_string();
        let first =
            super::managed_host_process_owner(&project, &policy, process).expect("owner identity");
        let second =
            super::managed_host_process_owner(&project, &policy, process).expect("owner identity");
        assert_eq!(first, second);
        assert_eq!(first, format!("{project}|{}|{}", policy.name, process.name));
    }

    /// `down` withdraws a listener route whose supervisor died without
    /// withdrawing it, and reports the withdrawal.
    #[test]
    fn host_process_listener_down_withdraws_a_dead_supervisors_route() {
        let fixture = fixture();
        let domain = "reclaim-down.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "down");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts and publishes its route");
        assert!(published_owner(domain).address.starts_with("127.0.0.1:"));
        let leader = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .first()
            .and_then(|status| status.pid)
            .expect("custody receipt names the child group");
        kill_recorded_run(&listener_state(&policy), leader);

        // Nothing withdrew the route: it outlived the supervisor that published
        // it, which is exactly the residue `down` has to clear.
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "a killed supervisor leaves its route behind"
        );
        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "down withdraws the dead supervisor's route: {report:?}"
        );
        assert!(
            report
                .routes
                .iter()
                .any(|line| line.contains(domain) && line.contains("withdrawn")),
            "down reports the withdrawal: {report:?}"
        );
    }

    /// `down` withdraws a live listener's route as well: the environment's own
    /// stop owns every route its declarations published, whichever side of the
    /// stop closes the supervisor.
    #[test]
    fn host_process_listener_down_withdraws_a_live_supervisors_route() {
        let fixture = fixture();
        let domain = "reclaim-live-down.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "live-down");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts and publishes its route");
        let owner = published_owner(domain);
        assert_eq!(
            owner.generation,
            published_owner(domain).generation,
            "the route records its publisher generation"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_none(),
            "down withdraws a live listener's route: {report:?}"
        );
    }

    /// A dead listener run's route is replaced by the next start of the same
    /// checkout, without a `down` and without manual cleanup.
    #[test]
    fn host_process_listener_successor_replaces_a_dead_runs_route() {
        let fixture = fixture();
        let domain = "reclaim-successor.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "successor");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts and publishes its route");
        let first = published_owner(domain);
        let first_generation = first.generation.clone();
        let leader = super::kernel_run::host_process_status(&fixture.checkout, &policy)
            .first()
            .and_then(|status| status.pid)
            .expect("custody receipt names the child group");
        kill_recorded_run(&listener_state(&policy), leader);
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "the route survives the run that published it"
        );

        // No `down`: the same checkout path replaces its own route on the next
        // start, whatever the previous generation did.
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("a successor run replaces the route its predecessor left");
        let successor = published_owner(domain);
        assert_ne!(
            successor.generation, first_generation,
            "the route records the successor generation"
        );
        assert_eq!(
            read_json(&listener_state(&policy))["generation"],
            serde_json::Value::String(successor.generation.clone())
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .is_none());
    }

    /// A live sibling environment of the same checkout takes the domain over:
    /// the checkout path is the whole owner, so the last writer wins between a
    /// container environment and its listener.
    #[test]
    fn host_process_listener_replaces_its_own_checkouts_live_listener() {
        let fixture = fixture();
        let domain = "reclaim-live.host.test";
        let (_gateway_home, policy) = listener_policy(&fixture.checkout, domain, "live");
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts and publishes its route");
        let live = published_owner(domain);
        let route = RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .cloned()
            .expect("published route");
        assert_eq!(
            route.project,
            fixture.checkout.display().to_string(),
            "the route's owner is the checkout path"
        );
        effigy_gateway::registration::register_route(
            &route_table_path(),
            &effigy_gateway::registration::RouteRegistration {
                domain: domain.to_owned(),
                target: Some("127.0.0.1:9".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                project_path: fixture.checkout.display().to_string(),
                source: effigy_gateway::routes::RouteSource::Container,
            },
        )
        .expect("the same checkout path replaces its own listener route");
        let replaced = RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .cloned()
            .expect("route stays");
        assert!(replaced.managed_listener.is_none());
        assert_ne!(replaced.target.as_deref(), Some(live.address.as_str()));

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
        assert!(
            RouteTable::load(&route_table_path())
                .expect("route table")
                .lookup(domain)
                .is_some(),
            "the container environment's route is not the host-process stop's to remove"
        );
        // `down` from the container side removes it by path.
        assert!(effigy_gateway::registration::deregister_route(
            &route_table_path(),
            domain,
            &fixture.checkout.display().to_string(),
        )
        .unwrap());
        assert!(RouteTable::load(&route_table_path())
            .expect("route table")
            .lookup(domain)
            .is_none());
    }

    /// A child receives its `env` references resolved against the environment's
    /// published facts, and a typo'd reference refuses to start the process
    /// naming the key (effigy#321).
    #[test]
    fn host_process_env_references_reach_the_child_and_a_typo_refuses() {
        let fixture = fixture();
        fs::write(
            fixture.checkout.join("compose.yml"),
            "services:\n  postgres:\n    image: postgres\n    ports:\n      - \"127.0.0.1:8141:5432\"\n",
        )
        .expect("compose file");
        let out = fixture.checkout.join("resolved-env.txt");
        let mut resolved = host_process(
            "app",
            "printf '%s|%s\\n' \"$DATABASE_URL\" \"$COOKIE_DOMAIN\" > \"$HOST_PROCESS_ENV_OUT\"",
            HostProcessRestart::Never,
        );
        resolved.env = vec![
            (
                "DATABASE_URL".to_owned(),
                "postgres://app@${service.postgres.host}:${service.postgres.port}/app".to_owned(),
            ),
            ("COOKIE_DOMAIN".to_owned(), ".${apex}".to_owned()),
            ("HOST_PROCESS_ENV_OUT".to_owned(), out.display().to_string()),
        ];
        let mut configured = policy(&fixture.checkout, vec![resolved]);
        configured.dns_routes = vec![effigy_containers::EffectiveDnsRoute {
            domain: "api.acme.test".to_owned(),
            declared_domain: "api.acme.test".to_owned(),
            tls: true,
            port: Some(3000),
            service: Some("app".to_owned()),
            target_host: None,
        }];
        configured.dns_base_domain = Some("acme.test".to_owned());
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &configured)
            .expect("the host process starts with its resolved environment");
        wait_until("the child to write its resolved environment", || {
            out.exists()
        });
        let written = fs::read_to_string(&out).expect("resolved environment file");
        assert_eq!(
            written.trim(),
            "postgres://app@127.0.0.1:8141/app|.acme.test"
        );

        let mut typo = host_process("typo", "/bin/true", HostProcessRestart::Never);
        typo.env = vec![(
            "DATABASE_URL".to_owned(),
            "${service.postgres.prot}".to_owned(),
        )];
        let typo_policy = policy(&fixture.checkout, vec![typo]);
        let error =
            super::kernel_run::start_host_processes_for_container(&fixture.checkout, &typo_policy)
                .expect_err("a typo'd reference refuses to start");
        let message = error.to_string();
        assert!(message.contains("DATABASE_URL"), "{message}");
        assert!(message.contains("service.postgres.prot"), "{message}");
    }

    fn runtime_spec(env: &[(&str, &str)]) -> HostProcessRuntimeSpec {
        HostProcessRuntimeSpec {
            schema: HOST_PROCESS_SPEC_SCHEMA.to_owned(),
            project_path: "/checkout".to_owned(),
            runtime_generation: "runtime".to_owned(),
            cwd: PathBuf::from("/checkout"),
            env: env
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            listener: None,
            owner: "owner".to_owned(),
            dependencies: Vec::new(),
            listener_state_file: PathBuf::from("/checkout/app.listener.json"),
            gateway_root: None,
            environment: "web".to_owned(),
            profile: "effigy".to_owned(),
            process_name: "api".to_owned(),
            container_name: "web".to_owned(),
            run: "/bin/true".to_owned(),
            log_file: PathBuf::from("/checkout/app.log"),
            restart: HostProcessRestart::Never,
            restart_delay_ms: 10,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
            checkout: None,
            state_home: PathBuf::from("/home"),
        }
    }

    fn plain_env(
        environment: BTreeMap<OsString, effigy_kernel::custody::EnvValue>,
        key: &str,
    ) -> Option<String> {
        match environment.get(&OsString::from(key))? {
            effigy_kernel::custody::EnvValue::Plain(value) => {
                Some(value.to_string_lossy().into_owned())
            }
            _ => None,
        }
    }

    /// A generation that never answers names the reserved address, the path it
    /// was asked for and the status it was asked to return.
    #[test]
    fn host_process_listener_timeout_names_the_reserved_address() {
        let config = EffectiveManagedHostListener {
            bind: "127.0.0.1:9400".to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: 1,
            declared_route_domain: "timeout.host.test".to_owned(),
            route_domain: "timeout.host.test".to_owned(),
            route_tls: false,
        };
        let message = listener_readiness_timeout(
            "127.0.0.1:9400"
                .parse::<SocketAddr>()
                .expect("reserved address"),
            &config,
            Duration::from_secs(1),
            HttpReadinessOutcome::ConnectFailed,
        )
        .to_string();
        assert!(message.contains("127.0.0.1:9400"), "{message}");
        assert!(message.contains("/health"), "{message}");
        assert!(message.contains("200"), "{message}");
        assert!(message.contains("connect_failed"), "{message}");
    }

    /// The published route is withdrawn only after three consecutive probe
    /// misses, and the next answer republishes it.
    #[test]
    fn listener_liveness_withdraws_after_three_misses_and_republishes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fixture listener");
        let address = listener.local_addr().expect("fixture address");
        let mut liveness = ListenerLiveness::new(address);
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answering port changes nothing");
        drop(listener);
        for miss in 1..LISTENER_PROBE_MISSES {
            liveness.next_probe = Instant::now();
            assert_eq!(liveness.poll(), None, "miss {miss} only counts");
        }
        liveness.next_probe = Instant::now();
        assert_eq!(
            liveness.poll(),
            Some(false),
            "the third consecutive miss withdraws the route"
        );
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a withdrawn route stays withdrawn");
        // The port answers again: the route is republished exactly once.
        let listener = std::net::TcpListener::bind(address).expect("rebound fixture listener");
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(true), "a later answer republishes");
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answer with no change is quiet");
        drop(listener);
    }

    /// A failed republication is not a published route: while the port keeps
    /// answering the liveness probe retries the publication instead of waiting
    /// for a silence that may never come.
    #[test]
    fn listener_liveness_retries_a_failed_publication_while_the_port_answers() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fixture listener");
        let address = listener.local_addr().expect("fixture address");
        let mut liveness = ListenerLiveness::new(address);
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "an answering port changes nothing");

        // The supervisor's publication failed: the route is not published.
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(
            liveness.poll(),
            Some(true),
            "the next answer retries a publication that failed"
        );
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a published route is quiet");

        // A second failure retries again, not once.
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(true));
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None);

        // A silent port is still withdrawn after three misses, and a failed
        // publication on a withdrawn route asks for no republish.
        drop(listener);
        for miss in 1..LISTENER_PROBE_MISSES {
            liveness.next_probe = Instant::now();
            assert_eq!(liveness.poll(), None, "miss {miss} only counts");
        }
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), Some(false));
        liveness.publication_failed();
        liveness.next_probe = Instant::now();
        assert_eq!(liveness.poll(), None, "a silent port is retried by nothing");
    }

    /// A dependent is replaced only when its dependency's supervisor replaced
    /// its child generation; a probe miss on the dependency is not a change.
    #[test]
    fn dependency_transition_only_follows_a_changed_generation() {
        let dir = tempfile::tempdir().expect("fixture dir");
        let state_file = dir.path().join("app.listener.json");
        let spec = HostProcessRuntimeSpec {
            dependencies: vec![HostProcessDependency {
                name: "app".to_owned(),
                owner: "fixture|web|app".to_owned(),
                state_file: state_file.clone(),
            }],
            ..runtime_spec(&[])
        };
        let write = |generation: &str, status: &str| {
            write_private_json(
                &state_file,
                &serde_json::json!({
                    "schema": HOST_LISTENER_STATE_SCHEMA,
                    "owner": "fixture|web|app",
                    "runtime_generation": "runtime",
                    "generation": generation,
                    "status": status,
                    "address": null,
                    "internal_url": null,
                    "public_url": null,
                    "route_domain": null,
                    "supervisor_pid": std::process::id(),
                    "child_pid": null,
                    "child_boot_identity": null,
                    "child_start_identity": null,
                    "route_owner": null,
                }),
            )
            .expect("dependency state");
        };
        write("child-1", "ready");
        let expected = dependency_generations(&spec).expect("dependency generations");
        assert!(!dependency_transition(&spec, &expected).expect("no transition"));
        // The dependency's liveness probe misses and its route is withdrawn:
        // the child generation is unchanged, so the dependent keeps running.
        write("child-1", "unavailable");
        assert!(
            !dependency_transition(&spec, &expected).expect("no transition"),
            "a probe miss is not a generation change"
        );
        // An unreadable record is never a change either.
        fs::write(&state_file, b"{not json").expect("corrupt dependency state");
        assert!(!dependency_transition(&spec, &expected).expect("no transition"));
        // A replaced child generation is.
        write("child-2", "ready");
        assert!(dependency_transition(&spec, &expected).expect("transition"));
    }

    /// The reserved listener port reaches the child as `HOST`/`PORT` even when
    /// the supervisor's own environment exports a value for them; only a value
    /// the manifest declares outranks the reserved address (effigy#321). A
    /// shipped adapter's report path and generation token are supplied too
    /// (effigy#349), and Effigy never reads the report back.
    #[test]
    fn reserved_listener_host_and_port_outrank_an_ambient_value() {
        let bind = "127.0.0.1:8311";
        let report = Path::new("/checkout/api.gen.listener-report.json");
        let adapter = |bind: &'static str| Some((bind, report, "gen"));
        let ambient = vec![
            (OsString::from("HOST"), OsString::from("0.0.0.0")),
            (OsString::from("PORT"), OsString::from("9999")),
        ];
        let spec = runtime_spec(&[]);
        let environment = child_environment(ambient.clone().into_iter(), &spec, &[], adapter(bind));
        assert_eq!(
            plain_env(environment.clone(), "HOST").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            plain_env(environment.clone(), "PORT").as_deref(),
            Some("8311")
        );
        // The adapter contract is still absolute: the reserved address, the
        // report path and this generation's token.
        assert_eq!(
            plain_env(environment.clone(), HOST_LISTENER_BIND_ENV).as_deref(),
            Some(bind)
        );
        assert_eq!(
            plain_env(environment.clone(), HOST_LISTENER_REPORT_ENV).as_deref(),
            Some("/checkout/api.gen.listener-report.json")
        );
        assert_eq!(
            plain_env(environment, HOST_LISTENER_GENERATION_ENV).as_deref(),
            Some("gen")
        );

        // A declared value is the process's own decision and wins.
        let declared = runtime_spec(&[("HOST", "0.0.0.0"), ("PORT", "3000")]);
        let environment = child_environment(ambient.into_iter(), &declared, &[], adapter(bind));
        assert_eq!(
            plain_env(environment.clone(), "HOST").as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(plain_env(environment, "PORT").as_deref(), Some("3000"));

        // A process without a listener keeps the ambient environment untouched.
        let environment = child_environment(
            vec![
                (OsString::from("HOST"), OsString::from("0.0.0.0")),
                (OsString::from("PORT"), OsString::from("9999")),
            ]
            .into_iter(),
            &runtime_spec(&[]),
            &[],
            None,
        );
        assert_eq!(
            plain_env(environment.clone(), "HOST").as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(plain_env(environment, "PORT").as_deref(), Some("9999"));
    }

    /// A leftover v0.14.1 PID record, and the spec beside it, are never read:
    /// no Effigy surface opens them, so a record from a supervisor that died
    /// with the reboot cannot block `up` (effigy#341).
    #[test]
    fn host_process_environment_carries_no_internal_marker() {
        let _markers = crate::contract_test_support::EnvGuard::set_many(&[
            ("EFFIGY_INTERNAL_SUPPRESS_HEADER", Some("1".to_owned())),
            ("EFFIGY_INTERNAL_ENVIRONMENT_HOLDER", Some("run".to_owned())),
            ("EFFIGY_MANAGED_RUN_TOKEN", Some("token".to_owned())),
            ("EFFIGY_SECRETS_REQUIRED", Some("1".to_owned())),
            ("EFFIGY_INTERNAL_CONTAINER_HANDOFF", Some("1".to_owned())),
        ]);
        let environment = child_environment(
            crate::runner::secret_session::workload_ambient_environment(),
            &runtime_spec(&[]),
            &[],
            None,
        );
        let keys = environment
            .keys()
            .map(|key| key.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for key in [
            "EFFIGY_INTERNAL_SUPPRESS_HEADER",
            "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER",
            "EFFIGY_MANAGED_RUN_TOKEN",
            "EFFIGY_SECRETS_REQUIRED",
        ] {
            assert!(
                !keys.iter().any(|present| present == key),
                "`{key}` reached a host process environment: {keys:?}"
            );
        }
        assert!(
            keys.iter()
                .any(|present| present == "EFFIGY_INTERNAL_CONTAINER_HANDOFF"),
            "a consumer reads the container handoff marker: {keys:?}"
        );
    }

    /// A crash-looping child backs off: the declared delay, then doubled, to a
    /// 30 s cap, and never a restart limit (effigy#341).
    #[test]
    fn host_process_restart_backoff_doubles_to_its_cap() {
        let mut backoff = Duration::ZERO;
        let delays: Vec<u64> = (0..8)
            .map(|_| restart_backoff(&mut backoff, 1_000).as_millis() as u64)
            .collect();
        assert_eq!(
            delays,
            vec![1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]
        );
    }

    /// `on-failure` replaces a leader that exited non-zero or took a signal
    /// (`128 + signal`), `always` also replaces a clean exit, and `never`
    /// settles whatever the code (effigy#336).
    #[test]
    fn host_process_restart_policy_decides_a_signalled_leader() {
        assert!(HostProcessRestart::OnFailure.restarts_after(143));
        assert!(HostProcessRestart::OnFailure.restarts_after(1));
        assert!(!HostProcessRestart::OnFailure.restarts_after(0));
        assert!(HostProcessRestart::Always.restarts_after(0));
        assert!(!HostProcessRestart::Never.restarts_after(143));
        assert!(!HostProcessRestart::Never.restarts_after(0));
    }

    /// An unreaped zombie is not a group member: the process table still lists
    /// its PID, but it has exited and holds nothing the group reserves.
    #[test]
    fn host_process_group_observation_ignores_an_unreaped_zombie() {
        use std::os::unix::process::CommandExt;

        let mut zombie = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .expect("leader in its own group");
        let pid = zombie.id();
        kill(Pid::from_raw(pid as i32), Signal::SIGKILL).expect("stop the fixture leader");
        wait_until("the fixture leader to become an unreaped zombie", || {
            std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .ok()
                .is_some_and(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .starts_with('Z')
                })
        });
        assert!(
            group_members(pid).is_empty(),
            "a zombie holds nothing its group reserves"
        );
        assert!(effigy_process::surviving_group_member_ids(pid)
            .expect("group observation")
            .is_empty());
        zombie.wait().expect("reap the fixture zombie");
    }

    /// After a leader exits, the exited leader is never a surviving member of
    /// its own group, while a live grandchild still is (effigy#336).
    #[test]
    fn host_process_leader_exit_observation_excludes_the_exited_leader() {
        use std::os::unix::process::CommandExt;

        let dir = tempfile::tempdir().expect("fixture dir");
        let grandchild_file = dir.path().join("grandchild.pid");
        let mut leader = std::process::Command::new("/bin/sh")
            // The leader stays live (`wait`) so the observation sees both a
            // running leader and a real grandchild before it exits.
            .args([
                "-c",
                "sleep 300 & printf '%s\\n' \"$!\" > \"$1\"; wait",
                "sh",
            ])
            .arg(&grandchild_file)
            .process_group(0)
            .spawn()
            .expect("leader in its own group");
        let pid = leader.id();
        wait_until("the grandchild to record its pid", || {
            grandchild_file.exists()
        });
        let grandchild = fs::read_to_string(&grandchild_file)
            .expect("grandchild pid file")
            .trim()
            .parse::<u32>()
            .expect("grandchild pid");
        assert!(
            group_members(pid).contains(&pid),
            "the running leader is a member of its own group"
        );
        assert_eq!(
            effigy_process::surviving_group_member_ids(pid).expect("group observation"),
            vec![grandchild],
            "a live grandchild is the only surviving member"
        );
        kill(Pid::from_raw(pid as i32), Signal::SIGKILL).expect("stop the fixture leader");
        // The leader passes through reapable zombie to reaped; neither state may
        // make it a surviving member, and the grandchild keeps blocking reuse.
        for _ in 0..40 {
            assert_eq!(
                effigy_process::surviving_group_member_ids(pid).expect("group observation"),
                vec![grandchild],
                "the exited leader must never be a surviving member of its own group"
            );
            thread::sleep(Duration::from_millis(25));
        }
        leader.wait().expect("reap the fixture leader");
        assert_eq!(
            effigy_process::surviving_group_member_ids(pid).expect("group observation"),
            vec![grandchild],
            "the live grandchild keeps the generation unavailable"
        );
        kill(Pid::from_raw(grandchild as i32), Signal::SIGKILL).expect("stop the grandchild");
        wait_until("the fixture group to close", || {
            group_members(pid).is_empty()
        });
    }

    /// A leader terminated by a signal closes a `never` host process with its
    /// exit recorded (`128 + SIGTERM`) and starts no successor (effigy#336).
    #[test]
    fn host_process_exit_observation_never_policy_records_a_terminated_leader() {
        let fixture = fixture();
        let policy = policy(
            &fixture.checkout,
            vec![host_process(
                "app",
                "exec /bin/sleep 120",
                HostProcessRestart::Never,
            )],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("host process starts");
        wait_until("the host process leader to be recorded", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state == "running" && status.pid.is_some())
        });
        let pid = super::kernel_run::host_process_status(&fixture.checkout, &policy)[0]
            .pid
            .expect("recorded leader pid");
        kill(Pid::from_raw(pid as i32), Signal::SIGTERM).expect("terminate the leader");
        wait_until("the host process run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .first()
                .is_some_and(|status| status.state == "closed")
        });
        let status = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(status[0].last_exit, Some(143), "{status:?}");
        assert!(
            !effigy_process::process_is_running(pid),
            "`never` starts no successor generation"
        );
    }

    // ---- `container <NAME> restart <process>...` --------------------------
    //
    // A restart is `up`'s start path narrowed to a set of names: the named
    // processes and the processes that depend on them are replaced, and every
    // other host process and container keeps running (effigy#367).

    /// A test gateway home and a policy over `processes`, so these proofs can
    /// declare a managed listener beside the processes they restart.
    fn restart_policy(
        checkout: &Path,
        name: &str,
        processes: Vec<EffectiveHostProcess>,
    ) -> (
        super::super::gateway_command::TestGatewayHomeGuard,
        EffectiveContainerPolicy,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let gateway_home = checkout
            .parent()
            .expect("fixture root")
            .join(format!("gateway-{name}"));
        let gateway_dir = gateway_home.join(".effigy/gateway");
        fs::create_dir_all(&gateway_dir).expect("gateway state directory");
        for directory in [gateway_home.join(".effigy"), gateway_dir] {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("private gateway permissions");
        }
        let guard = super::super::gateway_command::set_test_gateway_home(&gateway_home);
        let mut policy = policy(checkout, processes);
        policy.project_name = format!("restart-{name}");
        (guard, policy)
    }

    /// One declared process's run and leader pid, as `container status` reads
    /// them.
    fn running_process(
        status: &[super::kernel_run::HostProcessStatus],
        name: &str,
    ) -> (String, u32) {
        let row = status
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("host process `{name}` is declared"));
        assert_eq!(row.state, "running", "{name}: {row:?}");
        (
            row.run.clone().expect("a running process has a run"),
            row.pid.expect("a running process has a recorded leader"),
        )
    }

    /// One readiness request to a reserved listener address.
    fn http_get(address: &str, domain: &str) -> String {
        use std::io::{Read as _, Write as _};
        let address: SocketAddr = address.parse().expect("a loopback listener address");
        let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .expect("the restarted listener answers its reserved address");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("listener read timeout");
        write!(
            stream,
            "GET /health HTTP/1.1\r\nHost: {domain}\r\nConnection: close\r\n\r\n"
        )
        .expect("write the readiness request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the readiness response");
        response
    }

    fn named_worker() -> EffectiveHostProcess {
        host_process("worker", "exec /bin/sleep 120", HostProcessRestart::Never)
    }

    /// Two host processes are up: `restart worker` replaces only the worker.
    /// The listener's run, pid and route stay exactly as they were.
    #[test]
    fn restart_replaces_only_the_named_host_process() {
        let fixture = fixture();
        let domain = "restart-named.host.test";
        let bind = fixture_listener_bind("restart-named|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "named", vec![app, named_worker()]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (worker_run, worker_pid) = running_process(&before, "worker");
        let route_before = published_owner(domain);

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["worker".to_owned()],
        )
        .expect("restart names a declared process");
        assert_eq!(
            report
                .processes
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["worker"],
            "only the named process is replaced"
        );
        assert!(!report.processes[0].dependent);
        assert!(
            report.processes[0].listener.is_none(),
            "a non-listener restarts without a route"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run_after, app_pid_after) = running_process(&after, "app");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_eq!(
            (app_run_after.clone(), app_pid_after),
            (app_run, app_pid),
            "a process that does not depend on the worker is never touched"
        );
        assert_ne!(
            (worker_run_after.clone(), worker_pid_after),
            (worker_run, worker_pid),
            "the restarted process is a new generation with a new leader"
        );
        assert_eq!(
            report.processes[0].run.as_deref(),
            Some(worker_run_after.as_str()),
            "the report names the new run"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "an untouched listener keeps its published route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A process that exited - by `effigy run stop <run>` or by crashing - is
    /// simply started again, and the listener beside it keeps running.
    #[test]
    fn restart_starts_a_stopped_process_and_leaves_the_listener_running() {
        let fixture = fixture();
        let domain = "restart-stopped.host.test";
        let bind = fixture_listener_bind("restart-stopped|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "stopped", vec![app, named_worker()]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (_worker_run, worker_pid) = running_process(&before, "worker");

        // The worker's generation ends: its group is gone and its run settles
        // closed, exactly as `effigy run stop <run>` leaves it.
        kill(Pid::from_raw(worker_pid as i32), Signal::SIGKILL).expect("end the worker");
        wait_until("the worker run to close", || {
            super::kernel_run::host_process_status(&fixture.checkout, &policy)
                .iter()
                .find(|row| row.name == "worker")
                .is_some_and(|row| row.state == "closed")
        });

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["worker".to_owned()],
        )
        .expect("a stopped process is restarted by name");
        assert_eq!(report.processes.len(), 1);
        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run_after, app_pid_after) = running_process(&after, "app");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_eq!(
            (app_run_after, app_pid_after),
            (app_run, app_pid),
            "the listener is untouched"
        );
        assert_ne!(worker_pid_after, worker_pid);
        assert_eq!(report.processes[0].run.as_deref(), Some(&*worker_run_after));

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// Restarting a listener keeps its declared address and republishes its
    /// route for the new generation, which answers on that address.
    #[test]
    fn restart_of_a_listener_keeps_its_address_and_republishes_its_route() {
        let fixture = fixture();
        let domain = "restart-listener.host.test";
        let bind = fixture_listener_bind("restart-listener|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) = restart_policy(&fixture.checkout, "listener", vec![app]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts and publishes its route");
        let before = read_json(&listener_state(&policy));
        assert_eq!(before["status"], "ready", "{before}");
        let address = before["address"]
            .as_str()
            .expect("the listener state records its address")
            .to_owned();
        let route_before = published_owner(domain);
        assert_eq!(
            serde_json::Value::String(route_before.generation.clone()),
            before["generation"],
            "the route serves the first generation"
        );

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the listener restarts");
        assert_eq!(report.processes.len(), 1);
        let listener = report.processes[0]
            .listener
            .as_ref()
            .expect("a restarted listener reports its listener");
        assert_eq!(listener.status, "ready");
        assert_eq!(
            listener.address, address,
            "a restart keeps the declared listener address"
        );

        let after = read_json(&listener_state(&policy));
        assert_eq!(after["status"], "ready", "{after}");
        assert_eq!(after["address"], serde_json::Value::String(address.clone()));
        assert_ne!(
            after["generation"], before["generation"],
            "the restarted listener is a new generation"
        );
        assert_ne!(
            after["child_pid"], before["child_pid"],
            "the new generation is a new child"
        );
        let route_after = published_owner(domain);
        assert_eq!(
            serde_json::Value::String(route_after.generation.clone()),
            after["generation"],
            "the route serves the restarted generation"
        );
        assert_eq!(route_after.address, address);
        let response = http_get(&address, domain);
        assert!(
            response.contains("200 OK"),
            "the restarted generation answers its reserved address: {response}"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A process that depends on a restarted process is restarted after it and
    /// named as a dependent; a process that does not depend on it stays up.
    #[test]
    fn restart_restarts_a_dependent_after_its_dependency_and_names_it() {
        let fixture = fixture();
        let domain = "restart-dependent.host.test";
        let bind = fixture_listener_bind("restart-dependent|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let mut consumer = named_worker();
        consumer.name = "consumer".to_owned();
        consumer.depends_on = vec!["app".to_owned()];
        let worker = named_worker();
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "dependent", vec![app, consumer, worker]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the listener, its dependent and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (_app_run, app_pid) = running_process(&before, "app");
        let (consumer_run, consumer_pid) = running_process(&before, "consumer");
        let (worker_run, worker_pid) = running_process(&before, "worker");

        let report = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["app".to_owned()],
        )
        .expect("the listener restarts with its dependent");
        assert_eq!(
            report
                .processes
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["app", "consumer"],
            "the dependent is restarted after its dependency"
        );
        assert!(!report.processes[0].dependent);
        assert!(
            report.processes[1].dependent,
            "a process restarted because it depends on a named one is named as a dependent"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (_app_run_after, app_pid_after) = running_process(&after, "app");
        let (consumer_run_after, consumer_pid_after) = running_process(&after, "consumer");
        let (worker_run_after, worker_pid_after) = running_process(&after, "worker");
        assert_ne!(app_pid_after, app_pid);
        assert_ne!(
            (consumer_run_after, consumer_pid_after),
            (consumer_run, consumer_pid),
            "the dependent is a new generation"
        );
        assert_eq!(
            (worker_run_after, worker_pid_after),
            (worker_run, worker_pid),
            "a process that does not depend on the restarted one is untouched"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A detach that fails while the restarted run is still live settles only
    /// the process the restart selected. Every other host process of the
    /// environment keeps its run, its leader and its route.
    #[test]
    fn detach_failure_rollback_stops_only_the_restarted_process() {
        let fixture = fixture();
        let domain = "detach-failure.host.test";
        let bind = fixture_listener_bind("detach-failure|front");
        let front = listener_host_process_running("front", domain, &bind, LISTENER_CHILD, 30);
        let api = host_process("api", "exec /bin/sleep 120", HostProcessRestart::Never);
        let (_guard, policy) =
            restart_policy(&fixture.checkout, "detach-failure", vec![front, api]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (front_run, front_pid) = running_process(&before, "front");
        let route_before = published_owner(domain);

        // The restart selected `api`; its detach failed while the run was
        // live, so the rollback settles exactly that selection.
        let selection = super::kernel_run::restart_selection(&policy, &["api".to_owned()])
            .expect("the restart selects the named process")
            .ordered
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            selection,
            std::collections::BTreeSet::from(["api".to_owned()]),
            "a restart of one process does not select the other"
        );
        let report = super::kernel_run::rollback_after_detach_failure(
            &fixture.checkout,
            &policy,
            Some(&selection),
        );
        assert!(report.complete(), "{report:?}");
        assert!(
            report.processes.iter().any(|process| process.name == "api"),
            "the selected process is settled: {report:?}"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert!(
            !after
                .iter()
                .any(|row| row.name == "api" && row.state == "running"),
            "the selected process is stopped: {after:?}"
        );
        assert_eq!(
            running_process(&after, "front"),
            (front_run, front_pid),
            "a detach-failure rollback leaves an unrelated host process alone"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "an unrelated listener keeps its route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// A restart that cannot reach readiness settles only what it replaced:
    /// the processes that were already running keep their run, their leader
    /// and their route.
    #[test]
    fn restart_failure_settles_only_what_it_replaced() {
        let fixture = fixture();
        let domain = "restart-failure.host.test";
        let bind = fixture_listener_bind("restart-failure|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, up_policy) = restart_policy(
            &fixture.checkout,
            "restart-failure",
            vec![app, named_worker()],
        );
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &up_policy)
            .expect("the listener and the worker start");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &up_policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let (worker_run, worker_pid) = running_process(&before, "worker");
        let route_before = published_owner(domain);

        // The same environment, now also declaring a listener that answers
        // every readiness request with 503.
        let mut broken = listener_host_process_running(
            "broken",
            "restart-failure-broken.host.test",
            &fixture_listener_bind("restart-failure|broken"),
            REFUSING_LISTENER_CHILD,
            1,
        );
        broken.name = "broken".to_owned();
        let mut policy = up_policy;
        policy.host_processes.push(broken);
        let error = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["broken".to_owned()],
        )
        .expect_err("a listener that never becomes ready fails the restart");
        assert!(
            error.to_string().contains("readiness"),
            "the readiness error is reported: {error}"
        );

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(
            running_process(&after, "app"),
            (app_run, app_pid),
            "a failed restart leaves the running listener alone"
        );
        assert_eq!(
            running_process(&after, "worker"),
            (worker_run, worker_pid),
            "a failed restart leaves every other host process alone"
        );
        assert_eq!(
            published_owner(domain).generation,
            route_before.generation,
            "a failed restart keeps the untouched listener's route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }

    /// An unknown process name fails before any effect, naming the processes
    /// the environment declares.
    #[test]
    fn restart_refuses_an_unknown_process_and_changes_nothing() {
        let fixture = fixture();
        let domain = "restart-unknown.host.test";
        let bind = fixture_listener_bind("restart-unknown|app");
        let app = listener_host_process_running("app", domain, &bind, LISTENER_CHILD, 30);
        let (_guard, policy) = restart_policy(&fixture.checkout, "unknown", vec![app]);
        super::kernel_run::start_host_processes_for_container(&fixture.checkout, &policy)
            .expect("the managed listener starts");
        let before = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        let (app_run, app_pid) = running_process(&before, "app");
        let state_before = read_json(&listener_state(&policy));

        let error = super::kernel_run::restart_host_processes_for_container(
            &fixture.checkout,
            &policy,
            &["nope".to_owned()],
        )
        .expect_err("an unknown process name is refused");
        let message = error.to_string();
        assert!(
            message.contains("declares no host process `nope`"),
            "{message}"
        );
        assert!(message.contains("`app`"), "{message}");

        let after = super::kernel_run::host_process_status(&fixture.checkout, &policy);
        assert_eq!(
            running_process(&after, "app"),
            (app_run, app_pid),
            "a refused restart changes nothing"
        );
        assert_eq!(
            read_json(&listener_state(&policy))["generation"],
            state_before["generation"],
            "a refused restart republishes no route"
        );

        let report =
            super::kernel_run::stop_host_processes_for_container(&fixture.checkout, &policy);
        assert!(report.complete(), "{report:?}");
    }
}
