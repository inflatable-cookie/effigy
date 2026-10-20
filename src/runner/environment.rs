//! The runner's handle on the environment reconciler (contract 053).
//!
//! Every container lifecycle command reaches the engine here. Bring-up runs
//! as a reconcile run and an activation run, every stop is one transition,
//! and a command that uses an environment holds it through a session run
//! (or, for a managed run, through its own task run). The first Mutate of an
//! environment imports its v0.14.1 markers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_containers::environment::{
    self as engine, Applied, BackendError, BackendSelection, EnvContext, EnvError, EnvRecord,
    EnvRef, HoldError, Idle, ImportRequest, LegacyMarkers, Operation, Ownership, Pending,
    Retention, Session, Up,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::CancelToken;
use effigy_kernel::gate::Wait;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::AttachMode;

use crate::runner::error::RunnerError;

/// A managed run's task-run id: the bring-up it started attaches it.
pub(in crate::runner) const HOLDER_ENV: &str = "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER";
/// The settling run whose own stop this is (R4's one exemption).
pub(in crate::runner) const EXEMPT_ENV: &str = "EFFIGY_INTERNAL_ENVIRONMENT_EXEMPT";
/// Opts task-started environments out of holder-based idle stop.
const KEEP_RUNNING_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE";
/// The longest a first Mutate waits for a v0.14.1 idle-stop lease to end.
const LEGACY_LEASE_WAIT: Duration = Duration::from_secs(600);
const INVENTORY_BUDGET: Duration = Duration::from_secs(15);

pub(in crate::runner) fn context() -> Result<EnvContext<'static>, RunnerError> {
    if inside_guest() {
        return Err(error(EnvError::HostAuthorityRequired));
    }
    EnvContext::new(root()?, CancelToken::default()).map_err(error)
}

/// The state root beside the render objects (`$HOME/.effigy/state`, the
/// task engine's too). One root per home is opened once per process.
fn root() -> Result<&'static effigy_kernel::root::StateRoot, RunnerError> {
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};
    static ROOTS: OnceLock<Mutex<BTreeMap<PathBuf, &'static effigy_kernel::root::StateRoot>>> =
        OnceLock::new();
    let home = engine::state_home().map_err(error)?;
    let mut roots = ROOTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(root) = roots.get(&home) {
        return Ok(root);
    }
    let operation = effigy_kernel::ctx::OperationContext::new(
        effigy_kernel::ctx::Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        effigy_kernel::ctx::Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        effigy_kernel::ctx::EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    );
    let cx = operation
        .effects()
        .map_err(|_| RunnerError::task_invocation("Mutate capability refused"))?;
    let root: &'static _ = Box::leak(Box::new(engine::state_root(&cx).map_err(error)?));
    roots.insert(home, root);
    Ok(root)
}

fn inside_guest() -> bool {
    effigy_core::container_detection::process_is_inside_effigy_workspace_container()
}

pub(in crate::runner) fn error(error: EnvError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

fn runtime_error(error: RunnerError) -> effigy_runtime::EffigyRuntimeError {
    effigy_runtime::EffigyRuntimeError::task_invocation(error.to_string())
}

pub(in crate::runner) fn env_ref(policy: &EffectiveContainerPolicy) -> Result<EnvRef, RunnerError> {
    effigy_core::worktree_scope::prepare_checkout_token(&policy.repo_root).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
    })?;
    EnvRef::for_policy(policy).map_err(error)
}

/// The environment's record, read without effects; `None` when there is
/// no record yet or it cannot be read.
pub(in crate::runner) fn record(policy: &EffectiveContainerPolicy) -> Option<EnvRecord> {
    let env = EnvRef::for_policy(policy).ok()?;
    let ctx = context().ok()?;
    match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => Some(record),
        _ => None,
    }
}

