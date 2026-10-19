//! The task engine's handle on the runtime kernel: one state root per
//! process, operation contexts, the signal router and the sentinel adapter.
//! Contract 052 §5.2–§5.8; the kernel owns every mechanism used here.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, ObserveContext,
    OpenReason, OperationContext, Placement,
};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::outcome::Invocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Effect, Fence, Reconciled, SettlementAdapter};
use effigy_kernel::signal::{RouterHandle, RouterPolicy, RoutedSignal, SignalRouter};
use effigy_kernel::supervise::Executable;

use crate::runner::error::RunnerError;

/// Workload capture bounds. JSON task output keeps its own tail on top.
pub(super) const LIMITS: Limits = Limits {
    capture_tail: 64 * 1024,
    log_rotate: 1 << 20,
    log_keep: 3,
    line_max: 64 * 1024,
    queue_bytes: 1 << 20,
    probe_concurrency: 1,
};
/// Settle budget after a workload ends: grace, closure and journal commits.
pub(super) const CLEANUP_BUDGET: Duration = Duration::from_secs(60);
/// Observe budget for status reads and gate probes.
pub(super) const OBSERVE_BUDGET: Duration = Duration::from_secs(10);

/// The Effigy home that owns `~/.effigy/state`. Library tests in one process
/// share a private temporary home, never the operator's.
pub(super) fn state_home() -> Option<PathBuf> {
    #[cfg(test)]
    {
        static TEST_HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
        let home = TEST_HOME.get_or_init(|| {
            tempfile::Builder::new()
                .prefix("effigy-task-engine-home-")
                .tempdir()
                .expect("private test home")
        });
        Some(home.path().to_path_buf())
    }
    #[cfg(not(test))]
    {
        StateRoot::home_from_env()
    }
}

pub(super) fn operation(class: EffectClass, deadline: Deadline) -> OperationContext {
    OperationContext::new(deadline, cancel_token(), LIMITS, class, Placement::Host)
}

/// A Mutate context for one run's admission and effects. A task's workload
/// lifetime is its own; the kernel still bounds every store and gate wait.
pub(super) fn effects() -> EffectContext {
    operation(EffectClass::Mutate, Deadline::Open(OpenReason::Shell))
        .effects()
        .expect("mutate context has effects")
}

pub(super) fn cleanup() -> Result<CleanupContext, RunnerError> {
    operation(EffectClass::Mutate, Deadline::Open(OpenReason::Shell))
        .cleanup(CLEANUP_BUDGET)
        .map_err(|_| RunnerError::task_invocation("task cleanup budget overflow"))
}

pub(super) fn observe() -> ObserveContext {
    operation(
        EffectClass::Observe,
        Deadline::At(std::time::Instant::now() + OBSERVE_BUDGET),
    )
    .observe()
    .expect("observe context")
}

/// The writable state root, opened once per process for its first run.
pub(super) fn effects_root() -> Result<&'static StateRoot, RunnerError> {
    static ROOT: OnceLock<Result<StateRoot, String>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let home = state_home().ok_or_else(|| {
            "HOME is unset or relative; Effigy keeps run state under $HOME/.effigy/state".to_owned()
        })?;
        StateRoot::effects(&effects(), &home).map_err(|error| {
            format!(
                "cannot open the Effigy state root under {}: {error}",
                home.display()
            )
        })
    })
    .as_ref()
    .map_err(|message| RunnerError::task_invocation(message.clone()))
}

/// A read-only view: never creates the state root (R2).
pub(super) fn observe_root(cx: &ObserveContext) -> Observed<StateRoot> {
    match state_home() {
        Some(home) => StateRoot::observe(cx, &home),
        None => Observed::Unknown(UnknownReason::Untrusted(
            "HOME is unset or relative".to_owned(),
        )),
    }
}

/// This binary, identified once for every sentinel this process spawns.
pub(super) fn executable() -> Result<Executable, RunnerError> {
    static CURRENT: OnceLock<Result<Executable, String>> = OnceLock::new();
    CURRENT
        .get_or_init(|| Executable::current().map_err(|error| error.to_string()))
        .clone()
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot identify the effigy binary: {error}"))
        })
}

static ROUTER: OnceLock<Mutex<RouterHandle>> = OnceLock::new();
static CUSTODY_WAITING: AtomicBool = AtomicBool::new(false);

