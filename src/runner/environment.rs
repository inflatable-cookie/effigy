//! The runner's handle on the environment reconciler (contract 053).
//!
//! Every container lifecycle command reaches the engine here. Bring-up runs
//! as a reconcile run and an activation run, every stop is one transition,
//! and a command that uses an environment holds it through a session run
//! (or, for a managed run, through its own task run).

#[path = "environment_workspace.rs"]
pub(in crate::runner) mod workspace;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::environment::{
    self as engine, Applied, BackendError, BackendSelection, EnvContext, EnvError, EnvRecord,
    EnvRef, HoldError, Idle, Operation, Retention, Session, Up,
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
/// Opts task-started environments out of holder-based idle stop.
const KEEP_RUNNING_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE";
const INVENTORY_BUDGET: Duration = Duration::from_secs(15);

pub(in crate::runner) fn context(repo_root: &Path) -> Result<EnvContext<'static>, RunnerError> {
    if inside_guest() {
        return Err(error(EnvError::HostAuthorityRequired));
    }
    EnvContext::new(root(repo_root)?, CancelToken::default()).map_err(error)
}

/// Environment records and render objects share the owning checkout with
/// task holders. Retained handles are revalidated before each use.
fn root(repo_root: &Path) -> Result<&'static effigy_kernel::root::StateRoot, RunnerError> {
    if inside_guest() {
        return Err(error(EnvError::HostAuthorityRequired));
    }
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};
    static ROOTS: OnceLock<Mutex<BTreeMap<PathBuf, &'static effigy_kernel::root::StateRoot>>> =
        OnceLock::new();
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
    let current = engine::state_root(&cx, repo_root).map_err(error)?;
    let mut roots = ROOTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = current.owning_directory().path().to_owned();
    if let Some(root) = roots.get(&key) {
        let identity = |dir: &effigy_kernel::fs::DirFd| {
            dir.identity()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        };
        if identity(root.owning_directory())? != identity(current.owning_directory())?
            || identity(root.state_directory())? != identity(current.state_directory())?
        {
            return Err(RunnerError::task_invocation(
                "environment checkout was replaced during this invocation",
            ));
        }
        return Ok(root);
    }
    let root = Box::leak(Box::new(current));
    roots.insert(key, root);
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
    root(&policy.repo_root)?;
    effigy_core::worktree_scope::prepare_checkout_token(&policy.repo_root).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
    })?;
    EnvRef::for_policy(policy).map_err(error)
}