/// The exact backend: the record's, unless this invocation names one.
pub(in crate::runner) fn backend(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> BackendSelection {
    let recorded = record(policy).and_then(|record| record.recorded_backend().cloned());
    engine::effective_backend(repo_root, policy, recorded.as_ref())
}

/// Where v0.14.1 kept this environment's lifecycle markers.
pub(in crate::runner) fn legacy_marker_dir(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> PathBuf {
    let base = legacy_marker_home()
        .unwrap_or_else(|| repo_root.join(".effigy/runtime/host-container-leases"));
    base.join(engine::sanitize_marker_component(&policy.profile))
        .join(engine::sanitize_marker_component(&policy.project_name))
}

fn legacy_marker_home() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME") {
        return Some(PathBuf::from(path).join("runtime/host-container-leases"));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".effigy/runtime/host-container-leases"))
}

/// Imports v0.14.1 markers and labels once, before the first transition.
/// Running containers are adopted only on proven ownership, never restarted.
pub(in crate::runner) fn import_legacy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if record(policy).is_some_and(|record| record.engine.is_some()) {
        return Ok(());
    }
    let markers = LegacyMarkers::read(&legacy_marker_dir(repo_root, policy), &policy.name);
    if markers.is_empty() {
        return Ok(());
    }
    if let Some(remaining) = markers.lease_pending(now_ms()) {
        if remaining > LEGACY_LEASE_WAIT {
            return Err(RunnerError::task_invocation(format!(
                "environment `{}` still has a v0.14.1 idle-stop lease for {}s; its reaper may stop the stack. Retry after it expires",
                policy.name,
                remaining.as_secs()
            )));
        }
        eprintln!(
            "[info] waiting {}s for the v0.14.1 idle-stop lease of `{}` to end before adopting it",
            remaining.as_secs().max(1),
            policy.name
        );
        std::thread::sleep(remaining + Duration::from_secs(1));
    }
    let env = env_ref(policy)?;
    let ctx = context()?;
    let backend = backend(repo_root, policy);
    let workloads = backend.adapter().inventory(repo_root, &policy.project_name);
    let ownership = Ownership {
        checkout: repo_root.canonicalize().map_err(|error| {
            RunnerError::task_invocation(format!("cannot resolve the checkout: {error}"))
        })?,
        scope: effigy_core::worktree_scope::load(repo_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
    };
    engine::import(
        &ctx,
        ImportRequest {
            env: &env,
            project: &policy.project_name,
            markers: &markers,
            workloads,
            ownership: &ownership,
            primary_service: &policy.primary_service,
            backend: &backend,
        },
    )
    .map(|_| ())
    .map_err(error)
}

/// Whether an imported v0.14.1 partial generation is these inputs.
pub(in crate::runner) fn adopts_legacy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    record: &EnvRecord,
) -> bool {
    adopt(repo_root, policy, Some(record))
}

/// Whether the imported, never-applied running stack matches the current
/// inputs exactly as its v0.14.1 admission recorded them (M7).
fn adopt(repo_root: &Path, policy: &EffectiveContainerPolicy, record: Option<&EnvRecord>) -> bool {
    let Some(record) = record else {
        return false;
    };
    if record.last_applied.is_some()
        || !record
            .engine()
            .is_some_and(|engine| engine.imported.is_some())
    {
        return false;
    }
    let markers = LegacyMarkers::read(&legacy_marker_dir(repo_root, policy), &policy.name);
    let Some(admitted) = engine::recorded_admission(&markers) else {
        return false;
    };
    let current = effigy_containers::policy::admission::generation_context(policy);
    match (
        effigy_containers::policy::admission::normalize_context(admitted.clone(), policy),
        current,
    ) {
        (Ok(admitted), Ok(current)) => admitted == current,
        _ => false,
    }
}

