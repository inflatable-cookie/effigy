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
use effigy_kernel::location::CheckoutLocation;
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
        static TEST_HOME: OnceLock<PathBuf> = OnceLock::new();
        // Prefer a short home inside this run's scratch root. On macOS and
        // Linux the kernel also supports deep confined homes by binding and
        // probing control sockets relative to verified directories.
        Some(
            TEST_HOME
                .get_or_init(|| {
                    effigy_test_support::Fixture::process_lifetime_short("ef-te-")
                        .expect("private test home")
                })
                .clone(),
        )
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
pub(in crate::runner) fn probe_runtime(
) -> Option<std::sync::Arc<effigy_kernel::probe::ProbeRuntime>> {
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

/// One machine-wide gate per shared Compose service project (effigy#377).
///
/// A shared service belongs to no checkout, so two checkouts' `up`s are
/// serialised on the service's own Compose project name in the machine gate
/// namespace every checkout already uses. The caller decides what an
/// unacquirable gate means; this only reports it.
pub(in crate::runner) fn shared_service_gate(
    project_name: &str,
) -> Result<effigy_kernel::gate::GateGuard, RunnerError> {
    let home =
        state_home().ok_or_else(|| RunnerError::task_invocation("HOME is unset or relative"))?;
    let cx = effects();
    let root = StateRoot::runtime_machine_effects(&cx, &home)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let resource = effigy_kernel::id::ResourceId::store(
        effigy_kernel::id::Scope::Machine,
        &format!("shared-service {project_name}"),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    effigy_kernel::gate::Gate::acquire(
        &cx,
        &root,
        &[resource],
        effigy_kernel::gate::Wait::Until(
            std::time::Instant::now() + std::time::Duration::from_secs(SHARED_SERVICE_GATE_SECS),
        ),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

/// How long one `up` waits for another checkout to finish with a shared
/// project before it proceeds without the gate. A shared `up` pulls and
/// starts a data service, so the wait is generous.
const SHARED_SERVICE_GATE_SECS: u64 = 120;

static CHECKOUT_ROOTS: OnceLock<Mutex<std::collections::BTreeMap<PathBuf, &'static StateRoot>>> =
    OnceLock::new();

#[derive(Debug)]
pub(super) struct RootsView {
    pub roots: Vec<StateRoot>,
    pub omitted: Vec<String>,
}

pub(super) fn known_roots_view(cx: &ObserveContext) -> Result<RootsView, RunnerError> {
    let mut paths = CHECKOUT_ROOTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let cwd = crate::runner::command_context::current_working_dir()?;
    let current = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
        .map(|resolved| resolved.resolved_root)
        .unwrap_or(cwd);
    let home =
        state_home().ok_or_else(|| RunnerError::task_invocation("HOME is unset or relative"))?;
    let config = effigy_manifest::load_user_config_from(&home.join(".effigy/config.toml"))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let found = super::discovery::collect(cx, &current, &config.runtime.checkout_roots);
    let mut omitted = found
        .omitted
        .iter()
        .map(|entry| format!("{}: {}", entry.path.display(), entry.reason))
        .collect::<Vec<_>>();
    paths.extend(found.checkouts);
    let mut roots = Vec::new();
    for path in paths {
        match observe_root(cx, &path) {
            Observed::Verified(root, _) => roots.push(root),
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => omitted.push(format!("{}: {reason:?}", path.display())),
        }
    }
    match StateRoot::runtime_machine_observe(cx, &home) {
        Observed::Verified(root, _) => roots.push(root),
        Observed::Absent(_) => {}
        Observed::Unknown(reason) => {
            omitted.push(format!("machine facts: {reason:?}"));
        }
    }
    Ok(RootsView { roots, omitted })
}

/// The kernel location of the checkout an invocation root belongs to.
///
/// Durable checkout state belongs to the resolved invocation root, not a
/// nested catalog's cwd (contract 052 §7.1): Git discovery is lexical and
/// upward, so a nested directory with no `.git` of its own declares no second
/// checkout and its state belongs to the nearest enclosing one.
pub(in crate::runner) fn checkout_location(root: &Path) -> CheckoutLocation {
    CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(root),
    }
}

/// Checkout journals are opened independently; the actual user HOME is only
/// the migration source and shared machine-store location.
pub(in crate::runner) fn effects_root(checkout: &Path) -> Result<&'static StateRoot, RunnerError> {
    let checkout = effigy_kernel::fs::DirFd::open(checkout)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let home =
        state_home().ok_or_else(|| RunnerError::task_invocation("HOME is unset or relative"))?;
    let location = checkout_location(checkout.path());
    // Revalidate before reusing a retained handle: same-path replacement is
    // never permission to write through the previous checkout's descriptor.
    let current = StateRoot::checkout_effects(&effects(), &home, &location)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut roots = CHECKOUT_ROOTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Keyed by the checkout the root belongs to, so every nested catalog in one
    // checkout reuses that checkout's single retained handle.
    if let Some(root) = roots.get(&location.root) {
        let identity = |dir: &effigy_kernel::fs::DirFd| {
            dir.identity()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        };
        if identity(root.owning_directory())? != identity(current.owning_directory())?
            || identity(root.state_directory())? != identity(current.state_directory())?
        {
            return Err(RunnerError::task_invocation(
                "checkout state directory was replaced during this invocation",
            ));
        }
        return Ok(root);
    }
    let root = Box::leak(Box::new(current));
    roots.insert(location.root.clone(), root);
    Ok(root)
}

/// A read-only checkout view never creates state, falls back to another
/// checkout or opens the legacy machine journal.
pub(super) fn observe_root(cx: &ObserveContext, checkout: &Path) -> Observed<StateRoot> {
    let location = checkout_location(checkout);
    match state_home() {
        Some(home) => StateRoot::checkout_observe_in(cx, &home, &location),
        None => StateRoot::checkout_observe(cx, &location),
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

/// Task runs record kernel effects: local custody and remote custody.
/// Remote sessions settle through the Compose backend their identity names,
/// also in sentinel recovery. Managed runs add their own engine effects,
/// settled by the run supervisor's adapter. Any other effect is no engine's
/// here, so recovery observes it as `Unknown` and leaves residue (R3).
#[derive(Default)]
pub(in crate::runner) struct TaskSettlement {
    remote: Option<Box<dyn effigy_kernel::remote::RemoteExecAdapter>>,
}
impl std::fmt::Debug for TaskSettlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TaskSettlement")
    }
}
#[cfg(test)]
type RemoteFactory = std::rc::Rc<dyn Fn() -> Box<dyn effigy_kernel::remote::RemoteExecAdapter>>;
#[cfg(test)]
thread_local! {
    /// A test's strict remote model, used by every settle on its thread.
    pub(in crate::runner) static TEST_REMOTE: std::cell::RefCell<Option<RemoteFactory>> =
        const { std::cell::RefCell::new(None) };
}
fn recovery_adapter() -> Box<dyn effigy_kernel::remote::RemoteExecAdapter> {
    #[cfg(test)]
    if let Some(factory) = TEST_REMOTE.with(|factory| factory.borrow().clone()) {
        return factory();
    }
    Box::new(effigy_containers::environment::ComposeRemote::<
        effigy_containers::environment::HostCli,
    >::recovery())
}
impl SettlementAdapter for TaskSettlement {
    fn remote(&mut self) -> Option<&mut dyn effigy_kernel::remote::RemoteExecAdapter> {
        Some(self.remote.get_or_insert_with(recovery_adapter).as_mut())
    }
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
    // The workspace-serving process-lifetime proof's initiating front end: it
    // starts one detached serving host, acknowledges the kernel detach and
    // returns, leaving the host serving. Present only in a fixture build.
    #[cfg(feature = "workspace-serving-fixture")]
    if entry == "__workspace-serving-fixture" {
        return Some(
            match crate::runner::workspace_serving_fixture::initiate_from_stdin() {
                Ok(result) => {
                    println!("{result}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            },
        );
    }
    #[cfg(feature = "workspace-serving-fixture")]
    if entry == "__workspace-serving-fixture-abandon" {
        return Some(
            match crate::runner::workspace_serving_fixture::initiate_abandoning_from_stdin() {
                Ok(result) => {
                    println!("{result}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            },
        );
    }
    #[cfg(feature = "workspace-serving-fixture")]
    if entry == "__workspace-serving-fixture-deadline" {
        return Some(
            match crate::runner::workspace_serving_fixture::initiate_deadline_from_stdin() {
                Ok(result) => {
                    println!("{result}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            },
        );
    }
    #[cfg(feature = "workspace-serving-fixture")]
    if entry == "__workspace-serving-fixture-crash" {
        return Some(
            match crate::runner::workspace_serving_fixture::initiate_then_crash_stdin() {
                Ok(_) => 0,
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            },
        );
    }
    #[cfg(feature = "workspace-serving-fixture")]
    if entry == "__workspace-serving-fixture-compete" {
        return Some(
            match crate::runner::workspace_serving_fixture::compete_stdin() {
                Ok(result) => {
                    println!("{result}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            },
        );
    }
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
    let mut adapter = TaskSettlement::default();
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
