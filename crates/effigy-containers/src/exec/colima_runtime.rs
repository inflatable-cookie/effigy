use nix::errno::Errno;
use nix::fcntl::{Flock, FlockArg};
use serde_json::Value as JsonValue;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
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
        classify_colima_observation, colima_start_command, colima_status_command,
        colima_stop_command, managed_colima_profile_drift, managed_colima_profile_resources,
        parse_colima_running, prepare_managed_colima_profile, ColimaObservation,
    },
    compose::{resolve_compose_backend_for_repo, resolve_host_cli_program, ComposeBackend},
    user_global_colima_profile, BackendId, ContainerBackendDetection, ContainerManager,
    ContainerManagerError, EffectiveContainerPolicy, DEFAULT_COLIMA_PROFILE,
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

/// Whether Colima recovery guidance targets a person at a terminal or an automated caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColimaAdviceMode {
    Interactive,
    Automated,
}

/// Detects whether recovery guidance may include operator-run profile commands.
pub fn colima_advice_mode() -> ColimaAdviceMode {
    use std::io::IsTerminal as _;

    advice_mode_for_caller(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::env::args_os().any(|arg| arg == "--json"),
        std::env::var_os("CI").is_some(),
        effigy_ui::steps::in_managed_pane(),
    )
}

fn advice_mode_for_caller(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
    json_output: bool,
    in_ci: bool,
    in_managed_pane: bool,
) -> ColimaAdviceMode {
    if stdin_is_terminal && stdout_is_terminal && !json_output && !in_ci && !in_managed_pane {
        ColimaAdviceMode::Interactive
    } else {
        ColimaAdviceMode::Automated
    }
}

/// Renders a guarded force-stop hint for refusals that require manual cleanup.
pub fn colima_profile_force_stop_advice(profile: &str, mode: ColimaAdviceMode) -> String {
    match mode {
        ColimaAdviceMode::Interactive => format!(
            "Verify profile pidfile ownership before manually running `colima stop -p {profile} --force`; this stops every container on the shared profile. Effigy did not run it."
        ),
        ColimaAdviceMode::Automated => format!(
            "Colima profile `{profile}` is shared across checkouts. Automated runs must not stop or restart it; report the degraded profile to the host operator. Effigy did not run it."
        ),
    }
}

/// Renders recovery guidance without suggesting profile-wide actions to automation.
pub fn colima_profile_recovery_advice(profile: &str, mode: ColimaAdviceMode) -> String {
    match mode {
        ColimaAdviceMode::Interactive => format!(
            "Fix: `colima stop --profile {profile} && colima start --profile {profile}`. This stops every container on that profile."
        ),
        ColimaAdviceMode::Automated => format!(
            "Colima profile `{profile}` is shared across checkouts. Automated runs must not stop or restart it; report the degraded profile to the host operator."
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaRecoveryReport {
    pub profile: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColimaProfileEntry {
    name: String,
    status: String,
    runtime: String,
    cpus: u64,
    memory: u64,
    disk: u64,
}

/// Runtime and VM state for a named Colima profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaProfileInfo {
    /// Colima's profile status, compared case-insensitively with `running`.
    pub status: String,
    /// Explicit Colima runtime, or `None` for older/mocked output.
    pub runtime: Option<String>,
}

impl ColimaProfileInfo {
    /// Whether this profile can be used through Colima's nerdctl interface.
    pub fn supports_nerdctl(&self) -> bool {
        self.runtime
            .as_deref()
            .map(runtime_supports_nerdctl)
            .unwrap_or(true)
    }

    /// Whether Colima reports the profile's VM as running.
    pub fn is_running(&self) -> bool {
        self.status.eq_ignore_ascii_case("running")
    }
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

/// Held shared advisory lock guarding a Colima profile against concurrent idle
/// `mountInotify` stop/restart while a container bring-up or Compose activation is
/// in flight.
pub struct ColimaProfileBringupGuard {
    _lock: Option<Flock<File>>,
}

impl std::fmt::Debug for ColimaProfileBringupGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColimaProfileBringupGuard")
            .field("locked", &self._lock.is_some())
            .finish()
    }
}

impl ColimaProfileBringupGuard {
    fn empty() -> Self {
        Self { _lock: None }
    }
}

fn colima_profile_lifecycle_lock_path(profile: &str) -> Option<PathBuf> {
    let home = crate::runtime::dns::colima_home_dir()?;
    let is_plain_safe = !profile.is_empty()
        && profile != "."
        && profile != ".."
        && !profile.starts_with('.')
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    let filename = if is_plain_safe {
        format!("{profile}.lock")
    } else {
        let sanitized: String = profile
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let stem = if sanitized.is_empty() {
            "profile".to_owned()
        } else {
            sanitized
        };
        let mut hash: u64 = 0xcbf29ce484222325;
        for &byte in profile.as_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{stem}-{hash:016x}.lock")
    };
    Some(home.join(".effigy-profile-locks").join(filename))
}

fn open_profile_lifecycle_lock_file(profile: &str) -> Result<Option<File>, ContainerExecError> {
    let Some(path) = colima_profile_lifecycle_lock_path(profile) else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ContainerExecError::Failure {
            command: "Colima profile lock".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: format!(
                "cannot create Colima profile lock directory `{}`: {error}",
                parent.display()
            ),
        })?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| ContainerExecError::Failure {
            command: "Colima profile lock".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: format!(
                "cannot open Colima profile lock `{}`: {error}",
                path.display()
            ),
        })?;
    Ok(Some(file))
}

fn acquire_shared_profile_lock(profile: &str) -> Result<Option<Flock<File>>, ContainerExecError> {
    let Some(mut file) = open_profile_lifecycle_lock_file(profile)? else {
        return Ok(None);
    };
    loop {
        match Flock::lock(file, FlockArg::LockShared) {
            Ok(lock) => return Ok(Some(lock)),
            Err((returned, Errno::EINTR)) => file = returned,
            Err((_, errno)) => {
                return Err(ContainerExecError::Failure {
                    command: "Colima profile lock".to_owned(),
                    code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "cannot acquire shared lock for Colima profile `{profile}`: {errno}"
                    ),
                });
            }
        }
    }
}

fn try_acquire_exclusive_profile_lock(
    profile: &str,
) -> Result<Option<Flock<File>>, ContainerExecError> {
    let Some(mut file) = open_profile_lifecycle_lock_file(profile)? else {
        return Ok(None);
    };
    loop {
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => return Ok(Some(lock)),
            Err((returned, Errno::EINTR)) => file = returned,
            Err((_, errno)) if errno == Errno::EAGAIN || errno == Errno::EWOULDBLOCK => {
                return Ok(None);
            }
            Err((_, errno)) => {
                return Err(ContainerExecError::Failure {
                    command: "Colima profile lock".to_owned(),
                    code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "cannot acquire exclusive lock for Colima profile `{profile}`: {errno}"
                    ),
                });
            }
        }
    }
}

fn downgrade_exclusive_lock_to_shared(
    profile: &str,
    lock: Flock<File>,
) -> Result<Flock<File>, ContainerExecError> {
    loop {
        match lock.relock(FlockArg::LockShared) {
            Ok(()) => return Ok(lock),
            Err(Errno::EINTR) => continue,
            Err(errno) => {
                return Err(ContainerExecError::Failure {
                    command: "Colima profile lock".to_owned(),
                    code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "cannot downgrade lock for Colima profile `{profile}`: {errno}"
                    ),
                });
            }
        }
    }
}

/// Acquire a shared Colima profile lock across container bring-up / Compose
/// activation so a concurrent interactive invocation cannot stop the VM between
/// `ps -q` and `colima stop`.
pub fn acquire_colima_profile_bringup_guard(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaProfileBringupGuard, ContainerExecError> {
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return Ok(ColimaProfileBringupGuard::empty());
    }
    Ok(ColimaProfileBringupGuard {
        _lock: acquire_shared_profile_lock(&policy.profile)?,
    })
}

pub fn ensure_colima_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    Ok(ensure_colima_running_report(policy, repo_root)?.started)
}

/// The one entry point for a managed Colima profile: create it when it does not
/// exist and start it when stopped. A running profile is never stopped or
/// restarted here unless interactive idle `mountInotify` migration applies.
pub fn ensure_colima_running_report(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    ensure_colima_running_report_with_advice_mode(policy, repo_root, colima_advice_mode())
}

fn ensure_colima_running_report_with_advice_mode(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    let (report, _guard) =
        ensure_colima_running_report_for_bringup_with_advice_mode(policy, repo_root, advice_mode)?;
    Ok(report)
}