/// Installs the one process signal router before the first custody spawn
/// (K9). Commands that never spawn under custody keep default dispositions.
/// Once installed, a signal that arrives while no custody wait is polling
/// ends the process as its default action would.
pub(super) fn install_router() -> Result<(), RunnerError> {
    // Library tests run many tasks in one process that other tests signal;
    // the kill matrix proves routing against the real binary instead.
    if cfg!(test) {
        return Ok(());
    }
    if ROUTER.get().is_some() {
        return Ok(());
    }
    let handle = SignalRouter::install(RouterPolicy::Inline).map_err(|error| {
        RunnerError::task_invocation(format!("cannot install the signal router: {error}"))
    })?;
    if ROUTER.set(Mutex::new(handle)).is_err() {
        return Ok(());
    }
    std::thread::Builder::new()
        .name("effigy-signal-default".to_owned())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_millis(20));
            if CUSTODY_WAITING.load(Ordering::Acquire) {
                continue;
            }
            if let Some(signal) = poll_router().first() {
                std::process::exit(128 + signal.received);
            }
        })
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot start the signal watcher: {error}"))
        })?;
    Ok(())
}

pub(super) fn poll_router() -> Vec<RoutedSignal> {
    ROUTER
        .get()
        .map(|router| {
            router
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .poll()
        })
        .unwrap_or_default()
}

/// Marks a custody wait loop as the signal consumer while it lives.
pub(super) struct CustodyWait;
impl CustodyWait {
    pub(super) fn enter() -> Self {
        CUSTODY_WAITING.store(true, Ordering::Release);
        Self
    }
}
impl Drop for CustodyWait {
    fn drop(&mut self) {
        CUSTODY_WAITING.store(false, Ordering::Release);
    }
}

fn cancel_token() -> CancelToken {
    ROUTER
        .get()
        .map(|router| {
            router
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .root_cancel()
        })
        .unwrap_or_default()
}

/// Task runs record only kernel effects (custody). Any other effect is not
/// the task engine's, so recovery observes it as `Unknown` and leaves residue
/// rather than guessing (R3).
#[derive(Debug, Default)]
pub(in crate::runner) struct TaskSettlement;
impl SettlementAdapter for TaskSettlement {
    fn observe(&mut self, _effect: &Effect, _cx: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "not a task engine effect".to_owned(),
        ))
    }
    fn release(&mut self, _effect: &Effect, _cx: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "not a task engine effect".to_owned(),
        ))
    }
    fn reconcile(&mut self, _effect: &Effect, _cx: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "not a task engine effect".to_owned(),
        ))
    }
}

/// The recovery command a run records: rerunning the task settles an
/// interrupted predecessor before it is admitted (K5).
pub(super) fn next_invocation(selector: &str) -> Result<Invocation, RunnerError> {
    Invocation::validated(
        vec![OsString::from("effigy"), OsString::from(selector)],
        valid_next,
    )
    .map_err(|_| RunnerError::task_invocation(format!("`{selector}` is not a runnable selector")))
}

/// The engine's own parser accepts the exact recorded argument vector.
pub(in crate::runner) fn valid_next(argv: &[OsString]) -> bool {
    let Some((program, rest)) = argv.split_first() else {
        return false;
    };
    if program != "effigy" || rest.is_empty() {
        return false;
    }
    let Some(args) = rest
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    effigy_cli::parse_command(args).is_ok()
}

/// Hidden kernel entry points (`__kernel-sentinel`, `__kernel-supervise`).
/// `None` means an ordinary invocation.
pub fn run_internal_kernel_entry(entry: &str) -> Option<i32> {
    if !entry.starts_with("__kernel-") {
        return None;
    }
    let policy = if entry == "__kernel-supervise" {
        RouterPolicy::Supervisor
    } else {
        RouterPolicy::Sentinel
    };
    let mut router = match SignalRouter::install(policy) {
        Ok(router) => router,
        Err(_) => return Some(70),
    };
    let mut adapter = TaskSettlement;
    match effigy_kernel::supervise::internal_entry(entry, &mut router, &mut adapter, valid_next) {
        Some(Ok(())) => Some(0),
        Some(Err(_)) => Some(1),
        None => None,
    }
}

/// Root-relative, '/'-separated rendering of `path` under `root`, or `None`
/// when `path` lies outside it.
pub(super) fn relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// A library test binary is also its runs' sentinel: the kernel re-execs the
/// current executable with `__kernel-sentinel`. This constructor dispatches
/// that entry before the test harness parses its arguments.
#[cfg(test)]
#[used]
#[cfg_attr(target_os = "linux", link_section = ".init_array")]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
static TEST_BINARY_KERNEL_ENTRY: extern "C" fn() = {
    extern "C" fn dispatch() {
        // Linux std may populate `env::args` after this constructor runs.
        #[cfg(target_os = "linux")]
        let entry = std::fs::read("/proc/self/cmdline").ok().and_then(|raw| {
            raw.split(|byte| *byte == 0)
                .nth(1)
                .and_then(|arg| String::from_utf8(arg.to_vec()).ok())
        });
        #[cfg(not(target_os = "linux"))]
        let entry = std::env::args().nth(1);
        if let Some(code) = entry.as_deref().and_then(run_internal_kernel_entry) {
            std::process::exit(code);
        }
    }
    dispatch
};