/// The environment's record, read without effects; `None` when there is
/// no record yet or it cannot be read.
pub(in crate::runner) fn record(policy: &EffectiveContainerPolicy) -> Option<EnvRecord> {
    let env = EnvRef::for_policy(policy).ok()?;
    let cx = super::task_engine::observe_context(Duration::from_secs(10));
    let observed = engine::observe_root(&cx, &policy.repo_root);
    let root = observed.verified()?;
    match EnvRecord::read(&cx, root, &env.resource) {
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

/// Whether every declared service of the environment's project runs, from one
/// labelled `ps`. A project this checkout does not own is another checkout's:
/// its containers carry that checkout's labels, so its rows never answer for
/// this project.
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
    let live = rows
        .into_iter()
        .filter(|row| effigy_containers::exec::compose_status_is_running(&row.status))
        .collect::<Vec<_>>();
    if live.is_empty() {
        return Ok(Observed::Verified(false, evidence()));
    }
    // One labelled `ps` answers both questions: the rows are this project's
    // containers, and a row that runs is part of the stack.
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

/// A bring-up whose backend apply finished, before the command's
/// integrations activate it.
#[derive(Debug)]
pub(in crate::runner) struct BringUp {
    env: EnvRef,
    repo_root: PathBuf,
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

/// The apply run of a bring-up. `apply` performs the one backend effect:
/// `compose up -d --remove-orphans`, from which Compose recreates only the
/// services whose rendered configuration changed.
pub(in crate::runner) fn reconcile(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    retention: Retention,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<BringUp, RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context(&policy.repo_root)?;
    // The exact-identity inventory still removes owned containers a profile
    // transition left unusable, so the apply recreates rather than collides.
    let _running = running(repo_root, policy)?;
    // A bind-mounted directory that does not exist yet cannot be mounted.
    effigy_containers::exec::ensure_isolated_mount_host_points(repo_root, policy).map_err(
        |error| {
            RunnerError::task_invocation(format!(
                "container `{}` could not prepare its isolated mount points in the checkout: {error}",
                policy.name
            ))
        },
    )?;
    let applied = engine::reconcile(
        &ctx,
        &env,
        Up {
            render: policy.render.as_ref().map(|render| &render.closure),
            project: &policy.project_name,
            backend: &backend(repo_root, policy),
            retention,
        },
        apply,
    )
    .map_err(error)?;
    match &applied {
        Applied::HeldByRuns(holders) => eprintln!(
            "[warn] environment `{}` is held by {}; the edit applies on the next up. The runs using it stay attached",
            policy.name,
            holders
                .iter()
                .map(|run| format!("run {run}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Applied::ReconciledUnreadableHolders(reason) => eprintln!(
            "[warn] environment `{}` holder list could not be read ({reason}); the edit was applied without checking for runs that hold it",
            policy.name
        ),
        Applied::Reconciled => {}
    }
    Ok(BringUp {
        env,
        repo_root: policy.repo_root.clone(),
    })
}

/// The activation run: publishes the generation and attaches `holders`.
pub(in crate::runner) fn activate(
    bring_up: &BringUp,
    policy: &EffectiveContainerPolicy,
    holders: &[RunId],
) -> Result<(), RunnerError> {
    let ctx = context(&bring_up.repo_root)?;
    let mode = record_mode(&ctx, &bring_up.env);
    let holders = holders
        .iter()
        .chain(requested_holder().iter())
        .map(|run| (*run, mode))
        .collect::<Vec<_>>();
    engine::activate_with(&ctx, &bring_up.env, &holders, |run| {
        super::workspace_command::publish_workspace_guest_in(&bring_up.repo_root, policy, run)
            .map_err(|error| EnvError::Invalid(error.to_string()))
    })
    .map_err(error)
}

/// A command that changes part of a running environment in place - a
/// host-process restart - holds `Gate(Env)` for the change, exactly like a
/// bring-up, so `up` and `down` cannot interleave with it (effigy#367).
///
/// The change is not a backend transition: it publishes nothing and touches no
/// render. A run that changed nothing republishes the state it found.
pub(in crate::runner) fn holding_environment<T>(
    policy: &EffectiveContainerPolicy,
    op: Operation,
    change: impl FnOnce() -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    let env = env_ref(policy)?;
    // An environment nothing has brought up yet has no record to gate and no
    // transition to interleave with, so the change runs unheld (effigy#370).
    if record(policy).is_none() {
        return change();
    }
    let ctx = context(&policy.repo_root)?;
    let run = engine::EnvRun::begin(
        &ctx,
        &env,
        op,
        Wait::Until(Instant::now() + engine::GATE_WAIT),
    )
    .map_err(error)?;
    let result = change();
    run.finish(result.is_ok()).map_err(error)?;
    result
}

fn record_mode(ctx: &EnvContext<'_>, env: &EnvRef) -> AttachMode {
    match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => engine::mode_for(&record),
        _ => AttachMode::Borrow,
    }
}

/// The run a managed run's `container up` was asked to attach, if any.
pub(in crate::runner) fn requested_holder() -> Option<RunId> {
    std::env::var(HOLDER_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

/// One stop transition. A run settling in this process exempts its own hold by
/// id ([`stop_exempting`]); every other holder refuses it (R4). `effect` is the
/// host teardown plus the backend stop.
pub(in crate::runner) fn stop(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    op: Operation,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    stop_exempting(repo_root, policy, op, None, effect)
}

/// [`stop`] for a run settling in this process: its own hold is exempt by id.
pub(in crate::runner) fn stop_exempting(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    op: Operation,
    exempt: Option<RunId>,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = env_ref(policy)?;
    stop_env(
        &policy.repo_root,
        &env,
        &backend(repo_root, policy),
        &policy.project_name,
        op,
        exempt,
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
        &policy.repo_root,
        &env,
        &backend(repo_root, policy),
        project,
        Operation::Handover,
        None,
        effect,
    )
}

fn stop_env(
    repo_root: &Path,
    env: &EnvRef,
    backend: &BackendSelection,
    project: &str,
    op: Operation,
    exempt: Option<RunId>,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let ctx = context(repo_root)?;
    let mut failure = None;
    let result = engine::down(&ctx, env, op, backend, project, exempt, || {
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

/// One reset transition: the stop plus dropping this environment's render
/// references.
pub(in crate::runner) fn reset(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    effect: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context(&policy.repo_root)?;
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
    stop_hook_exempting(op, None)
}

/// [`stop_hook`] exempting `exempt`, a run settling in this process.
pub(in crate::runner) fn stop_hook_exempting(
    op: Operation,
    exempt: Option<RunId>,
) -> impl Fn(
    &Path,
    &EffectiveContainerPolicy,
    effigy_runtime::write::ShutdownEffect<'_>,
) -> Result<(), effigy_runtime::EffigyRuntimeError> {
    move |repo_root, policy, effect| {
        stop_exempting(repo_root, policy, op, exempt, || {
            report_host_process_stop(
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy),
            );
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
            report_host_process_stop(
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy),
            );
            effect().map_err(RunnerError::from)
        })
        .map_err(runtime_error)
    }
}

/// A stop report is never dropped: every stopped process and every residue
/// line reaches the operator, on stderr so a JSON envelope stays parseable.
fn report_host_process_stop(report: crate::runner::host_process::HostProcessStopReport) {
    for line in report.lines() {
        eprintln!("{line}");
    }
    if !report.complete() {
        eprintln!(
            "[host process] environment `{}` did not confirm every host-process stop; `effigy container status` shows each run",
            report.environment
        );
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
    /// can attach it. A failed local journal refuses the operation before
    /// backend effects; guest work continues through host authority.
    pub(in crate::runner) fn begin(
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        label: &str,
    ) -> Result<Option<Self>, RunnerError> {
        if inside_guest() {
            return Ok(None);
        }
        let env = env_ref(policy)?;
        let ctx = context(&policy.repo_root)?;
        let session = Session::begin(&ctx, &env, label).map_err(error)?;
        Ok(Some(Self {
            session: Some(session),
            repo_root: repo_root.to_path_buf(),
            policy: policy.clone(),
        }))
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
        let ctx = context(&self.policy.repo_root)?;
        let running = matches!(
            running(&self.policy.repo_root, &self.policy),
            Ok(Observed::Verified(true, _))
        );
        match engine::hold(
            session.run_mut(),
            &ctx,
            &env,
            Wait::Until(Instant::now() + engine::GATE_WAIT),
            running,
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
    let (Ok(env), Ok(ctx)) = (env_ref(policy), context(&policy.repo_root)) else {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn busy() -> RunnerError {
        error(EnvError::Busy(
            "environment `stack` is busy: run abc (pid 7, task `dev`) is transitioning it. Nothing was changed; inspect it with `effigy container stack status`, then retry".to_owned(),
        ))
    }

    /// A busy refusal keeps its exit code and its one-sentence text, and
    /// carries no parallel JSON record of the environment.
    #[test]
    fn a_busy_refusal_is_one_sentence() {
        let refused = busy();
        assert_eq!(refused.process_exit_code(), 1);
        let text = refused.to_string();
        assert!(text.contains("environment `stack` is busy"), "{text}");
        assert!(text.contains("run abc"), "{text}");
        assert!(text.contains("pid 7"), "{text}");
        assert!(text.contains("task `dev`"), "{text}");
        assert!(text.contains("effigy container stack status"), "{text}");
        assert!(refused.json_error_details().is_none());

        // Every other environment error keeps the plain invocation error.
        let other = error(EnvError::Invalid("no record".to_owned()));
        assert_eq!(other.to_string(), "no record");
        assert!(other.json_error_details().is_none());
    }
}