fn ensure_colima_running_report_for_bringup_with_advice_mode(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
) -> Result<(ColimaEnsureReport, ColimaProfileBringupGuard), ContainerExecError> {
    let (action, guard) = ensure_colima_profile_running(policy, repo_root, advice_mode)?;
    let report = ColimaEnsureReport {
        started: action != ColimaProfileAction::Reused,
        profile_created_or_restarted: action.recreates_profile(),
    };
    maintain_health_check_pid_files(policy, repo_root);
    Ok((report, guard))
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

enum EnsureProfileLock {
    Exclusive {
        lock: Flock<File>,
    },
    Shared {
        lock: Option<Flock<File>>,
        contended: bool,
    },
}

fn acquire_profile_lock_for_ensure(
    policy: &EffectiveContainerPolicy,
    advice_mode: ColimaAdviceMode,
) -> Result<EnsureProfileLock, ContainerExecError> {
    let wants_migration = advice_mode == ColimaAdviceMode::Interactive
        && crate::colima::is_managed_colima_profile(&policy.profile)
        && crate::colima_profile_mount_inotify(&policy.profile) == Some(true);
    if wants_migration {
        if let Some(lock) = try_acquire_exclusive_profile_lock(&policy.profile)? {
            return Ok(EnsureProfileLock::Exclusive { lock });
        }
        let lock = acquire_shared_profile_lock(&policy.profile)?;
        return Ok(EnsureProfileLock::Shared {
            lock,
            contended: true,
        });
    }
    let lock = acquire_shared_profile_lock(&policy.profile)?;
    Ok(EnsureProfileLock::Shared {
        lock,
        contended: false,
    })
}

fn ensure_colima_profile_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
) -> Result<(ColimaProfileAction, ColimaProfileBringupGuard), ContainerExecError> {
    match acquire_profile_lock_for_ensure(policy, advice_mode)? {
        EnsureProfileLock::Exclusive { lock } => {
            let action = if colima_is_running_for_start(policy, repo_root, advice_mode)? {
                ensure_running_managed_profile_ready_inner(
                    policy,
                    repo_root,
                    advice_mode,
                    true,
                    false,
                )?
            } else if colima_profile_exists(policy, repo_root)? {
                start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Started)?
            } else {
                start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Created)?
            };
            let shared = downgrade_exclusive_lock_to_shared(&policy.profile, lock)?;
            Ok((
                action,
                ColimaProfileBringupGuard {
                    _lock: Some(shared),
                },
            ))
        }
        EnsureProfileLock::Shared { lock, contended } => {
            let action = if colima_is_running_for_start(policy, repo_root, advice_mode)? {
                ensure_running_managed_profile_ready_inner(
                    policy,
                    repo_root,
                    advice_mode,
                    false,
                    contended,
                )?
            } else if colima_profile_exists(policy, repo_root)? {
                start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Started)?
            } else {
                start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Created)?
            };
            Ok((action, ColimaProfileBringupGuard { _lock: lock }))
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MountInotifyMigration {
    /// `mountInotify` is already false/missing or the profile is unmanaged.
    NotNeeded,
    /// Profile was cleanly stopped and restarted with `mountInotify: false`.
    Migrated,
    /// `mountInotify: true` is live, and migration was deferred (automated mode,
    /// running containers, failed `ps -q`, or concurrent bring-up lock).
    Deferred,
}

/// A running profile is only reusable when it carries Effigy's managed
/// settings. Observed drift warns once and names the deliberate command: the
/// profile is shared with every other checkout's stacks, so Effigy never stops
/// or restarts it (except idle interactive `mountInotify: true` migration), and
/// the user's profile is never deleted.
#[cfg_attr(not(test), allow(dead_code))]
fn ensure_running_managed_profile_ready(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
) -> Result<ColimaProfileAction, ContainerExecError> {
    let wants_migration = advice_mode == ColimaAdviceMode::Interactive
        && crate::colima::is_managed_colima_profile(&policy.profile)
        && crate::colima_profile_mount_inotify(&policy.profile) == Some(true);
    if wants_migration {
        if let Some(_lock) = try_acquire_exclusive_profile_lock(&policy.profile)? {
            return ensure_running_managed_profile_ready_inner(
                policy,
                repo_root,
                advice_mode,
                true,
                false,
            );
        }
        return ensure_running_managed_profile_ready_inner(
            policy,
            repo_root,
            advice_mode,
            false,
            true,
        );
    }
    ensure_running_managed_profile_ready_inner(policy, repo_root, advice_mode, false, false)
}

fn ensure_running_managed_profile_ready_inner(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
    exclusive_held: bool,
    contended: bool,
) -> Result<ColimaProfileAction, ContainerExecError> {
    let migration = maybe_migrate_running_profile_mount_inotify(
        policy,
        repo_root,
        advice_mode,
        exclusive_held,
        contended,
    )?;
    if migration == MountInotifyMigration::Migrated {
        return Ok(ColimaProfileAction::Started);
    }
    if migration == MountInotifyMigration::NotNeeded {
        if let Some(drift) = managed_colima_profile_drift(&policy.profile) {
            // Write the managed settings so the operator's own restart applies them.
            let _ = prepare_managed_colima_profile(policy);
            eprintln!("[warn] {}", drift.warning(&policy.profile));
        }
    } else if let Some(drift) = managed_colima_profile_drift(&policy.profile) {
        // Migration is deferred while global `mountInotify: true` is live: never
        // call `prepare_managed_colima_profile`, which would rewrite `colima.yaml`
        // to `mountInotify: false` without restarting the VM and silence `doctor`.
        match drift {
            crate::colima::ManagedColimaProfileDrift::SshAgentForwardingDisabled => eprintln!(
                "[warn] Colima profile `{}` is running without SSH agent forwarding while global mountInotify: true migration is deferred; leaving colima.yaml untouched. Restart it deliberately with `colima restart {}`.",
                policy.profile, policy.profile
            ),
            crate::colima::ManagedColimaProfileDrift::MemoryBelowPlan {
                actual_gib,
                planned_gib,
            } => eprintln!(
                "[warn] Colima profile `{}` is running with {actual_gib}GiB RAM (requires {planned_gib}GiB) while global mountInotify: true migration is deferred; leaving colima.yaml untouched. Restart it deliberately with `colima restart {}`.",
                policy.profile, policy.profile
            ),
        }
    }
    if let Some(resources) = managed_colima_profile_resources(&policy.profile) {
        if let Some(entry) = colima_profile_entry(repo_root, &policy.profile) {
            let planned_bytes = resources.memory_gib.saturating_mul(1024 * 1024 * 1024);
            if entry.memory < planned_bytes {
                let actual_gib = (entry.memory / (1024 * 1024 * 1024)).max(1);
                let config_note = if migration == MountInotifyMigration::Deferred {
                    "Effigy left colima.yaml untouched while global mountInotify: true is active and did not restart the VM."
                } else {
                    "Effigy wrote the managed settings and did not restart the VM."
                };
                return Err(ContainerExecError::Failure {
                    command: "Colima profile resources".to_owned(),
                    code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "Colima profile `{}` is running with {actual_gib}GiB RAM (requires {planned_gib}GiB)\n{config_note}\nnext: restart it deliberately with `colima restart {}`",
                        policy.profile,
                        policy.profile,
                        planned_gib = resources.memory_gib
                    ),
                });
            }
        }
    }
    ensure_running_profile_runtime_ready(
        policy.profile.as_str(),
        COLIMA_RUNNING_PROFILE_GRACE,
        advice_mode,
        |timeout| wait_for_colima_runtime_ready(policy, repo_root, timeout),
        || start_colima_runtime_units(policy, repo_root),
    )?;
    Ok(ColimaProfileAction::Reused)
}

/// Migrate an already-running managed Colima profile away from global
/// `mountInotify: true` only when running interactively, holding the exclusive
/// profile lifecycle lock, and zero containers are active on that profile.
/// Automated callers, concurrent bring-ups, and profiles with running containers
/// (or unreadable container lists) defer migration and leave `colima.yaml` and
/// the VM untouched.
fn maybe_migrate_running_profile_mount_inotify(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
    exclusive_held: bool,
    contended: bool,
) -> Result<MountInotifyMigration, ContainerExecError> {
    if !crate::colima::is_managed_colima_profile(&policy.profile)
        || crate::colima_profile_mount_inotify(&policy.profile) != Some(true)
    {
        return Ok(MountInotifyMigration::NotNeeded);
    }
    if advice_mode == ColimaAdviceMode::Automated {
        return Ok(MountInotifyMigration::Deferred);
    }
    if contended || !exclusive_held {
        eprintln!(
            "[info] Colima profile `{}` is still running with global mountInotify: true; deferring migration while another container bring-up is active",
            policy.profile
        );
        return Ok(MountInotifyMigration::Deferred);
    }
    if colima_profile_has_running_containers(policy, repo_root) {
        eprintln!(
            "[info] Colima profile `{}` is still running with global mountInotify: true; Effigy will switch it off automatically next time it starts with no running containers (or on the next planned restart)",
            policy.profile
        );
        return Ok(MountInotifyMigration::Deferred);
    }
    eprintln!(
        "[info] upgrading Colima profile `{}` to disable global mountInotify (no running containers)…",
        policy.profile
    );
    let stop = colima_stop_command(policy);
    let stop_args: Vec<&str> = stop.args.iter().map(String::as_str).collect();
    run_command_capture_with_timeout(
        repo_root,
        &stop.program,
        &stop_args,
        &stop.label,
        COLIMA_STOP_TIMEOUT,
    )?;
    start_managed_colima_profile(policy, repo_root, ColimaProfileAction::Started)?;
    Ok(MountInotifyMigration::Migrated)
}

