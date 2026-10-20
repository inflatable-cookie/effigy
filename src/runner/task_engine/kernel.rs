//! The task engine's handle on the runtime kernel: one state root per
//! process, operation contexts, the signal router and the sentinel adapter.
//! Contract 052 §5.2–§5.8; the kernel owns every mechanism used here.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
use effigy_kernel::signal::{RoutedSignal, RouterHandle, RouterPolicy, SignalRouter};
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

/// A detached supervisor records its run under its client's state home.
static STATE_HOME: OnceLock<PathBuf> = OnceLock::new();

/// Adopts the client's state home before this process opens any run.
pub(in crate::runner) fn adopt_state_home(home: PathBuf) {
    let _ = STATE_HOME.set(home);
}

/// The state home this process records runs under, for a supervisor it starts.
pub(in crate::runner) fn state_home_path() -> Option<PathBuf> {
    state_home()
}

/// The Effigy home that owns `~/.effigy/state`. Library tests in one process
/// share a private temporary home, never the operator's.
pub(super) fn state_home() -> Option<PathBuf> {
    if let Some(home) = STATE_HOME.get() {
        return Some(home.clone());
    }
    #[cfg(test)]
    {
        static TEST_HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
        // Short: a detached run's control socket lives under this home, and
        // a Unix socket path is bounded (SUN_LEN).
        let home = TEST_HOME.get_or_init(|| {
            tempfile::Builder::new()
                .prefix("effigy-te-")
                .tempdir_in("/tmp")
                .expect("private test home")
        });
        Some(home.path().to_path_buf())
    }
    #[cfg(not(test))]
    {
        StateRoot::home_from_env()
    }
}

/// The private home library tests share; a compiled `effigy` child a test
/// spawns gets it as `HOME` so its runs land where the test reads them.
#[cfg(test)]
pub(in crate::runner) fn test_state_home() -> PathBuf {
    state_home().expect("private test home")
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

/// A bounded pure read. A signal this process received does not cancel it:
/// a client that requested a stop keeps reading the run it is settling.
pub(super) fn observe() -> ObserveContext {
    reading_context(Deadline::At(std::time::Instant::now() + OBSERVE_BUDGET))
        .observe()
        .expect("observe context")
}

/// A Mutate context for an engine's effects, with the process's own cancel.
pub(in crate::runner) fn effect_context(reason: OpenReason) -> EffectContext {
    operation(EffectClass::Mutate, Deadline::Open(reason))
        .effects()
        .expect("mutate context has effects")
}

/// An Observe context bounded by `budget`, for an engine's probes.
pub(in crate::runner) fn observe_context(budget: Duration) -> ObserveContext {
    let observe = operation(
        EffectClass::Observe,
        Deadline::At(std::time::Instant::now() + budget),
    )
    .observe()
    .expect("observe context");
    match probe_runtime() {
        Some(runtime) => observe.with_probes(runtime),
        None => observe,
    }
}

/// This process's probe guard service (§5.11), installed on first use. When
/// it cannot be installed, probes observe `GuardUnavailable`; nothing is
/// guessed.
fn probe_runtime() -> Option<std::sync::Arc<effigy_kernel::probe::ProbeRuntime>> {
    static RUNTIME: OnceLock<Option<std::sync::Arc<effigy_kernel::probe::ProbeRuntime>>> =
        OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let executable = executable().ok()?;
            effigy_kernel::probe::ProbeRuntime::install(executable).ok()
        })
        .clone()
}

/// A cleanup context with its own `budget`, never a cancelled workload's.
pub(in crate::runner) fn cleanup_context(budget: Duration) -> CleanupContext {
    operation(EffectClass::Mutate, Deadline::Open(OpenReason::Shell))
        .cleanup(budget)
        .expect("cleanup budget")
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
/// Identifies the running binary, and holds its image, before any task can
/// rebuild it: a later run's helpers re-exec this image, not a replacement.
pub fn capture_startup_executable() {
    let _ = executable();
}

/// The binary identified at startup, for a supervisor this process starts.
pub(in crate::runner) fn startup_executable() -> Result<Executable, RunnerError> {
    executable()
}

/// A bounded read that a signal this process received does not cancel: a
/// front end keeps reading its run's events while the run settles a stop
/// that the signal requested.
pub(in crate::runner) fn reading_context(deadline: Deadline) -> OperationContext {
    OperationContext::new(
        deadline,
        CancelToken::default(),
        LIMITS,
        EffectClass::Observe,
        Placement::Host,
    )
}

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
static CUSTODY_WAITING: AtomicUsize = AtomicUsize::new(0);

/// Installs the one process signal router before the first custody spawn
/// (K9). Commands that never spawn under custody keep default dispositions.
/// Once installed, a signal that arrives while no custody wait is polling
/// ends the process as its default action would.
/// A command that owns the process's one router (source preparation) lends
/// it to the task engine, so a run inside that command never installs a
/// second one (K9). The owner keeps its own cancellation; no exit watcher.
pub fn adopt_signal_router(handle: RouterHandle) {
    let _ = ROUTER.set(Mutex::new(handle));
}

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
            if let Some(router) = ROUTER.get() {
                let mut router = router
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // Enter takes this same mutex: an idle observation cannot race
                // a run's admission or custody start. Exit while holding it so
                // no new custody scope can begin after this decision.
                if CUSTODY_WAITING.load(Ordering::Acquire) == 0 {
                    if let Some(signal) = router.poll().first() {
                        std::process::exit(128 + signal.received);
                    }
                }
            }
        })
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot start the signal watcher: {error}"))
        })?;
    Ok(())
}