/// Whether every declared service of the environment's project runs, from
/// an exact-identity inventory. Containers this checkout cannot prove it
/// owns refuse, so another checkout's stack is never recreated.
pub(in crate::runner) fn running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Observed<bool>, RunnerError> {
    let deadline = Instant::now() + INVENTORY_BUDGET;
    let rows = match effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    ) {
        Ok(rows) => rows,
        Err(error) => return Ok(unknown(error)),
    };
    let rows = rows
        .into_iter()
        .filter(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str()) && !row.oneoff
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Ok(Observed::Verified(false, evidence()));
    }
    effigy_containers::exec::running_stack_admits_reactivation(repo_root, policy, &rows, deadline)
        .map_err(|reason| {
            RunnerError::task_invocation(format!(
                "container `{}` cannot be re-activated: {reason}. Nothing was started, recreated or published. If these containers belong to this checkout, run `effigy container down`, then `effigy container up --detach`; otherwise stop the other checkout's stack first",
                policy.name
            ))
        })?;
    let live = rows
        .into_iter()
        .filter(|row| effigy_containers::exec::compose_status_is_running(&row.status))
        .collect::<Vec<_>>();
    if live.is_empty() {
        return Ok(Observed::Verified(false, evidence()));
    }
    // Every row's ownership, including the pre-label bind proof, was
    // verified by the reactivation check above.
    Ok(
        match effigy_containers::exec::compose_stack_is_running(policy, &live) {
            Ok(running) => Observed::Verified(running, evidence()),
            Err(error) => unknown(error),
        },
    )
}

fn unknown(error: impl std::fmt::Display) -> Observed<bool> {
    Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
        error.to_string(),
    ))
}

fn evidence() -> effigy_kernel::observe::Evidence {
    effigy_kernel::observe::Evidence::new("environment inventory".into(), None)
        .expect("fixed evidence")
}

/// A reconciled generation, before the command's integrations activate it.
#[derive(Debug)]
pub(in crate::runner) struct BringUp {
    env: EnvRef,
    pending: Pending,
}

impl BringUp {
    pub(in crate::runner) fn applied(&self) -> Applied {
        self.pending.applied
    }
}