/// Returns `true` when at least one container is running on `policy.profile`,
/// or when listing fails (fail-safe so active stacks are never interrupted).
fn colima_profile_has_running_containers(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> bool {
    let args = colima_nerdctl_args(policy, &["ps", "-q"]);
    let rendered = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let borrowed = rendered.iter().map(String::as_str).collect::<Vec<_>>();
    match run_command_capture_with_timeout(
        repo_root,
        "colima",
        &borrowed,
        "Colima running container check",
        COLIMA_RUNTIME_PROBE_TIMEOUT,
    ) {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| !line.trim().is_empty()),
        Err(_) => true,
    }
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
    advice_mode: ColimaAdviceMode,
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
                        advice_mode,
                    )),
                },
                Err(start_error) => Err(runtime_unit_start_refusal(
                    profile,
                    &readiness_error,
                    Some(&start_error),
                    "not retried: the unit start itself failed",
                    advice_mode,
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
    advice_mode: ColimaAdviceMode,
) -> ContainerExecError {
    let start = start_error
        .map(|error| format!("\nunit start: {error}"))
        .unwrap_or_default();
    ContainerExecError::Failure {
        command: "Colima runtime readiness".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "Colima profile `{profile}` is running but containerd/BuildKit are not ready\nStarting those units inside the VM did not recover them within {}s; Effigy did not stop or restart the profile because that stops every stack on it.\nfirst readiness probe: {readiness_error}{start}\nruntime probe after unit start: {probe_error}\nnext: retry, {}",
            COLIMA_UNIT_START_TIMEOUT.as_secs(),
            match advice_mode {
                ColimaAdviceMode::Interactive => {
                    "recover this profile in place with `effigy container profile repair`, or stop it deliberately with `effigy container profile reset-runtime`".to_owned()
                }
                ColimaAdviceMode::Automated => {
                    colima_profile_recovery_advice(profile, ColimaAdviceMode::Automated)
                }
            },
        ),
    }
}

/// Liveness of the resolved runtime backend. Only the backend actually
/// selected is probed: a Docker backend runs `docker ps` and never asks Colima
/// anything, because a Native Linux or Docker Desktop host has no `colima`
/// binary to answer (effigy#1087).
pub fn runtime_backend_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    runtime_backend_is_running_with_deadline(policy, repo_root, None)
}

/// Deadline-aware liveness of the resolved runtime backend. A bounded caller's
/// monotonic deadline supervises the spawn on either backend, so an expired
/// deadline never spawns and a slow probe cannot extend the budget. The Colima
/// branch keeps its read-only status probe (effigy#371); the Docker branch is
/// the plain `docker ps` the runtime exposes, and its non-zero exit is the
/// engine's own "not running" answer rather than a launcher failure.
pub fn runtime_backend_is_running_with_deadline(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    deadline: Option<Instant>,
) -> Result<bool, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure_with_deadline(
                repo_root,
                policy,
                &[OsString::from("ps")],
                "docker ps",
                deadline,
            )?;
            Ok(output.status.success())
        }
        ComposeBackend::ColimaNerdctl => {
            colima_is_running_with_deadline(policy, repo_root, deadline)
        }
    }
}

/// Status-path observation of the selected backend.
///
/// Lifecycle helpers still treat a failed Colima status as "not running".
/// `container status` JSON must not flatten that into a success payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeObservation {
    Running,
    Stopped,
    Unavailable { detail: String },
}

pub fn observe_runtime_backend(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<RuntimeObservation, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            if output.status.success() {
                Ok(RuntimeObservation::Running)
            } else {
                Ok(RuntimeObservation::Stopped)
            }
        }
        ComposeBackend::ColimaNerdctl => observe_colima_backend(policy, repo_root),
    }
}

fn observe_colima_backend(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<RuntimeObservation, ContainerExecError> {
    let cmd = colima_status_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    let output = run_command_capture_allow_failure_with_deadline(
        repo_root,
        &cmd.program,
        &args,
        &cmd.label,
        None,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(
        match classify_colima_observation(output.status.success(), &stdout, &stderr) {
            ColimaObservation::Running => RuntimeObservation::Running,
            ColimaObservation::Stopped => RuntimeObservation::Stopped,
            ColimaObservation::Unavailable => RuntimeObservation::Unavailable {
                detail: unavailable_colima_detail(&stdout, &stderr),
            },
        },
    )
}

fn unavailable_colima_detail(stdout: &str, stderr: &str) -> String {
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_owned();
    }
    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_owned();
    }
    "colima status produced no usable observation".to_owned()
}

pub fn ensure_runtime_backend_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    Ok(ensure_runtime_backend_running_report(policy, repo_root)?.started)
}

/// [`ensure_runtime_backend_running`] returning a shared bring-up guard that
/// stays held across container admission and `compose up`, preventing an idle
/// `mountInotify` migration from stopping the VM mid-bring-up.
pub fn ensure_runtime_backend_running_for_bringup(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(bool, ColimaProfileBringupGuard), ContainerExecError> {
    let (report, guard) = ensure_runtime_backend_running_report_for_bringup(policy, repo_root)?;
    Ok((report.started, guard))
}

/// [`ensure_runtime_backend_running`] with the profile transition reported, so
/// a caller holding a container admission can tell that the profile it was
/// recorded against was created or restarted.
pub fn ensure_runtime_backend_running_report(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    Ok(ensure_runtime_backend_running_report_for_bringup(policy, repo_root)?.0)
}

/// [`ensure_runtime_backend_running_report`] returning a shared bring-up guard
/// that stays held across container admission and `compose up`.
pub fn ensure_runtime_backend_running_report_for_bringup(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(ColimaEnsureReport, ColimaProfileBringupGuard), ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            if output.status.success() {
                Ok((
                    ColimaEnsureReport::default(),
                    ColimaProfileBringupGuard::empty(),
                ))
            } else {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let raw_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let stderr = if crate::exec::EngineAbsence::observed_in(&stdout, &raw_stderr)
                    == Some(crate::exec::EngineAbsence::DockerDaemon)
                {
                    let mut lines = vec!["Docker daemon is not running".to_owned()];
                    let trimmed = raw_stderr.trim();
                    if !trimmed.is_empty() {
                        lines.push(format!("stderr: {trimmed}"));
                    }
                    lines.push(
                        "next: start Docker Desktop or the Docker daemon, then retry".to_owned(),
                    );
                    lines.join("\n")
                } else {
                    raw_stderr
                };
                Err(ContainerExecError::Failure {
                    command: "docker ps".to_owned(),
                    code: output.status.code(),
                    stdout,
                    stderr,
                })
            }
        }
        ComposeBackend::ColimaNerdctl => ensure_colima_running_report_for_bringup_with_advice_mode(
            policy,
            repo_root,
            colima_advice_mode(),
        ),
    }
}

/// The no-change form of [`ensure_runtime_backend_running_report`]: `Ok` only
/// when the backend is already usable as configured, so the ordinary ensure
/// would reuse it without any write. Every other state (not running, managed
/// settings drifted, runtime units needing a start) refuses before a start,
/// create, restart, settings write or unit start can run. Docker's ensure is
/// already a read-only `ps`, so it is reused as-is.
pub fn verify_runtime_backend_no_change(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => ensure_runtime_backend_running_report(policy, repo_root),
        ComposeBackend::ColimaNerdctl => verify_colima_profile_no_change(policy, repo_root),
    }
}

fn verify_colima_profile_no_change(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaEnsureReport, ContainerExecError> {
    let refusal = |reason: &str, next: String| {
        ContainerExecError::Failure {
        command: "Colima profile no-change check".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "Colima profile `{}` {reason}\nno-change mode refuses to start, create, restart, rewrite or unit-start the shared profile\nnext: {next}",
            policy.profile
        ),
    }
    };
    if !colima_is_running(policy, repo_root)? {
        return Err(refusal(
            "is not running",
            format!(
                "start it with `colima start --profile {}` or run without no-change mode",
                policy.profile
            ),
        ));
    }
    if managed_colima_profile_drift(&policy.profile).is_some() {
        return Err(refusal(
            "has managed settings that differ from Effigy's",
            format!(
                "restart it deliberately with `colima restart {}` or run without no-change mode",
                policy.profile
            ),
        ));
    }
    ensure_running_profile_runtime_ready(
        policy.profile.as_str(),
        COLIMA_RUNNING_PROFILE_GRACE,
        colima_advice_mode(),
        |timeout| wait_for_colima_runtime_ready(policy, repo_root, timeout),
        || Err("containerd/BuildKit units are not running and would need a unit start".to_owned()),
    )?;
    Ok(ColimaEnsureReport::default())
}