/// Signals routed to this process since the last poll.
pub(in crate::runner) fn routed_signals() -> Vec<RoutedSignal> {
    poll_router()
}

/// The first signal that cancelled this process's root context, if any.
pub(in crate::runner) fn root_cancel_cause() -> Option<effigy_kernel::ctx::StopCause> {
    cancel_token().cause().cloned()
}

/// A front end's router (K9): INT, TERM and HUP become a stop request it
/// sends; the client never ends the run by dying quietly.
pub(in crate::runner) fn install_client_router() -> Result<(), RunnerError> {
    if cfg!(test) || ROUTER.get().is_some() {
        return Ok(());
    }
    let handle = SignalRouter::install(RouterPolicy::Client).map_err(|error| {
        RunnerError::task_invocation(format!("cannot install the signal router: {error}"))
    })?;
    let _ = ROUTER.set(Mutex::new(handle));
    Ok(())
}

pub(super) fn poll_router() -> Vec<RoutedSignal> {
    let routed = ROUTER
        .get()
        .map(|router| {
            router
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .poll()
        })
        .unwrap_or_default();
    if let Some(first) = routed.first() {
        let _ =
            INTERRUPTED_BY.compare_exchange(0, first.received, Ordering::AcqRel, Ordering::Acquire);
    }
    routed
}

static INTERRUPTED_BY: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// The first signal a custody wait consumed in this process, if any: the
/// run it interrupted records `cancelled` and Effigy exits 128 + n.
pub(in crate::runner) fn interrupted_by() -> Option<i32> {
    match INTERRUPTED_BY.load(Ordering::Acquire) {
        0 => None,
        signal => Some(signal),
    }
}

/// Marks a custody wait loop as the signal consumer while it lives.
#[derive(Debug)]
pub(in crate::runner) struct CustodyWait;
impl CustodyWait {
    pub(in crate::runner) fn enter() -> Self {
        let _router = ROUTER.get().map(|router| {
            router
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        });
        CUSTODY_WAITING.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for CustodyWait {
    fn drop(&mut self) {
        CUSTODY_WAITING.fetch_sub(1, Ordering::AcqRel);
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

/// Task runs record kernel effects (custody); managed runs add their own
/// engine effects, settled by the run supervisor's adapter. Any other effect
/// is no engine's here, so recovery observes it as `Unknown` and leaves
/// residue rather than guessing (R3).
#[derive(Debug, Default)]
pub(in crate::runner) struct TaskSettlement;
impl SettlementAdapter for TaskSettlement {
    fn observe(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Fence> {
        if crate::runner::managed_run::owns_effect(&effect.spec.kind) {
            return crate::runner::managed_run::observe_effect(effect, cx);
        }
        Observed::Unknown(UnknownReason::Inconsistent(
            "no engine adapter settles this effect".to_owned(),
        ))
    }
    fn release(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<()> {
        if crate::runner::managed_run::owns_effect(&effect.spec.kind) {
            return crate::runner::managed_run::release_effect(effect, cx);
        }
        Observed::Unknown(UnknownReason::Inconsistent(
            "no engine adapter settles this effect".to_owned(),
        ))
    }
    fn reconcile(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Reconciled> {
        if crate::runner::managed_run::owns_effect(&effect.spec.kind) {
            return crate::runner::managed_run::reconcile_effect(effect, cx);
        }
        Observed::Unknown(UnknownReason::Inconsistent(
            "no engine adapter settles this effect".to_owned(),
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
    let policy = match entry {
        "__kernel-sentinel" => RouterPolicy::Sentinel,
        "__kernel-supervise" => RouterPolicy::Supervisor,
        _ => RouterPolicy::Inline,
    };
    let router = match SignalRouter::install(policy) {
        Ok(router) => router,
        Err(_) => return Some(70),
    };
    if let Some(result) = effigy_kernel::probe::internal_entry(entry, router.root_cancel()) {
        return Some(i32::from(result.is_err()));
    }
    // One helper serves every engine's runs: a task run records its selector,
    // source preparation records `bundle sync`, the gateway records `gateway up`.
    // A managed run's detached supervisor is this binary's engine (§5.8).
    let mut adapter = TaskSettlement;
    let validate = |argv: &[OsString]| {
        valid_next(argv) || argv == ["bundle", "sync"] || argv == ["gateway", "up"]
    };
    let engine = |session, router| {
        adopt_signal_router(router);
        crate::runner::managed_run::supervise(session)
    };
    match effigy_kernel::supervise::engine_entry(entry, router, &mut adapter, validate, engine) {
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