/// The retention a task's ensure gives a stack it starts.
pub(in crate::runner) fn task_retention() -> Retention {
    if std::env::var(KEEP_RUNNING_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
    {
        Retention::Persistent
    } else {
        Retention::Held
    }
}

/// The reconcile run of a bring-up. `apply` performs the backend effect
/// only when the running stack is not already the desired generation.
pub(in crate::runner) fn reconcile(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    retention: Retention,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<BringUp, RunnerError> {
    import_legacy(repo_root, policy)?;
    let env = env_ref(policy)?;
    let ctx = context()?;
    let existing = record(policy);
    let running = running(repo_root, policy)?;
    let drift = match running {
        Observed::Verified(true, _) => effigy_containers::exec::probe_mount_drift(
            repo_root,
            policy,
            Instant::now() + INVENTORY_BUDGET,
        )
        .unwrap_or_default(),
        _ => Vec::new(),
    };
    let pending = engine::reconcile(
        &ctx,
        &env,
        Up {
            render: policy.render.as_ref().map(|render| &render.closure),
            project: &policy.project_name,
            backend: &backend(repo_root, policy),
            retention,
            running,
            adopt: adopt(repo_root, policy, existing.as_ref()),
            drifted: !drift.is_empty(),
        },
        apply,
    )
    .map_err(error)?;
    if pending.applied == Applied::KeptForHolders {
        eprintln!(
            "[warn] environment `{}` keeps its running generation: live runs hold it, so the changed configuration is applied after they end",
            policy.name
        );
        for detail in &drift {
            eprintln!("[warn] {detail}");
        }
    }
    Ok(BringUp { env, pending })
}

/// The activation run: publishes the generation and attaches `holders`.
pub(in crate::runner) fn activate(
    bring_up: &BringUp,
    holders: &[RunId],
) -> Result<(), RunnerError> {
    let ctx = context()?;
    let mode = record_mode(&ctx, &bring_up.env);
    let holders = holders
        .iter()
        .chain(requested_holder().iter())
        .map(|run| (*run, mode))
        .collect::<Vec<_>>();
    engine::activate(&ctx, &bring_up.env, &bring_up.pending, &holders).map_err(error)
}

fn record_mode(ctx: &EnvContext<'_>, env: &EnvRef) -> AttachMode {
    match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => engine::mode_for(&record),
        _ => AttachMode::Borrow,
    }
}

fn requested_holder() -> Option<RunId> {
    std::env::var(HOLDER_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

fn exempt_run() -> Option<RunId> {
    std::env::var(EXEMPT_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

/// One stop transition. The calling run's own stop is exempt once that run
/// is settling (`EFFIGY_INTERNAL_ENVIRONMENT_EXEMPT`); every other holder
/// refuses it (R4). `effect` is the host teardown plus the backend stop.
pub(in crate::runner) fn stop(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    op: Operation,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    import_legacy(repo_root, policy)?;
    let env = env_ref(policy)?;
    stop_env(
        &env,
        &backend(repo_root, policy),
        &policy.project_name,
        op,
        effect,
    )
}

/// A stop of a same-checkout peer known only by name and project, such as a
/// `data_from` peer whose own policy cannot be resolved for a whole handover.
pub(in crate::runner) fn stop_peer(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    peer: &str,
    project: &str,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = EnvRef::new(env_ref(policy)?.checkout, peer).map_err(error)?;
    stop_env(
        &env,
        &backend(repo_root, policy),
        project,
        Operation::Handover,
        effect,
    )
}

fn stop_env(
    env: &EnvRef,
    backend: &BackendSelection,
    project: &str,
    op: Operation,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let ctx = context()?;
    let mut failure = None;
    let result = engine::down(&ctx, env, op, backend, project, exempt_run(), || {
        effect().map_err(|error| {
            let message = error.to_string();
            failure = Some(error);
            BackendError::Uncertain(message)
        })
    });
    match (result, failure) {
        (Ok(()), _) => Ok(()),
        (Err(EnvError::Backend(_)), Some(failure)) => Err(failure),
        (Err(error_value), _) => Err(error(error_value)),
    }
}

/// A restart of a service that stopped answering, as one transition: it
/// refuses while any run holds the environment (R4, probes never repair
/// a held environment).
pub(in crate::runner) fn repair(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context()?;
    let mut failure = None;
    let result = engine::repair(
        &ctx,
        &env,
        &backend(repo_root, policy),
        &policy.project_name,
        || {
            effect().map_err(|error| {
                let message = error.to_string();
                failure = Some(error);
                BackendError::Uncertain(message)
            })
        },
    );
    match (result, failure) {
        (Ok(()), _) => Ok(()),
        (Err(EnvError::Backend(_)), Some(failure)) => Err(failure),
        (Err(error_value), _) => Err(error(error_value)),
    }
}

/// One reset transition: the stop plus dropping this environment's render
/// references.
pub(in crate::runner) fn reset(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    import_legacy(repo_root, policy)?;
    let env = env_ref(policy)?;
    let ctx = context()?;
    let backend = backend(repo_root, policy);
    let mut failure = None;
    let result = engine::reset(&ctx, &env, &backend, &policy.project_name, || {
        effect().map_err(|error| {
            let message = error.to_string();
            failure = Some(error);
            BackendError::Uncertain(message)
        })
    });
    match (result, failure) {
        (Ok(()), _) => Ok(()),
        (Err(EnvError::Backend(_)), Some(failure)) => Err(failure),
        (Err(error_value), _) => Err(error(error_value)),
    }
}

/// The runtime crate's shutdown hook as one stop transition, after the
/// environment's host supervisors stop (they must not race the backend).
pub(in crate::runner) fn stop_hook(
    op: Operation,
) -> impl Fn(
    &Path,
    &EffectiveContainerPolicy,
    effigy_runtime::write::ShutdownEffect<'_>,
) -> Result<(), effigy_runtime::EffigyRuntimeError> {
    move |repo_root, policy, effect| {
        stop(repo_root, policy, op, || {
            let _ =
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy);
            effect().map_err(RunnerError::from)
        })
        .map_err(runtime_error)
    }
}

/// The runtime crate's reset hook as one reset transition.
pub(in crate::runner) fn reset_hook() -> impl Fn(
    &Path,
    &EffectiveContainerPolicy,
    effigy_runtime::write::ShutdownEffect<'_>,
) -> Result<(), effigy_runtime::EffigyRuntimeError> {
    move |repo_root, policy, effect| {
        reset(repo_root, policy, || {
            let _ =
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy);
            effect().map_err(RunnerError::from)
        })
        .map_err(runtime_error)
    }
}

/// A command's hold on its environment for as long as it uses it. Dropping
/// it settles the session, then applies holder-based idle stop.
#[derive(Debug)]
pub(in crate::runner) struct Held {
    session: Option<Session<'static>>,
    repo_root: PathBuf,
    policy: EffectiveContainerPolicy,
}

impl Held {
    /// Begins the session before any bring-up, so the bring-up's activation
    /// can attach it; outside a host or for an unrecordable checkout there
    /// is no session and the command proceeds unheld.
    pub(in crate::runner) fn begin(
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        label: &str,
    ) -> Option<Self> {
        if inside_guest() {
            return None;
        }
        // Prepare already minted the checkout identity; a hold never does.
        let env = EnvRef::for_policy(policy).ok()?;
        let ctx = context().ok()?;
        let session = Session::begin(&ctx, &env, label).ok()?;
        Some(Self {
            session: Some(session),
            repo_root: repo_root.to_path_buf(),
            policy: policy.clone(),
        })
    }

    pub(in crate::runner) fn id(&self) -> Option<RunId> {
        self.session.as_ref().map(Session::id)
    }

    /// Attaches the session to an environment that is already active.
    /// Returns false when it is not active: the caller ensures it first.
    pub(in crate::runner) fn attach_active(&mut self) -> Result<bool, RunnerError> {
        let Some(session) = self.session.as_mut() else {
            return Ok(true);
        };
        let env = env_ref(&self.policy)?;
        let ctx = context()?;
        match engine::hold(
            session.run_mut(),
            &ctx,
            &env,
            Wait::Until(Instant::now() + engine::GATE_WAIT),
        ) {
            Ok(_) => Ok(true),
            Err(HoldError::NotActive) => Ok(false),
            Err(other) => Err(RunnerError::task_invocation(other.to_string())),
        }
    }

    /// Whether this session now holds the environment.
    pub(in crate::runner) fn holds(&self) -> bool {
        let (Some(id), Some(record)) = (self.id(), record(&self.policy)) else {
            return false;
        };
        matches!(record.holders, Observed::Verified(ref holders, _) if holders.iter().any(|holder| holder.run == id))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.close();
            release(&self.repo_root, &self.policy);
        }
    }
}

/// Holder-based idle stop after a run released its hold. Never fails the
/// command: an idle stop that cannot run leaves the stack running.
pub(in crate::runner) fn release(repo_root: &Path, policy: &EffectiveContainerPolicy) {
    let (Ok(env), Ok(ctx)) = (env_ref(policy), context()) else {
        return;
    };
    let backend = backend(repo_root, policy);
    let outcome = engine::reap_if_idle(&ctx, &env, |run| {
        run.stop_with(&backend, &policy.project_name, None, || {
            effigy_containers::exec::shutdown_container(repo_root, policy)
                .map_err(|error| BackendError::Uncertain(error.to_string()))
        })
    });
    match outcome {
        Ok(Idle::Stopped) => eprintln!(
            "[info] stopped environment `{}`: the last run using it ended. `effigy container up --detach` keeps an environment running",
            policy.name
        ),
        Ok(Idle::Kept(_)) => {}
        Err(error) => eprintln!(
            "[warn] environment `{}` was left running: idle stop failed ({error})",
            policy.name
        ),
    }
}

/// The notice for a stack a command started and holds.
pub(in crate::runner) fn emit_held_notice(container_name: &str) {
    eprintln!(
        "[info] environment `{container_name}` was started for this run; it stops when the last run using it ends. Run `effigy container up --detach` to keep it running, or set {KEEP_RUNNING_ENV}=1"
    );
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