fn colima_is_running_for_start(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    advice_mode: ColimaAdviceMode,
) -> Result<bool, ContainerExecError> {
    let output = colima_status_output(policy, repo_root, None)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        && docker_failure_looks_like_colima_runtime_state_loss(&stdout, &stderr)
        && colima_profile_entry(repo_root, &policy.profile)
            .is_some_and(|entry| entry.status.eq_ignore_ascii_case("running"))
    {
        return Err(ContainerExecError::Failure {
            command: "colima status".to_owned(),
            code: output.status.code(),
            stdout: stdout.into_owned(),
            stderr: format!(
                "Colima profile `{}` is running with an empty runtime record. {}",
                policy.profile,
                colima_profile_recovery_advice(&policy.profile, advice_mode)
            ),
        });
    }
    Ok(output.status.success() && parse_colima_running(&stdout, &stderr))
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
    let output = colima_status_output(policy, repo_root, deadline)?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_colima_running(&stdout, &stderr))
}

fn colima_status_output(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let cmd = colima_status_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    run_command_capture_allow_failure_with_deadline(
        repo_root,
        &cmd.program,
        &args,
        &cmd.label,
        deadline,
    )
}

fn colima_nerdctl_args(policy: &EffectiveContainerPolicy, docker_args: &[&str]) -> Vec<OsString> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(BackendId::colima_nerdctl());
    let args: Vec<OsString> = docker_args.iter().map(OsString::from).collect();
    ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", &args)
        .map(|(_, args)| args)
        .unwrap_or_else(|_| {
            let mut fallback = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
            ];
            fallback.extend(args);
            fallback
        })
}

fn colima_runtime_probe_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    colima_nerdctl_args(policy, &["info"])
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
    let per_probe_timeout = if timeout.is_zero() {
        Duration::ZERO
    } else {
        (timeout / 2)
            .max(Duration::from_millis(1))
            .min(timeout)
            .min(COLIMA_RUNTIME_PROBE_TIMEOUT)
    };
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
        "Colima containerd and BuildKit did not become ready within {}; last runtime probe: {}",
        super::process::format_timeout_duration(timeout),
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
            "Colima profile `{}` is running with {}GiB RAM; Effigy recommends {}GiB memory and {}GiB swap{} for workspace-heavy Rust builds. Run `effigy container profile resize` to stop and restart it; the command may irreversibly grow disk when below the managed target.",
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
            "Colima profile `{}` is running with {}GiB disk; Effigy recommends {}GiB disk for multi-repo Rust/container workspaces. Run `effigy container profile resize` to grow it explicitly; disk growth is irreversible.",
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
    let mut detection =
        crate::backend::ambient_detection(ContainerBackendDetection::from_env_and_path());
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
    Ok(running_nerdctl_profile_names(profiles))
}

/// Read the runtime and state for a named Colima profile, including profiles
/// that cannot be used with nerdctl.
///
/// # Errors
///
/// Returns an error when Colima cannot list or parse its profiles.
pub fn colima_profile_info(
    repo_root: &Path,
    profile: &str,
) -> Result<Option<ColimaProfileInfo>, ContainerExecError> {
    Ok(list_colima_profiles(repo_root)?
        .into_iter()
        .find(|entry| entry.name == profile)
        .map(|entry| ColimaProfileInfo {
            status: entry.status,
            runtime: (!entry.runtime.is_empty()).then_some(entry.runtime),
        }))
}

fn is_nerdctl_compatible_profile(entry: &ColimaProfileEntry) -> bool {
    runtime_supports_nerdctl(&entry.runtime)
}

fn runtime_supports_nerdctl(runtime: &str) -> bool {
    runtime.is_empty() || runtime.eq_ignore_ascii_case("containerd")
}

fn running_nerdctl_profile_names(profiles: Vec<ColimaProfileEntry>) -> Vec<String> {
    profiles
        .into_iter()
        .filter(|entry| {
            is_nerdctl_compatible_profile(entry) && entry.status.eq_ignore_ascii_case("running")
        })
        .map(|entry| entry.name)
        .collect()
}

fn nerdctl_profile_names_for_all_states(
    entries: Vec<ColimaProfileEntry>,
) -> (Vec<String>, Vec<String>) {
    let mut running = Vec::new();
    let mut stopped = Vec::new();
    for entry in entries {
        if !is_nerdctl_compatible_profile(&entry) {
            continue;
        }
        if entry.status.eq_ignore_ascii_case("running") {
            running.push(entry.name);
        } else {
            stopped.push(entry.name);
        }
    }
    (running, stopped)
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
    Ok(nerdctl_profile_names_for_all_states(entries))
}

pub(crate) fn default_runtime_profile() -> String {
    user_global_colima_profile().unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned())
}

/// Run one bounded runtime command on the policy's exact backend.
/// Launch, exit and timeout failures are propagated without runtime recovery.
pub fn run_runtime_command_capture_for_policy_with_timeout(
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

/// Deadline-aware allow-failure runtime capture on the policy's exact backend.
/// One caller-visible deadline supervises the spawn; with none the unbounded
/// path keeps its current behavior.
pub(super) fn run_runtime_command_capture_for_policy_allow_failure_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
    label: &str,
    deadline: Option<Instant>,
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
    run_command_capture_allow_failure_with_deadline(
        repo_root,
        &program,
        &rendered.iter().map(String::as_str).collect::<Vec<_>>(),
        label,
        deadline,
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

mod recovery;
pub use recovery::{
    recover_colima_runtime, recover_colima_runtime_with_progress, reset_colima_runtime,
    reset_colima_runtime_with_progress, ColimaRecoveryPhase,
};

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
                runtime: value
                    .get("runtime")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
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
    fn parse_colima_profiles_reads_disk_bytes() {
        let entries = parse_colima_profiles(
            r#"{"name":"effigy","status":"Running","cpus":2,"memory":34359738368,"disk":322122547200}"#,
        )
        .expect("parse profiles");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "effigy");
        assert_eq!(entries[0].runtime, "");
        assert_eq!(entries[0].disk, 300 * 1024 * 1024 * 1024);
    }

    #[test]
    fn colima_profile_discovery_skips_explicit_non_containerd_runtimes() {
        let entries = parse_colima_profiles(
            concat!(
                r#"{"name":"containerd","status":"Running","runtime":"containerd","cpus":2,"memory":1,"disk":1}"#,
                "\n",
                r#"{"name":"docker","status":"Running","runtime":"docker","cpus":2,"memory":1,"disk":1}"#,
                "\n",
                r#"{"name":"legacy","status":"Running","cpus":2,"memory":1,"disk":1}"#,
                "\n",
                r#"{"name":"stopped-docker","status":"Stopped","runtime":"docker","cpus":2,"memory":1,"disk":1}"#,
                "\n",
                r#"{"name":"stopped-legacy","status":"Stopped","runtime":"","cpus":2,"memory":1,"disk":1}"#
            ),
        )
        .expect("parse profiles");

        assert_eq!(entries[0].runtime, "containerd");
        assert_eq!(entries[1].runtime, "docker");
        assert_eq!(entries[2].runtime, "");
        assert_eq!(
            running_nerdctl_profile_names(entries.clone()),
            ["containerd", "legacy"]
        );
        assert_eq!(
            nerdctl_profile_names_for_all_states(entries),
            (
                vec!["containerd".to_owned(), "legacy".to_owned()],
                vec!["stopped-legacy".to_owned()]
            )
        );
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
        assert_eq!(
            error,
            "Colima containerd and BuildKit did not become ready within 10ms; last runtime probe: containerd endpoint unavailable"
        );
    }

    #[test]
    fn runtime_readiness_subsecond_final_probe_never_reports_zero_seconds_and_names_window() {
        let mut probe_calls = 0;
        let window_error = wait_for_runtime_readiness(
            Duration::from_millis(1200),
            Duration::from_millis(1),
            |remaining| {
                probe_calls += 1;
                if probe_calls == 1 {
                    std::thread::sleep(Duration::from_millis(850));
                    Err("nerdctl info: containerd not ready yet".to_owned())
                } else {
                    assert!(
                        remaining < Duration::from_secs(1),
                        "remaining={remaining:?}"
                    );
                    let per_probe = (remaining / 2).max(Duration::from_millis(1));
                    let failure = run_command_capture_with_timeout(
                        Path::new("."),
                        "/bin/sh",
                        &["-c", "/bin/sleep 2"],
                        "Colima containerd readiness probe",
                        per_probe,
                    )
                    .expect_err("sub-second final probe should time out");
                    Err(failure.to_string())
                }
            },
            std::thread::sleep,
        )
        .expect_err("readiness window should time out");

        assert!(
            window_error.contains("within 1s"),
            "names readiness window: {window_error}"
        );
        assert!(
            !window_error.contains("after 0s"),
            "never truncates sub-second probe to 0s: {window_error}"
        );
        assert!(
            !window_error.contains("\nstdout:\n"),
            "omits empty stdout block: {window_error}"
        );
        assert!(
            window_error.contains("ms: owned capture child terminated and reaped"),
            "reports sub-second timeout honestly in milliseconds: {window_error}"
        );
    }

    #[test]
    fn warm_colima_profile_is_reused_without_starting_units() {
        let mut wait_timeouts = Vec::new();
        let started = ensure_running_profile_runtime_ready(
            "effigy",
            Duration::from_secs(30),
            ColimaAdviceMode::Automated,
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
            ColimaAdviceMode::Automated,
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
            ColimaAdviceMode::Automated,
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
        assert!(rendered.contains("report the degraded profile to the host operator"));
        assert!(!rendered.contains("effigy container profile repair"));
        assert!(!rendered.contains("effigy container profile reset-runtime"));
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
            ColimaAdviceMode::Automated,
            |_| Err("buildctl cannot reach BuildKit".to_owned()),
            || Err("systemctl start containerd buildkit timed out".to_owned()),
        )
        .expect_err("a refused unit start does not fall back to a VM restart");

        let rendered = error.to_string();
        assert!(
            rendered.contains("systemctl start containerd buildkit timed out"),
            "names the unit-start failure: {rendered}"
        );
        assert!(rendered.contains("report the degraded profile to the host operator"));
        assert!(!rendered.contains("effigy container profile repair"));
    }

    #[test]
    fn colima_recovery_advice_matches_interactive_and_automated_callers() {
        assert_eq!(
            advice_mode_for_caller(true, true, false, false, false),
            ColimaAdviceMode::Interactive
        );
        for signals in [
            (false, true, false, false, false),
            (true, false, false, false, false),
            (true, true, true, false, false),
            (true, true, false, true, false),
            (true, true, false, false, true),
        ] {
            assert_eq!(
                advice_mode_for_caller(signals.0, signals.1, signals.2, signals.3, signals.4),
                ColimaAdviceMode::Automated
            );
        }
        let interactive = colima_profile_recovery_advice("effigy", ColimaAdviceMode::Interactive);
        assert!(interactive.contains("colima stop --profile effigy"));
        assert!(interactive.contains("stops every container on that profile"));

        let automated = colima_profile_recovery_advice("effigy", ColimaAdviceMode::Automated);
        assert!(automated.contains("shared across checkouts"));
        assert!(automated.contains("must not stop or restart it"));
        assert!(automated.contains("report the degraded profile to the host operator"));
        assert!(!automated.contains("colima stop"));
        assert!(!automated.contains("effigy container profile repair"));
        assert!(!automated.contains("effigy container profile reset-runtime"));

        let interactive_force =
            colima_profile_force_stop_advice("effigy", ColimaAdviceMode::Interactive);
        assert!(interactive_force.contains("colima stop -p effigy --force"));
        assert!(interactive_force.contains("stops every container on the shared profile"));

        let automated_force =
            colima_profile_force_stop_advice("effigy", ColimaAdviceMode::Automated);
        assert!(automated_force.contains("report the degraded profile to the host operator"));
        assert!(!automated_force.contains("colima stop"));
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
    if [ -f "$root/runtime-empty" ]; then
      printf 'error retrieving current runtime: empty value\n' >&2
      exit 1
    elif [ -f "$root/status-unknown" ]; then
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
      if [ -f "$root/under-floor" ]; then memory=2147483648; disk=1073741824
      elif [ -f "$root/small" ]; then memory=8589934592; disk=1073741824
      else memory=34359738368; disk=322122547200; fi
      if [ -f "$root/release-profile" ]; then profile=effigy-release; else profile=effigy; fi
      printf '{"name":"%s","status":"%s","cpus":2,"memory":%s,"disk":%s}\n' "$profile" "$state" "$memory" "$disk"
    fi
    ;;
  start)
    printf '%s\n' "$PWD" "${TMPDIR-unset}" "${TMP-unset}" "${TEMP-unset}" "$PATH" "$HOME" "$COLIMA_HOME" "$SSH_AUTH_SOCK" > "$root/start-env"
    : > "$root/running"; : > "$root/listed"; printf 'started\n' ;;
  stop)
    if [ -f "$root/pause-on-stop" ]; then
      : > "$root/stop-entered"
      n=0
      while [ ! -f "$root/release-stop" ] && [ "$n" -lt 250 ]; do
        n=$((n + 1))
        sleep 0.02
      done
    fi
    if [ ! -f "$root/stop-stays-running" ]; then rm -f "$root/running"; fi; printf 'stopped\n' ;;
  nerdctl)
    if [ -f "$root/offline" ]; then
      printf '%s\n' 'failed to solve: rust:1.88-bookworm: failed to resolve source metadata for docker.io/library/rust:1.88-bookworm: dial tcp: lookup registry-1.docker.io on 192.168.5.3:53: read udp: connection refused' >&2
      exit 1
    fi
    if printf '%s\n' "$*" | grep -q -- '-- ps -q'; then
      if [ -f "$root/ps-fails" ]; then
        printf 'containerd unavailable\n' >&2
        exit 1
      fi
      if [ -f "$root/ps-running" ]; then
        printf 'abc123def456\n'
      fi
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
                ("COLIMA_HOME", home.join(".colima").into_os_string()),
                ("SSH_AUTH_SOCK", "fixture-agent".into()),
                ("TMPDIR", root.clone().into_os_string()),
                ("TMP", root.clone().into_os_string()),
                ("TEMP", root.clone().into_os_string()),
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

        fn release_policy(&self) -> EffectiveContainerPolicy {
            let mut policy = self.policy();
            policy.profile = "effigy-release".to_owned();
            policy
        }

        fn state(&self, name: &str) {
            std::fs::write(self.root.join(name), "").expect("fixture state");
        }

        fn has_state(&self, name: &str) -> bool {
            self.root.join(name).is_file()
        }

        fn write_colima_config(&self, body: &str) {
            self.write_colima_profile_config("effigy", body);
        }

        fn write_colima_profile_config(&self, name: &str, body: &str) {
            let profile = self.root.join("home/.colima").join(name);
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

        fn clear_calls(&self) {
            let _ = std::fs::remove_file(self.root.join("colima-calls"));
        }
    }

    #[test]
    fn colima_starts_and_recovery_use_neutral_host_environment() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        let policy = fixture.policy();
        let assert_start = || {
            let recorded = std::fs::read_to_string(fixture.root.join("start-env"))
                .expect("recorded start environment");
            let lines: Vec<_> = recorded.lines().collect();
            assert_eq!(lines[0], fixture.home.to_str().unwrap());
            assert_eq!(&lines[1..4], &["unset", "unset", "unset"]);
            assert_eq!(lines[4], std::env::var("PATH").unwrap());
            assert_eq!(lines[5], fixture.home.to_str().unwrap());
            assert_eq!(lines[6], fixture.home.join(".colima").to_str().unwrap());
            assert_eq!(lines[7], "fixture-agent");
        };
        ensure_colima_running_report(&policy, &fixture.root).expect("start profile");
        assert_start();
        std::fs::remove_file(fixture.root.join("start-env")).expect("clear recording");
        recover_colima_runtime(&policy, &fixture.root).expect("recover profile");
        assert_start();
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

    /// Repair and reset stop the shared profile gracefully and report every
    /// phase they put in flight, so the owning command plane can name the
    /// command actually running instead of a generic recovery wait (Q-017).
    /// The fixture's fake `colima` is the only executable on the path, so
    /// nothing here touches a real runtime.
    #[test]
    fn colima_runtime_repair_and_reset_stop_gracefully_and_report_phases() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let policy = fixture.policy();

        // Phase 1: explicit repair and reset stop gracefully, never forced.
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

        // Phase 2: the same flows report each phase they put in flight.
        fixture.state("running");
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
    fn unusable_running_profiles_are_refused_without_touching_the_vm() {
        // Phase 1: a profile that stays running refuses recovery, and a
        // profile with unproven status refuses reset — neither starts.
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("stop-stays-running");
        let policy = fixture.policy();
        let error = recovery::recover_colima_runtime_with_progress_and_advice_mode(
            &policy,
            &fixture.root,
            &mut |_| {},
            ColimaAdviceMode::Automated,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "colima status failed (no exit code: the command ended on a signal)\nstdout:\n[check] recovering Colima profile `effigy`\n[ok] stopped Colima profile `effigy`\nstderr:\nprofile is still running; refusing profile recovery. Colima profile `effigy` is shared across checkouts. Automated runs must not stop or restart it; report the degraded profile to the host operator. Effigy did not run it."
        );
        assert!(!fixture
            .calls()
            .iter()
            .any(|call| call.starts_with("start ")));
        fixture.state("status-unknown");
        let error = recovery::reset_colima_runtime_with_progress_and_advice_mode(
            &policy,
            &fixture.root,
            &mut |_| {},
            ColimaAdviceMode::Automated,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "colima status failed (no exit code: the command ended on a signal)\nstdout:\n[check] resetting Colima profile `effigy` to a fully stopped state\n[ok] stopped Colima profile `effigy`\nstderr:\ncolima status failed (exit code 1)\nstderr:\nColima stop status is unproven: status unavailable\n; refusing profile recovery. Colima profile `effigy` is shared across checkouts. Automated runs must not stop or restart it; report the degraded profile to the host operator. Effigy did not run it."
        );

        // Phase 2: a registry failure never stops or restarts the shared profile.
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
        match failure {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "compose up");
                assert!(
                    stderr.contains("registry-1.docker.io"),
                    "the runtime's own cause reaches the caller: {stderr}"
                );
            }
            other => panic!("expected the compose failure, got {other}"),
        }
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
    fn managed_profile_lifecycle_creates_starts_warns_then_reuses() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        let policy = fixture.policy();
        const MANAGED_START: &str = "start --profile effigy --runtime containerd --ssh-agent --memory 32 --mount-inotify=false --dns 1.1.1.1 --dns 8.8.8.8";

        // Phase 1: an absent profile is created with the managed arguments.
        let report = ensure_colima_running_report(&policy, &fixture.root).expect("create profile");
        assert!(report.started);
        assert!(report.profile_created_or_restarted);
        assert!(fixture.has_state("running"));
        let calls = fixture.calls();
        assert!(
            calls.contains(&MANAGED_START.to_owned()),
            "the create start carries the managed runtime, memory and dns arguments: {calls:#?}"
        );
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "a missing profile is created, never stopped: {calls:#?}"
        );

        // Phase 2: a stopped profile is started, not recreated.
        std::fs::remove_file(fixture.root.join("running")).expect("stop the profile");
        let report = ensure_colima_running_report(&policy, &fixture.root).expect("start profile");
        assert!(report.started);
        assert!(!report.profile_created_or_restarted);
        assert!(fixture.has_state("running"));
        let calls = fixture.calls();
        let starts: Vec<_> = calls
            .iter()
            .filter(|call| call.starts_with("start "))
            .collect();
        assert_eq!(starts, [MANAGED_START, MANAGED_START], "{calls:#?}");
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "a stopped profile is started, never stopped or recreated: {calls:#?}"
        );

        // Phase 3: a drifted running profile is never stopped or restarted,
        // and its config is rewritten to the managed settings so the
        // operator's own `colima restart effigy` applies them.
        fixture.write_colima_config("forwardAgent: false\n");
        let report = ensure_colima_running_report(&policy, &fixture.root).expect("reuse profile");
        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("managed config");
        assert!(config.contains("forwardAgent: true"), "{config}");

        // Phase 4: with the config now aligned, a further ensure reuses the
        // running profile without a restart.
        let report = ensure_colima_running_report(&policy, &fixture.root).expect("reuse profile");
        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);

        // Across the whole lifecycle only the two starts ran: create, then
        // the stopped-profile start. Nothing was ever stopped or restarted.
        let calls = fixture.calls();
        let starts: Vec<_> = calls
            .iter()
            .filter(|call| call.starts_with("start "))
            .collect();
        assert_eq!(starts, [MANAGED_START, MANAGED_START], "{calls:#?}");
        assert!(
            !calls.iter().any(|call| call.starts_with("stop ")),
            "the shared profile is never stopped: {calls:#?}"
        );
    }

    #[test]
    fn colima_running_release_profile_with_empty_runtime_reports_remedy() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("release-profile");
        fixture.state("runtime-empty");
        let error = ensure_colima_running_report_with_advice_mode(
            &fixture.release_policy(),
            &fixture.root,
            ColimaAdviceMode::Automated,
        )
        .expect_err("runtime record loss must stop before starting or waiting");
        let message = error.to_string();
        assert!(message.contains("shared across checkouts"), "{message}");
        assert!(
            message.contains("report the degraded profile to the host operator"),
            "{message}"
        );
        assert!(!message.contains("colima stop"), "{message}");
        let calls = fixture.calls();
        assert_eq!(
            calls,
            ["status --profile effigy-release", "list --json"],
            "{calls:?}"
        );
    }

    #[test]
    fn running_release_profile_passes_at_8_gib_and_refuses_at_2_gib() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("small");
        fixture.state("release-profile");
        let policy = fixture.release_policy();
        fixture.write_colima_profile_config("effigy-release", "memory: 8\nforwardAgent: true\n");

        ensure_running_managed_profile_ready(&policy, &fixture.root, ColimaAdviceMode::Automated)
            .expect("running release profile meets the 8 GiB floor");

        std::fs::remove_file(fixture.root.join("small")).expect("set below-floor memory");
        fixture.state("under-floor");
        fixture.write_colima_profile_config("effigy-release", "memory: 2\nforwardAgent: true\n");
        let error = ensure_running_managed_profile_ready(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Automated,
        )
        .expect_err("running release profile below the floor refuses");
        let message = error.to_string();
        assert!(message.contains("running with 2GiB RAM"), "{message}");
        assert!(message.contains("requires 8GiB"), "{message}");
        assert!(
            message.contains("colima restart effigy-release"),
            "{message}"
        );
        assert!(message.contains("did not restart"), "{message}");
    }

    #[test]
    fn running_default_profile_passes_at_its_plan_and_refuses_below_it() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let policy = fixture.policy();
        let plan = crate::colima::managed_colima_profile_resources("effigy")
            .expect("default profile has a managed resource plan");
        fixture.write_colima_config(&format!(
            "memory: {}\nforwardAgent: true\n",
            plan.memory_gib
        ));

        ensure_running_managed_profile_ready(&policy, &fixture.root, ColimaAdviceMode::Automated)
            .expect("running default profile meets its host-derived memory plan");

        fixture.state("under-floor");
        fixture.write_colima_config("memory: 2\nforwardAgent: true\n");
        let error = ensure_running_managed_profile_ready(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Automated,
        )
        .expect_err("running default profile below its plan refuses");
        let message = error.to_string();
        assert!(message.contains("running with 2GiB RAM"), "{message}");
        assert!(
            message.contains(&format!("requires {}GiB", plan.memory_gib)),
            "{message}"
        );
        assert!(message.contains("colima restart effigy"), "{message}");
        assert!(message.contains("did not restart"), "{message}");
    }

    #[test]
    fn no_change_verifies_without_touching_anything() {
        use crate::exec::ContainerExecError;
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        let policy = fixture.policy();
        let no_change_refusal = |error: ContainerExecError, reason: &str| match error {
            ContainerExecError::Failure {
                command,
                code,
                stdout,
                stderr,
            } => {
                assert_eq!(command, "Colima profile no-change check");
                assert_eq!(code, None);
                assert!(stdout.is_empty());
                assert!(
                    stderr.starts_with(&format!("Colima profile `effigy` {reason}\n")),
                    "expected summary line first, got: {stderr}"
                );
                assert!(
                    stderr.contains("no-change mode refuses to start, create, restart, rewrite or unit-start the shared profile"),
                    "expected refusal explanation, got: {stderr}"
                );
                assert!(
                    stderr.contains("\nnext: "),
                    "expected next line, got: {stderr}"
                );
            }
            other => panic!("expected the no-change refusal, got {other}"),
        };

        // Phase 1: an absent profile is refused without starting it.
        let absent = verify_runtime_backend_no_change(&policy, &fixture.root)
            .expect_err("absent profile refuses");
        no_change_refusal(absent, "is not running");

        // Phase 2: a listed-but-stopped profile is refused the same way.
        fixture.state("listed");
        let stopped = verify_runtime_backend_no_change(&policy, &fixture.root)
            .expect_err("stopped profile refuses");
        no_change_refusal(stopped, "is not running");
        assert!(!fixture.has_state("running"));

        // Phase 3: a configured running profile is reused without any write.
        fixture.state("running");
        fixture.write_colima_config("forwardAgent: true\n");
        let report =
            verify_runtime_backend_no_change(&policy, &fixture.root).expect("reuse profile");
        assert_eq!(report, ColimaEnsureReport::default());
        let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("managed config");
        assert_eq!(config, "forwardAgent: true\n");

        // Phase 4: drifted managed settings are refused without rewriting them.
        fixture.write_colima_config("forwardAgent: false\n");
        let error = verify_runtime_backend_no_change(&policy, &fixture.root)
            .expect_err("drifted settings refuse");
        no_change_refusal(error, "has managed settings that differ from Effigy's");
        let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("unchanged config");
        assert_eq!(
            config, "forwardAgent: false\n",
            "no-change never rewrites settings"
        );

        // Across all four phases nothing was started, stopped, restarted or deleted.
        let calls = fixture.calls();
        assert!(
            !calls.iter().any(|call| call.starts_with("start ")
                || call.starts_with("stop ")
                || call.starts_with("restart ")
                || call.starts_with("delete ")),
            "no-change never starts or creates the profile: {calls:#?}"
        );
    }

    /// A fake Docker CLI double that records the arguments it receives and
    /// answers the liveness probe from a fixture marker file.
    const DOCKER_BACKEND_STUB: &str = "#!/bin/sh\n# Private Docker CLI double: records its arguments, answers from markers.\nprintf '%s\\n' \"$*\" >> '__FIXTURE__/docker-calls'\nif [ -f '__FIXTURE__/ps-fails' ]; then\n    exit 1\nfi\nexit 0\n";

    /// A Docker-backend host with no `colima` binary anywhere on `PATH`.
    ///
    /// This is the Native Linux qualification host (2026-11-20 evidence run):
    /// Docker Engine is the resolved backend, and every `colima` invocation is
    /// an `ENOENT` launch failure.
    struct DockerBackendFixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        _env: EnvRestore,
    }

    impl DockerBackendFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path().canonicalize().expect("canonical fixture");
            let bin = root.join("bin");
            let home = root.join("home");
            std::fs::create_dir_all(&bin).expect("mkdir bin");
            std::fs::create_dir_all(&home).expect("mkdir home");
            let docker = bin.join("docker");
            std::fs::write(
                &docker,
                DOCKER_BACKEND_STUB.replace("__FIXTURE__", &root.to_string_lossy()),
            )
            .expect("write fake docker");
            let mut permissions = std::fs::metadata(&docker).expect("metadata").permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
            std::fs::set_permissions(&docker, permissions).expect("chmod fake docker");
            // Only the fixture's own bin: `colima` is not merely unselected,
            // it is absent, exactly as on the failing host.
            let env = EnvRestore::set(&[
                ("PATH", bin.into_os_string()),
                ("HOME", home.into_os_string()),
                ("EFFIGY_COMPOSE_BACKEND", "docker".into()),
            ]);
            Self {
                _dir: dir,
                root,
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

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.root.join("docker-calls"))
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn clear_calls(&self) {
            let _ = std::fs::remove_file(self.root.join("docker-calls"));
        }
    }

    /// Liveness on the resolved Docker backend asks `docker ps` and never
    /// spawns `colima` (effigy#1087). Before this, all three call sites asked
    /// Colima regardless of the backend and failed with `ENOENT` on any host
    /// without a `colima` binary.
    #[test]
    fn docker_backend_liveness_probes_docker_ps_and_never_colima() {
        let _lock = crate::test_env_lock();
        let fixture = DockerBackendFixture::new();
        let policy = fixture.policy();

        assert!(
            runtime_backend_is_running(&policy, &fixture.root).expect("docker backend liveness"),
            "a successful `docker ps` is a running backend"
        );
        assert!(
            runtime_backend_is_running_with_deadline(&policy, &fixture.root, None)
                .expect("deadline-aware docker backend liveness"),
            "the deadline-aware form keeps the same answer"
        );
        let calls = fixture.calls();
        assert_eq!(calls, ["ps".to_owned(), "ps".to_owned()], "{calls:#?}");

        // A failing `docker ps` is the engine's own "not running" answer, not
        // a launch failure.
        fixture.state("ps-fails");
        assert!(
            !runtime_backend_is_running(&policy, &fixture.root).expect("stopped docker backend"),
            "a failed `docker ps` is a stopped backend"
        );

        // An expired deadline refuses before the spawn.
        fixture.clear_calls();
        let deadline = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("expired deadline");
        let error =
            runtime_backend_is_running_with_deadline(&policy, &fixture.root, Some(deadline))
                .expect_err("an expired deadline must refuse before spawning");
        assert!(error.to_string().contains("command timed out"), "{error}");
        let calls = fixture.calls();
        assert!(
            calls.is_empty(),
            "an expired deadline never spawns: {calls:#?}"
        );
    }

    /// The Colima backend keeps its own read-only probe: selecting Docker never
    /// changes what Colima hosts do.
    #[test]
    fn colima_backend_liveness_still_probes_colima_status() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let policy = fixture.policy();

        assert!(
            runtime_backend_is_running_with_deadline(&policy, &fixture.root, None)
                .expect("colima backend liveness")
        );
        let calls = fixture.calls();
        assert!(
            calls
                .iter()
                .any(|call| call.starts_with("status --profile effigy")),
            "{calls:#?}"
        );
    }

    #[test]
    fn interactive_idle_running_managed_profile_migrates_mount_inotify_false() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.write_colima_config("memory: 32\nforwardAgent: true\nmountInotify: true\n");
        let policy = fixture.policy();

        let report = ensure_colima_running_report_with_advice_mode(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Interactive,
        )
        .expect("idle running profile should migrate");

        assert!(report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert_eq!(calls.len(), 7, "{calls:#?}");
        assert_eq!(
            &calls[..6],
            [
                "status --profile effigy",
                "nerdctl --profile effigy -- ps -q",
                "stop --profile effigy",
                "start --profile effigy --runtime containerd --ssh-agent --memory 32 --mount-inotify=false --dns 1.1.1.1 --dns 8.8.8.8",
                "nerdctl --profile effigy -- info",
                "ssh --profile effigy -- sudo buildctl debug workers",
            ],
            "{calls:#?}"
        );
        assert!(
            calls[6].starts_with("ssh --profile effigy -- sudo sh -c "),
            "{calls:#?}"
        );
        assert_eq!(
            crate::colima_profile_mount_inotify("effigy"),
            Some(false),
            "colima.yaml must be rewritten to mountInotify: false"
        );
        let recorded = std::fs::read_to_string(fixture.root.join("start-env"))
            .expect("recorded start environment");
        assert_eq!(
            recorded.lines().next(),
            Some(fixture.home.to_str().unwrap()),
            "migrated start must use neutral $HOME"
        );
    }

    #[test]
    fn interactive_busy_or_unlisted_running_profile_skips_mount_inotify_migration() {
        let _lock = crate::test_env_lock();
        let original = "memory: 32\nforwardAgent: true\nmountInotify: true\n";

        for marker in ["ps-running", "ps-fails"] {
            let fixture = ManagedProfileFixture::new();
            fixture.state("listed");
            fixture.state("running");
            fixture.state(marker);
            fixture.write_colima_config(original);
            let policy = fixture.policy();

            let report = ensure_colima_running_report_with_advice_mode(
                &policy,
                &fixture.root,
                ColimaAdviceMode::Interactive,
            )
            .expect("busy or failed ps must reuse profile without restart");

            assert!(!report.started, "{marker}");
            assert!(!report.profile_created_or_restarted, "{marker}");
            let calls = fixture.calls();
            assert!(
                !calls
                    .iter()
                    .any(|call| call.starts_with("stop ") || call.starts_with("start ")),
                "{marker}: must not stop or restart: {calls:#?}"
            );
            assert!(
                calls.contains(&"nerdctl --profile effigy -- ps -q".to_owned()),
                "{marker}: {calls:#?}"
            );
            let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
                .expect("read config");
            assert_eq!(
                config, original,
                "{marker}: colima.yaml must remain untouched"
            );
        }
    }

    #[test]
    fn automated_running_profile_never_stops_or_edits_mount_inotify() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let original = "memory: 32\nforwardAgent: true\nmountInotify: true\n";
        fixture.write_colima_config(original);
        let policy = fixture.policy();

        let report = ensure_colima_running_report_with_advice_mode(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Automated,
        )
        .expect("automated mode must reuse profile");

        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert_eq!(calls.len(), 5, "{calls:#?}");
        assert_eq!(
            &calls[..4],
            [
                "status --profile effigy",
                "list --json",
                "nerdctl --profile effigy -- info",
                "ssh --profile effigy -- sudo buildctl debug workers",
            ],
            "automated mode must not run ps -q, stop, or start: {calls:#?}"
        );
        assert!(
            calls[4].starts_with("ssh --profile effigy -- sudo sh -c "),
            "{calls:#?}"
        );
        let config = std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("read config");
        assert_eq!(
            config, original,
            "automated mode must leave colima.yaml untouched"
        );
    }

    #[test]
    fn running_profile_already_mount_inotify_false_skips_ps_with_zero_extra_calls() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.write_colima_config("memory: 32\nforwardAgent: true\nmountInotify: false\n");
        let policy = fixture.policy();

        let report = ensure_colima_running_report_with_advice_mode(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Interactive,
        )
        .expect("already migrated profile should reuse cleanly");

        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert_eq!(calls.len(), 5, "{calls:#?}");
        assert_eq!(
            &calls[..4],
            [
                "status --profile effigy",
                "list --json",
                "nerdctl --profile effigy -- info",
                "ssh --profile effigy -- sudo buildctl debug workers",
            ],
            "must skip ps -q, stop, and start: {calls:#?}"
        );
        assert!(
            calls[4].starts_with("ssh --profile effigy -- sudo sh -c "),
            "{calls:#?}"
        );

        // Missing config (`None`) and unmanaged profile (`custom` even with
        // `mountInotify: true`) also skip ps -q, stop, and start.
        std::fs::remove_file(fixture.home.join(".colima/effigy/colima.yaml"))
            .expect("remove config");
        fixture.clear_calls();
        let missing_report = ensure_colima_running_report_with_advice_mode(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Interactive,
        )
        .expect("missing config reuses without migration calls");
        assert!(!missing_report.started);
        assert!(
            !fixture.calls().iter().any(|call| call.contains("ps -q")
                || call.starts_with("stop ")
                || call.starts_with("start ")),
            "missing config must skip migration calls: {:#?}",
            fixture.calls()
        );

        let mut unmanaged = fixture.policy();
        unmanaged.profile = "custom".to_owned();
        let unmanaged_cfg = "memory: 4\nmountInotify: true\n";
        fixture.write_colima_profile_config("custom", unmanaged_cfg);
        fixture.clear_calls();
        let unmanaged_report = ensure_colima_running_report_with_advice_mode(
            &unmanaged,
            &fixture.root,
            ColimaAdviceMode::Interactive,
        )
        .expect("unmanaged profile reuses without migration calls");
        assert!(!unmanaged_report.started);
        assert!(
            !fixture.calls().iter().any(|call| call.contains("ps -q")
                || call.starts_with("stop ")
                || call.starts_with("start ")),
            "unmanaged profile must skip migration calls: {:#?}",
            fixture.calls()
        );
        assert_eq!(
            std::fs::read_to_string(fixture.home.join(".colima/custom/colima.yaml"))
                .expect("read unmanaged config"),
            unmanaged_cfg
        );
    }

    #[test]
    fn deferred_mount_inotify_migration_preserves_colima_yaml_on_concurrent_drift() {
        let _lock = crate::test_env_lock();

        // Concurrent SSH-agent drift (`forwardAgent: false`) with `mountInotify: true`:
        // automated mode and interactive busy/failed ps-q must NOT rewrite `colima.yaml`.
        let drifted_ssh = "memory: 32\nforwardAgent: false\nmountInotify: true\n";
        for (mode, marker) in [
            (ColimaAdviceMode::Automated, None),
            (ColimaAdviceMode::Interactive, Some("ps-running")),
            (ColimaAdviceMode::Interactive, Some("ps-fails")),
        ] {
            let fixture = ManagedProfileFixture::new();
            fixture.state("listed");
            fixture.state("running");
            if let Some(marker) = marker {
                fixture.state(marker);
            }
            fixture.write_colima_config(drifted_ssh);
            let policy = fixture.policy();

            let report =
                ensure_colima_running_report_with_advice_mode(&policy, &fixture.root, mode)
                    .expect("deferred migration with concurrent SSH drift reuses safely");

            assert!(!report.started, "{mode:?} {marker:?}");
            assert!(!report.profile_created_or_restarted, "{mode:?} {marker:?}");
            assert_eq!(
                std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
                    .expect("read config"),
                drifted_ssh,
                "{mode:?} {marker:?}: deferred migration must preserve live mountInotify: true and not rewrite colima.yaml"
            );
            assert_eq!(
                crate::colima_profile_mount_inotify("effigy"),
                Some(true),
                "{mode:?} {marker:?}: doctor must still observe mountInotify: true"
            );
        }

        // Concurrent memory-floor drift (`memory: 2` + `mountInotify: true`):
        // automated mode refuses on RAM floor without rewriting `colima.yaml`.
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("under-floor");
        let drifted_mem = "memory: 2\nforwardAgent: true\nmountInotify: true\n";
        fixture.write_colima_config(drifted_mem);
        let policy = fixture.policy();

        let error = ensure_running_managed_profile_ready(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Automated,
        )
        .expect_err("under-floor RAM still refuses when migration is deferred");
        assert!(
            error.to_string().contains("running with 2GiB RAM"),
            "{error}"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
                .expect("read config"),
            drifted_mem,
            "memory-floor refusal during deferred migration must leave colima.yaml untouched"
        );
    }

    #[test]
    fn concurrent_bringup_guard_prevents_interactive_mount_inotify_stop() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        let original = "memory: 32\nforwardAgent: true\nmountInotify: true\n";
        fixture.write_colima_config(original);
        let policy = fixture.policy();

        // Simulate another checkout holding the shared bring-up guard between
        // `ensure_runtime_backend_running` and `compose up` (0 containers visible yet).
        let bringup_guard = acquire_colima_profile_bringup_guard(&policy, &fixture.root)
            .expect("acquire shared bring-up guard");

        let report = ensure_colima_running_report_with_advice_mode(
            &policy,
            &fixture.root,
            ColimaAdviceMode::Interactive,
        )
        .expect("interactive ensure should defer while another bring-up holds guard");

        assert!(!report.started);
        assert!(!report.profile_created_or_restarted);
        let calls = fixture.calls();
        assert!(
            !calls.iter().any(|call| call.contains("ps -q")
                || call.starts_with("stop ")
                || call.starts_with("start ")),
            "contended bring-up lock must defer without stopping or restarting: {calls:#?}"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.home.join(".colima/effigy/colima.yaml"))
                .expect("read config"),
            original,
            "contended bring-up lock must leave colima.yaml untouched"
        );
        drop(bringup_guard);
    }

    #[test]
    fn interactive_migration_serializes_concurrent_bringup_until_restart_completes() {
        let _lock = crate::test_env_lock();
        let fixture = ManagedProfileFixture::new();
        fixture.state("listed");
        fixture.state("running");
        fixture.state("pause-on-stop");
        fixture.write_colima_config("memory: 32\nforwardAgent: true\nmountInotify: true\n");
        let policy = fixture.policy();
        let root = fixture.root.clone();

        let (tx_entered, rx_entered) = std::sync::mpsc::channel();
        let (tx_done, rx_done) = std::sync::mpsc::channel();
        let policy_worker = policy.clone();
        let root_worker = root.clone();

        let migrator = std::thread::spawn(move || {
            let report = ensure_colima_running_report_with_advice_mode(
                &policy_worker,
                &root_worker,
                ColimaAdviceMode::Interactive,
            );
            let _ = tx_done.send(report);
        });

        // Wait until the migrator holds the exclusive lock inside `colima stop`.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("stop-entered").is_file() {
            assert!(
                Instant::now() < deadline,
                "migrator never reached colima stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // Start a concurrent automated bring-up on the same profile; it must block
        // on the shared profile lock until migration finishes `colima start`, rather
        // than proceeding while `colima stop` is stopping the VM.
        let policy_bringup = policy.clone();
        let root_bringup = root.clone();
        let bringup = std::thread::spawn(move || {
            let _ = tx_entered.send(());
            ensure_colima_running_report_with_advice_mode(
                &policy_bringup,
                &root_bringup,
                ColimaAdviceMode::Automated,
            )
        });

        rx_entered.recv().expect("bring-up thread started");
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            rx_done.try_recv().is_err(),
            "migrator should still be paused inside stop"
        );
        // Verify the concurrent bring-up has NOT issued `status` or `info` yet while stop is paused.
        let calls_while_paused = fixture.calls();
        assert_eq!(
            calls_while_paused,
            [
                "status --profile effigy",
                "nerdctl --profile effigy -- ps -q",
                "stop --profile effigy",
            ],
            "concurrent bring-up must wait behind exclusive migration lock: {calls_while_paused:#?}"
        );

        // Release `colima stop` so migration completes `prepare` + `colima start`.
        fixture.state("release-stop");
        let mig_report = rx_done
            .recv_timeout(Duration::from_secs(5))
            .expect("migrator finishes")
            .expect("migrator succeeds");
        assert!(mig_report.started);
        migrator.join().expect("migrator thread");

        let bringup_report = bringup
            .join()
            .expect("bring-up thread")
            .expect("concurrent bring-up succeeds after migration");
        assert!(
            !bringup_report.started,
            "concurrent bring-up should reuse the freshly migrated VM"
        );
        assert_eq!(crate::colima_profile_mount_inotify("effigy"), Some(false));
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
            workspace_source: crate::artifact_source::WorkspaceSource::Checkout,
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
            fs_events: Default::default(),
        }
    }
}
