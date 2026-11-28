//! The runner's handle on the environment reconciler (contract 053).
//!
//! Every container lifecycle command reaches the engine here. Bring-up runs
//! as a reconcile run and an activation run, every stop is one transition,
//! and a command that uses an environment holds it through a session run
//! (or, for a managed run, through its own task run).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_containers::environment::{
    self as engine, Applied, BackendError, BackendSelection, CreationOutcome, EnvContext, EnvError,
    EnvRecord, EnvRef, HoldError, Idle, IdleArm, IdleArmFence, Operation, Retention, Session, Up,
};
use effigy_containers::exec::ContainerExecError;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::CancelToken;
use effigy_kernel::gate::Wait;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::run::AttachMode;

use crate::runner::container_command::barge_lifecycle;
use crate::runner::error::RunnerError;

/// A managed run's task-run id: the bring-up it started attaches it.
pub(in crate::runner) const HOLDER_ENV: &str = "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER";
/// Opts task-started environments out of idle stop.
const KEEP_RUNNING_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE";
const IDLE_TIMEOUT_ENV: &str = "EFFIGY_CONTAINER_IDLE_TIMEOUT_SECS";
const LEGACY_IDLE_TIMEOUT_ENV: &str = "EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS";
const DISABLE_REAPER_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER";
const DEFERRED_IDLE_ARM_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_IDLE_ARM";
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;
const IDLE_REAPER_POLL: Duration = Duration::from_secs(1);
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
    match error {
        EnvError::Unsettled { run, effects } => RunnerError::PredecessorUnsettled {
            run: run.to_string(),
            effects,
            context: Some("container environment"),
        },
        other => RunnerError::task_invocation(other.to_string()),
    }
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
    match record_until(policy, Instant::now() + Duration::from_secs(10)) {
        Observed::Verified(record, _) => Some(record),
        _ => None,
    }
}

/// The environment's record, read within the caller's own absolute deadline.
///
/// Unlike [`record`], this keeps the three outcomes apart: a missing store or
/// row is `Absent` with its proof, and an unreadable state root or record is
/// `Unknown`. A publication that must not promote a default asks this one.
pub(in crate::runner) fn record_until(
    policy: &EffectiveContainerPolicy,
    until: Instant,
) -> Observed<EnvRecord> {
    // Refuse an expired budget before any filesystem work: establishing the
    // checkout token writes and resolving the environment reads.
    let remaining = until.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Observed::Unknown(UnknownReason::TimedOut {
            phase: "environment record".into(),
        });
    }
    // The environment resource is scoped by this checkout's identity token, which
    // a prepare establishes. Failing to establish or resolve it is unknown
    // evidence, never an absent record.
    if let Err(error) = effigy_core::worktree_scope::prepare_checkout_token(&policy.repo_root) {
        return Observed::Unknown(UnknownReason::Inconsistent(format!(
            "the environment's checkout identity cannot be established: {error}"
        )));
    }
    let env = match EnvRef::for_policy(policy) {
        Ok(env) => env,
        Err(error) => {
            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                "the environment's checkout identity cannot be resolved: {error}"
            )))
        }
    };
    let remaining = until.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Observed::Unknown(UnknownReason::TimedOut {
            phase: "environment record".into(),
        });
    }
    let cx = super::task_engine::observe_context(remaining);
    let root = match engine::observe_root(&cx, &policy.repo_root) {
        Observed::Verified(root, _) => root,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    EnvRecord::read(&cx, &root, &env.resource)
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
        return Ok(verified(false));
    }
    let live = rows
        .into_iter()
        .filter(|row| row.status.is_running())
        .collect::<Vec<_>>();
    if live.is_empty() {
        return Ok(verified(false));
    }
    // One labelled `ps` answers both questions: the rows are this project's
    // containers, and a row that runs is part of the stack.
    Ok(
        match effigy_containers::exec::compose_stack_is_running(policy, &live) {
            Ok(running) => verified(running),
            Err(error) => unknown(error),
        },
    )
}

/// A failed inventory read stays `Unknown`, never a `false` absence.
///
/// The container crate decides what its own failure means; the runner keeps
/// that typed class instead of flattening every refusal into one display
/// string. An engine that answered it is not running is carried as the typed
/// answer it decoded, so a caller can point at the daemon to start rather than
/// at the runner's wording.
fn unknown(error: ContainerExecError) -> Observed<bool> {
    Observed::Unknown(unknown_reason(error))
}

fn unknown_reason(error: ContainerExecError) -> UnknownReason {
    match &error {
        ContainerExecError::Failure { .. } => match error.engine_absence() {
            Some(absence) => UnknownReason::ToolUnavailable(format!(
                "the container runtime is not running: {absence}"
            )),
            None => UnknownReason::Inconsistent(error.to_string()),
        },
        ContainerExecError::Launch { .. } => UnknownReason::ToolUnavailable(error.to_string()),
        ContainerExecError::Refusal { .. } => UnknownReason::Unparsable(error.to_string()),
    }
}

/// A verified inventory answer, or the typed reason its evidence could not be
/// built. The source is a constant, so the failure is a defect signal rather
/// than an expected path; it is never an absence.
fn verified(running: bool) -> Observed<bool> {
    match effigy_kernel::observe::Evidence::new("environment inventory".into(), None) {
        Ok(evidence) => Observed::Verified(running, evidence),
        Err(reason) => Observed::Unknown(reason),
    }
}

/// A bring-up whose backend apply finished, before the command's
/// integrations activate it.
#[derive(Debug)]
pub(in crate::runner) struct BringUp {
    env: EnvRef,
    repo_root: PathBuf,
    /// The apply's creation evidence. Ordinary unsealed applies report an
    /// unproven outcome; only the sealed workspace route can attribute IDs.
    /// `None` when the environment was held and no apply ran.
    pub(in crate::runner) creation: Option<CreationOutcome>,
    pub(in crate::runner) holders: Vec<RunId>,
}

/// The retention a task's ensure gives a stack it starts.
pub(in crate::runner) fn task_retention() -> Retention {
    if env_enabled(KEEP_RUNNING_ENV) {
        Retention::Persistent
    } else {
        Retention::Held
    }
}

/// The apply run of a bring-up. `apply` performs the one backend effect:
/// `compose up -d --remove-orphans`, from which Compose recreates only the
/// services whose rendered configuration changed.
///
/// Production bring-ups use [`reconcile_owned`], which hands the apply the Env
/// Run it must spawn under (effigy#1003). This plain form stays for fixtures
/// whose apply starts no retained child.
#[cfg(test)]
pub(in crate::runner) fn reconcile(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    retention: Retention,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<BringUp, RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context(&policy.repo_root)?;
    prepare_apply(repo_root, policy)?;
    let backend = backend(repo_root, policy);
    let outcome = engine::reconcile(
        &ctx,
        &env,
        up(repo_root, policy, &backend, retention),
        || {
            prepare_isolated_mount_points(repo_root, policy)
                .map_err(|error| BackendError::Failed(error.to_string()))?;
            apply()
        },
    )
    .map_err(error)?;
    finish_reconcile(policy, &env, outcome)
}

/// [`reconcile`] for an apply that must spawn under the reconcile run's own
/// retained custody: a secret-bearing VM Compose launch (Tom's #534 ruling,
/// effigy#1003). The closure receives the exact `Run` that holds `Gate(Env)`,
/// the child effect that bounds the apply and that deadline, so the Compose
/// child belongs to the same owner as the transition that started it.
pub(in crate::runner) fn reconcile_owned(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    retention: Retention,
    apply: impl FnOnce(
        &mut effigy_kernel::run::Run<'_>,
        &effigy_kernel::ctx::EffectContext,
        Instant,
    ) -> Result<(), BackendError>,
) -> Result<BringUp, RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context(&policy.repo_root)?;
    prepare_apply(repo_root, policy)?;
    let backend = backend(repo_root, policy);
    let outcome = engine::reconcile_owned(
        &ctx,
        &env,
        up(repo_root, policy, &backend, retention),
        |run, effect, deadline| {
            // Q-011 permits ordinary applies when holders are unreadable, but
            // a workspace with named volumes may need an automatic recreation.
            // Prove its holder set before any Compose up can replace it.
            if policy.workspace_user.is_some()
                && effigy_containers::load_workspace_ownership_plan(policy)
                    .map_err(|error| BackendError::NotStarted(error.to_string()))?
                    .targets
                    .iter()
                    .any(|target| {
                        target.mount_kind == effigy_containers::WorkspaceMountKind::NamedVolume
                    })
            {
                ensure_mount_repair_unheld(policy)
                    .map_err(|error| BackendError::NotStarted(error.to_string()))?;
            }
            prepare_isolated_mount_points(repo_root, policy)
                .map_err(|error| BackendError::Failed(error.to_string()))?;
            apply(run, effect, deadline)
        },
    )
    .map_err(error)?;
    finish_reconcile(policy, &env, outcome)
}

/// The one `Up` a bring-up's apply describes, shared by both reconcile paths.
fn up<'a>(
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    backend: &'a BackendSelection,
    retention: Retention,
) -> Up<'a> {
    Up {
        render: policy.render.as_ref().map(|render| &render.closure),
        project: &policy.project_name,
        backend,
        retention,
        workdir: repo_root,
        primary_service: &policy.primary_service,
        compose_files: &policy.compose_files,
    }
}

/// The pre-apply checks every bring-up runs before the environment is touched.
fn prepare_apply(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Result<(), RunnerError> {
    // The exact-identity inventory still removes owned containers a profile
    // transition left unusable, so the apply recreates rather than collides.
    let _running = running(repo_root, policy)?;
    Ok(())
}

fn prepare_isolated_mount_points(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    // A running service may have lost a nested mount. Its host path is not
    // safe to inspect or recreate; Compose will reattach the volume instead.
    if !host_mountpoints_may_be_prepared(repo_root, policy) {
        return Ok(());
    }
    // A fresh stack needs its bind-mounted mount points before creation.
    effigy_containers::exec::ensure_isolated_mount_host_points(repo_root, policy)
        .map(|_| ())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "container `{}` could not prepare its isolated mount points in the checkout: {error}",
                policy.name
            ))
        })
}

fn finish_reconcile(
    policy: &EffectiveContainerPolicy,
    env: &EnvRef,
    outcome: engine::BringUp,
) -> Result<BringUp, RunnerError> {
    match &outcome.applied {
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
    let holders = match outcome.applied {
        Applied::HeldByRuns(holders) => holders,
        _ => Vec::new(),
    };
    Ok(BringUp {
        env: env.clone(),
        repo_root: policy.repo_root.clone(),
        creation: outcome.creation,
        holders,
    })
}

/// Only a complete, empty running inventory permits touching isolated host paths.
pub(in crate::runner) fn host_mountpoints_may_be_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> bool {
    let deadline = Instant::now() + INVENTORY_BUDGET;
    let Ok(rows) =
        effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            repo_root,
            policy,
            Some(deadline),
        )
    else {
        return false;
    };
    !rows.iter().any(|row| {
        row.project_name.as_deref() == Some(policy.project_name.as_str()) && row.status.is_running()
    })
}

/// A held environment cannot be recreated to repair a detached mount.
pub(in crate::runner) fn refuse_held_detached_mounts(
    bring_up: &BringUp,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
) -> Result<(), RunnerError> {
    if bring_up.holders.is_empty() {
        return Ok(());
    }
    match crate::runner::workspace_command::permissions::verify_workspace_named_volumes(
        &bring_up.repo_root,
        policy,
        container_name,
    ) {
        Err(RunnerError::DetachedNamedVolumes { mounts, detail }) => {
            let holders = bring_up
                .holders
                .iter()
                .map(|run| format!("run {run}"))
                .collect::<Vec<_>>()
                .join(", ");
            Err(RunnerError::DetachedNamedVolumes {
                mounts,
                detail: format!(
                    "{detail}\nautomatic repair will run after active holders release: {holders}"
                ),
            })
        }
        result => result,
    }
}

/// The Env gate prevents new attachments while the repair checks its holders.
pub(in crate::runner) fn ensure_mount_repair_unheld(
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let ctx = context(&policy.repo_root)?;
    let env = env_ref(policy)?;
    let holders = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => record.holders,
        _ => return Err(RunnerError::task_invocation(
            "cannot verify active environment holders; automatic named-volume repair is deferred",
        )),
    };
    match holders {
        Observed::Verified(holders, _) if holders.is_empty() => Ok(()),
        Observed::Absent(_) => Ok(()),
        Observed::Verified(holders, _) => Err(RunnerError::task_invocation(format!(
            "automatic named-volume repair is deferred until active holders release: {}",
            holders
                .iter()
                .map(|holder| format!("run {}", holder.run))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        Observed::Unknown(_) => Err(RunnerError::task_invocation(
            "cannot verify active environment holders; automatic named-volume repair is deferred",
        )),
    }
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
    // The guest artifact is acquired before the activation run opens (§8.9).
    let artifact =
        super::workspace_command::prepare_workspace_artifact(&bring_up.repo_root, policy)?;
    engine::activate_with(&ctx, &bring_up.env, &holders, |run| {
        super::workspace_command::publish_workspace_guest_in(
            &bring_up.repo_root,
            policy,
            run,
            artifact.as_ref(),
        )
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
/// host teardown plus the backend stop, reporting the runtime's lines into the
/// transition's progress so the stop is one line (effigy#974).
pub(in crate::runner) fn stop(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    op: Operation,
    effect: impl FnOnce(&effigy_ui::steps::ProgressSource) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    stop_exempting(repo_root, policy, op, None, effect)
}

/// [`stop`] for a run settling in this process: its own hold is exempt by id.
pub(in crate::runner) fn stop_exempting(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    op: Operation,
    exempt: Option<RunId>,
    effect: impl FnOnce(&effigy_ui::steps::ProgressSource) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = env_ref(policy)?;
    stop_env(
        &policy.repo_root,
        &env,
        &backend(repo_root, policy),
        &policy.project_name,
        op,
        exempt,
        |progress| {
            effect(progress)?;
            crate::runner::container_command::secret_env::cleanup_container_secret_runtime(
                &policy.repo_root,
                &policy.name,
            )?;
            barge_lifecycle::return_torn_down_stack(&policy.profile, &policy.project_name);
            Ok(())
        },
    )
}

/// A stop of a same-checkout peer known only by name and project, such as a
/// `data_from` peer whose own policy cannot be resolved for a whole handover.
pub(in crate::runner) fn stop_peer(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    peer: &str,
    project: &str,
    effect: impl FnOnce(&effigy_ui::steps::ProgressSource) -> Result<(), RunnerError>,
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
    effect: impl FnOnce(&effigy_ui::steps::ProgressSource) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let ctx = context(repo_root)?;
    let mut failure = None;
    let result = engine::down(&ctx, env, op, backend, project, exempt, |progress| {
        effect(progress).map_err(|error| {
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
    effect: impl FnOnce(&effigy_ui::steps::ProgressSource) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let env = env_ref(policy)?;
    let ctx = context(&policy.repo_root)?;
    let backend = backend(repo_root, policy);
    let mut failure = None;
    let result = engine::reset(&ctx, &env, &backend, &policy.project_name, |progress| {
        let torn_down = effect(progress).and_then(|()| {
            crate::runner::container_command::secret_env::cleanup_container_secret_runtime(
                &policy.repo_root,
                &policy.name,
            )?;
            barge_lifecycle::return_torn_down_stack(&policy.profile, &policy.project_name);
            Ok(())
        });
        torn_down.map_err(|error| {
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
        stop_exempting(repo_root, policy, op, exempt, |progress| {
            crate::runner::fs_relay::stop_stack_relay(repo_root, policy);
            report_host_process_stop(
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy),
            );
            effect(progress).map_err(RunnerError::from)
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
        reset(repo_root, policy, |progress| {
            crate::runner::fs_relay::stop_stack_relay(repo_root, policy);
            report_host_process_stop(
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy),
            );
            effect(progress).map_err(RunnerError::from)
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
    /// backend effects. Guest work uses its own local state.
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

fn env_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

fn idle_timeout() -> Duration {
    let value =
        std::env::var_os(IDLE_TIMEOUT_ENV).or_else(|| std::env::var_os(LEGACY_IDLE_TIMEOUT_ENV));
    Duration::from_secs(
        value
            .as_deref()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS),
    )
}

fn epoch_ms() -> u64 {
    milliseconds(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// After a session settles, stop immediately or schedule its idle reap.
pub(in crate::runner) fn release(repo_root: &Path, policy: &EffectiveContainerPolicy) {
    let (Ok(env), Ok(ctx)) = (env_ref(policy), context(&policy.repo_root)) else {
        return;
    };
    let timeout = idle_timeout();
    if !timeout.is_zero() {
        if let Err(error) = schedule_idle_reap(repo_root, policy, &ctx, &env, timeout) {
            eprintln!(
                "[warn] environment `{}` was left running: could not schedule idle stop ({error})",
                policy.name
            );
        }
        return;
    }
    let outcome = engine::reap_if_idle(&ctx, &env, |run| stop_idle(run, repo_root, policy));
    match outcome {
        Ok(Idle::Stopped) => eprintln!(
            "[info] stopped environment `{}`: the last run using it ended. `effigy container up --detach` keeps an environment running",
            policy.name
        ),
        Ok(Idle::Busy | Idle::Kept(_)) => {}
        Err(error) => eprintln!(
            "[warn] environment `{}` was left running: idle stop failed ({error})",
            policy.name
        ),
    }
}

fn schedule_idle_reap(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    timeout: Duration,
) -> Result<(), RunnerError> {
    let token = super::task_engine::fresh_run_id()?.to_string();
    let deadline = epoch_ms().saturating_add(milliseconds(timeout));
    let deferred = match engine::arm_idle_reap(ctx, env, deadline, &token).map_err(error)? {
        IdleArm::Armed => None,
        IdleArm::Busy(fence) => Some(DeferredIdleArm { deadline, fence }),
        IdleArm::Skipped(_) => return Ok(()),
    };
    if env_enabled(DISABLE_REAPER_ENV) {
        return Ok(());
    }
    let mut command = Command::new(std::env::current_exe().map_err(RunnerError::Cwd)?);
    command
        .arg("__container-lease-reaper")
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--container")
        .arg(&policy.name)
        .arg("--token")
        .arg(&token)
        .env_remove(effigy_core::run_deadline::RUN_DEADLINE_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let handoff = barge_lifecycle::handoff_file(policy, &token);
    if let Some(handoff) = &handoff {
        command.env(barge_lifecycle::HANDOFF_ENV, handoff);
    } else {
        command.env_remove(barge_lifecycle::HANDOFF_ENV);
    }
    if let Some(deferred) = deferred {
        let pending = serde_json::to_string(&deferred)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        command.env(DEFERRED_IDLE_ARM_ENV, pending);
    } else {
        command.env_remove(DEFERRED_IDLE_ARM_ENV);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: setsid is async-signal-safe. last_os_error captures errno
        // without allocating in the forked child before exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "__container-lease-reaper".to_owned(),
            error,
        })?;
    if let Some(handoff) = &handoff {
        barge_lifecycle::await_handoff(handoff, &mut child);
    }
    // A command can hold more than one session before exiting. Collect each
    // reaper that finishes while its original parent is still alive.
    let _ = std::thread::Builder::new().spawn(move || child.wait());
    Ok(())
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct DeferredIdleArm {
    deadline: u64,
    fence: IdleArmFence,
}

pub(in crate::runner) fn run_idle_reaper(
    args: effigy_cli::InternalContainerLeaseReaperArgs,
) -> Result<String, RunnerError> {
    let policy =
        effigy_containers::load_container_policy(&args.repo_root, Some(&args.container_name))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let deferred = match std::env::var(DEFERRED_IDLE_ARM_ENV) {
        Ok(value) => Some(
            serde_json::from_str(&value)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        ),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };
    // The reaper holds the lease through the linger window, so the warm stack
    // is not swept before its own teardown returns it.
    barge_lifecycle::hold_for_session(&policy);
    if let Some(handoff) = std::env::var_os(barge_lifecycle::HANDOFF_ENV) {
        // The parent stops waiting after its budget if this write fails.
        let _ = std::fs::write(handoff, b"held");
    }
    drive_idle_reaper(
        &args.repo_root,
        &policy,
        &args.token,
        deferred,
        std::thread::sleep,
    )?;
    Ok(String::new())
}

fn drive_idle_reaper(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    token: &str,
    deferred: Option<DeferredIdleArm>,
    mut pause: impl FnMut(Duration),
) -> Result<(), RunnerError> {
    if let Some(DeferredIdleArm { deadline, fence }) = deferred {
        let env = env_ref(policy)?;
        loop {
            let ctx = context(&policy.repo_root)?;
            match fence.retry(&ctx, &env, deadline, token).map_err(error)? {
                IdleArm::Armed => break,
                IdleArm::Busy(_) => pause(IDLE_REAPER_POLL),
                IdleArm::Skipped(_) => return Ok(()),
            }
        }
    }
    while let Some(deadline) = idle_deadline(policy, token) {
        let now = epoch_ms();
        if now >= deadline {
            let env = env_ref(policy)?;
            let ctx = context(&policy.repo_root)?;
            match engine::reap_if_idle_due(&ctx, &env, token, now, |run| {
                stop_idle(run, repo_root, policy)
            })
            .map_err(error)?
            {
                Idle::Busy => pause(IDLE_REAPER_POLL),
                _ => break,
            }
        } else {
            pause(Duration::from_millis(deadline - now).min(IDLE_REAPER_POLL));
        }
    }
    Ok(())
}

fn idle_deadline(policy: &EffectiveContainerPolicy, token: &str) -> Option<u64> {
    let current = record(policy)?;
    let engine = current.engine()?;
    if current.active()
        && engine.retention == Retention::Held
        && engine.idle_reap_token.as_deref() == Some(token)
    {
        engine.idle_reap_after_epoch_ms
    } else {
        None
    }
}

fn stop_idle(
    run: &mut engine::EnvRun<'_>,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EnvError> {
    let backend = engine::effective_backend(repo_root, policy, run.record().recorded_backend());
    run.stop_with(&backend, &policy.project_name, None, |_| {
        crate::runner::fs_relay::stop_stack_relay(repo_root, policy);
        effigy_containers::exec::shutdown_container(repo_root, policy)
            .map_err(|error| BackendError::Uncertain(error.to_string()))?;
        crate::runner::container_command::secret_env::cleanup_container_secret_runtime(
            &policy.repo_root,
            &policy.name,
        )
        .map_err(|error| BackendError::Uncertain(error.to_string()))?;
        barge_lifecycle::return_torn_down_stack(&policy.profile, &policy.project_name);
        Ok(())
    })
}

/// The notice for a stack a command started and holds.
pub(in crate::runner) fn emit_held_notice(container_name: &str) {
    let seconds = idle_timeout().as_secs();
    let linger = match seconds {
        0 => "it stops when the last run using it ends".to_owned(),
        seconds if seconds.is_multiple_of(60) => {
            format!(
                "it stays warm for {}m after the last run using it ends",
                seconds / 60
            )
        }
        seconds => format!("it stays warm for {seconds}s after the last run using it ends"),
    };
    eprintln!(
        "[info] environment `{container_name}` was started for this run and {linger}. Run `effigy container up --detach` to keep it running indefinitely, or `effigy container down` to stop it now"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test_support::{lock_test, EnvGuard};
    use effigy_containers::environment::Retention;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

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

    /// A checkout with one `web` environment and a private `bin` of runtime
    /// doubles. Nothing here touches a live runtime, image or container.
    struct EnvironmentFixture {
        _fixture: effigy_test_support::Fixture,
        repo_root: PathBuf,
        bin: PathBuf,
        home: PathBuf,
    }

    impl EnvironmentFixture {
        fn new(name: &str, docker: &str) -> Self {
            let fixture =
                effigy_test_support::Fixture::workspace(&format!("effigy-environment-{name}-"))
                    .expect("private fixture root");
            let repo_root = fixture.path().join("repo");
            fs::create_dir_all(repo_root.join(".git")).expect("mkdir checkout metadata");
            // One ephemeral checkout scope, so the environment journal has an
            // identity to key on without a shared state root.
            fs::write(
                repo_root.join(".git/config"),
                "[effigy]\nruntimeScope = ephemeral\n",
            )
            .expect("write checkout scope marker");
            fs::write(
                repo_root.join("effigy.toml"),
                "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nproject_name = \"effigy-environment-fixture\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n",
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("docker-compose.yml"),
                "services:\n  app:\n    image: example.invalid/fixture\n",
            )
            .expect("write compose file");
            let bin = fixture.path().join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            // Shadow both runtimes: `command_exists("colima")` probes the
            // well-known install directories, so an absent double would let a
            // developer's real VMs answer.
            Self::write_executable(&bin, "colima", "#!/bin/sh\nexit 0\n");
            Self::write_executable(&bin, "docker", docker);
            let home = fixture.path().join("home");
            fs::create_dir_all(&home).expect("mkdir isolated home");
            Self {
                _fixture: fixture,
                repo_root,
                bin,
                home,
            }
        }

        fn write_executable(bin: &Path, name: &str, body: &str) {
            let path = bin.join(name);
            fs::write(&path, body).expect("write runtime double");
            let mut permissions = fs::metadata(&path)
                .expect("stat runtime double")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod runtime double");
        }

        fn env(&self) -> EnvGuard {
            EnvGuard::set_many(&[
                (
                    "PATH",
                    Some(format!("{}:/usr/bin:/bin", self.bin.display())),
                ),
                ("HOME", Some(self.home.display().to_string())),
                (
                    "EFFIGY_TEST_CONTAINERS_HOME",
                    Some(self.home.join(".effigy").display().to_string()),
                ),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ])
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            crate::runner::test_support::load_fixture_policy(&self.repo_root, None)
                .expect("fixture container policy")
        }
    }

    /// An engine that answered it is not running reaches the caller as the
    /// typed tool absence it decoded, naming the daemon to start.
    #[test]
    fn an_engine_that_is_not_running_is_a_typed_tool_absence() {
        let fixture = EnvironmentFixture::new(
            "engine-absent",
            "#!/bin/sh\nprintf 'Cannot connect to the Docker daemon at unix:///var/run/docker.sock\n' >&2\nexit 1\n",
        );
        let _lock = lock_test();
        let _env = fixture.env();

        let observed = running(&fixture.repo_root, &fixture.policy())
            .expect("a failed inventory read keeps the command alive");
        match observed {
            Observed::Unknown(UnknownReason::ToolUnavailable(detail)) => assert!(
                detail.contains("the Docker daemon"),
                "the typed answer names what to start: {detail}"
            ),
            other => panic!(
                "an unreachable engine must not be a false absence: {}",
                other.unproven()
            ),
        }
    }

    /// The typed class of every inventory failure survives: a completed
    /// command that failed, a refused observation, and a launch failure are
    /// three different `Unknown` reasons, never one display string.
    ///
    /// The end-to-end engine-absence case is above; a missing runtime binary
    /// cannot be staged here because host CLI resolution also probes the
    /// platform's install directories.
    #[test]
    fn inventory_failures_keep_their_typed_class() {
        use std::io::ErrorKind;

        let launch = ContainerExecError::Launch {
            command: "docker ps".to_owned(),
            error: std::io::Error::new(ErrorKind::NotFound, "no such binary"),
        };
        assert!(
            matches!(unknown_reason(launch), UnknownReason::ToolUnavailable(_)),
            "a launch failure is tool availability"
        );

        let refused = ContainerExecError::Refusal {
            command: "docker ps".to_owned(),
            reason: "incomplete or inconsistent project inventory".to_owned(),
        };
        assert!(
            matches!(unknown_reason(refused), UnknownReason::Unparsable(_)),
            "a refused observation is unparsable evidence"
        );

        let failed = ContainerExecError::Failure {
            command: "docker ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "port is already allocated".to_owned(),
        };
        assert!(
            matches!(unknown_reason(failed), UnknownReason::Inconsistent(_)),
            "a plain failed command is an inconsistent observation"
        );
    }

    /// A checkout with no environment journal is `Absent` with its proof, not
    /// `Unknown`: the read never promotes a default and never writes.
    #[test]
    fn an_absent_journal_is_absence_not_unknown() {
        let fixture = EnvironmentFixture::new("absent", "#!/bin/sh\nexit 0\n");
        let _lock = lock_test();
        let _env = fixture.env();
        let policy = fixture.policy();

        let observed = record_until(&policy, Instant::now() + Duration::from_secs(5));
        assert!(
            matches!(observed, Observed::Absent(_)),
            "a missing environment journal is an absence with proof: {}",
            observed.unproven()
        );
        assert!(
            record(&policy).is_none(),
            "the convenience read agrees: no record yet"
        );
    }

    /// A hold taken before a bring-up settles without publishing an
    /// environment: nothing was started, so nothing claims one.
    #[test]
    fn a_hold_that_never_saw_a_bring_up_publishes_no_environment() {
        let fixture = EnvironmentFixture::new("hold", "#!/bin/sh\nexit 0\n");
        let _lock = lock_test();
        let _env = fixture.env();
        let policy = fixture.policy();

        let mut held = Held::begin(&fixture.repo_root, &policy, "environment-test")
            .expect("a host session begins")
            .expect("a host session is returned");
        assert!(held.id().is_some(), "the session has its own run id");
        assert!(
            !held
                .attach_active()
                .expect("an inactive environment is not an error"),
            "nothing is active to attach to"
        );
        assert!(!held.holds(), "an unattached session holds nothing");
        drop(held);

        assert!(
            record(&policy).is_none(),
            "a released hold leaves no environment record behind"
        );
    }

    /// The lease switch is one env read: only an explicit `1`/`true`/`yes`
    /// makes a task-started stack persistent.
    #[test]
    fn task_retention_follows_the_keep_running_switch() {
        let _lock = lock_test();
        for (value, expected) in [
            (None, Retention::Held),
            (Some("1"), Retention::Persistent),
            (Some("true"), Retention::Persistent),
            (Some(" yes "), Retention::Persistent),
            (Some("0"), Retention::Held),
            (Some("no"), Retention::Held),
            (Some(""), Retention::Held),
        ] {
            let _env = EnvGuard::set_many(&[(KEEP_RUNNING_ENV, value.map(str::to_owned))]);
            assert_eq!(task_retention(), expected, "value: {value:?}");
        }
    }

    #[test]
    fn idle_timeout_defaults_and_uses_the_legacy_value_only_when_unset() {
        let _lock = lock_test();
        for (current, legacy, seconds) in [
            (None, None, 300),
            (None, Some("2"), 2),
            (Some("0"), Some("2"), 0),
            (Some(" 60 "), Some("2"), 60),
            (Some("invalid"), Some("2"), 300),
        ] {
            let _env = EnvGuard::set_many(&[
                (IDLE_TIMEOUT_ENV, current.map(str::to_owned)),
                (LEGACY_IDLE_TIMEOUT_ENV, legacy.map(str::to_owned)),
            ]);
            assert_eq!(
                idle_timeout(),
                Duration::from_secs(seconds),
                "{current:?}, {legacy:?}"
            );
        }
    }

    #[test]
    fn successful_stop_and_reset_remove_secret_runtime_files() {
        let _lock = lock_test();
        let fixture = EnvironmentFixture::new("secret-stop", "#!/bin/sh\nexit 0\n");
        let _env = fixture.env();
        let policy = fixture.policy();
        let dir = fixture
            .repo_root
            .join(".effigy/runtime/secrets")
            .join(&policy.name);
        for operation in [Operation::Down, Operation::Reset] {
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("runtime.env"), "secret").unwrap();
            if operation == Operation::Down {
                stop(&fixture.repo_root, &policy, operation, |_| Ok(())).unwrap();
            } else {
                reset(&fixture.repo_root, &policy, |_| Ok(())).unwrap();
            }
            assert!(!dir.exists());
        }
    }

    #[test]
    fn the_idle_reaper_retries_a_busy_gate_when_arming_or_stopping() {
        use effigy_containers::environment::{EngineRecord, EnvRun, EnvState};
        use effigy_kernel::gate::Gate;

        let _lock = lock_test();
        for phase in ["arming", "stopping"] {
            let fixture = EnvironmentFixture::new(
                phase,
                "#!/bin/sh\ncase \"$*\" in *'down --remove-orphans'*) echo stop >> idle-reap.log;; esac\n",
            );
            let _env = fixture.env();
            let policy = fixture.policy();
            let secret_dir = fixture
                .repo_root
                .join(".effigy/runtime/secrets")
                .join(&policy.name);
            fs::create_dir_all(&secret_dir).unwrap();
            fs::write(secret_dir.join("runtime.env"), "secret").unwrap();
            let env = env_ref(&policy).unwrap();
            let ctx = context(&policy.repo_root).unwrap();
            let mut up = EnvRun::begin(&ctx, &env, Operation::Up, Wait::FailFast).unwrap();
            up.write_record(&EngineRecord::new(Retention::Held, &policy.project_name))
                .unwrap();
            up.activate(&[]).unwrap();
            up.finish(true).unwrap();

            let token = "idle-reap-test";
            let deadline = 0;
            if phase == "stopping" {
                assert_eq!(
                    engine::arm_idle_reap(&ctx, &env, deadline, token).unwrap(),
                    IdleArm::Armed
                );
            }
            let mut gate = Some(
                Gate::acquire(
                    &ctx.cx,
                    ctx.root,
                    std::slice::from_ref(&env.resource),
                    Wait::FailFast,
                )
                .unwrap(),
            );
            let deferred = if phase == "arming" {
                let IdleArm::Busy(fence) =
                    engine::arm_idle_reap(&ctx, &env, deadline, token).unwrap()
                else {
                    panic!("the release needs to hand the arm to the detached reaper");
                };
                let pending = serde_json::to_string(&DeferredIdleArm { deadline, fence }).unwrap();
                Some(serde_json::from_str(&pending).unwrap())
            } else {
                None
            };

            drive_idle_reaper(&fixture.repo_root, &policy, token, deferred, |delay| {
                assert_eq!(delay, IDLE_REAPER_POLL);
                let current = record(&policy).expect("environment still has its record");
                assert!(
                    current.active(),
                    "the gate still protects the stack while {phase}"
                );
                let current = current.engine().unwrap();
                if phase == "stopping" {
                    assert_eq!(current.idle_reap_after_epoch_ms, Some(deadline));
                    assert_eq!(current.idle_reap_token.as_deref(), Some(token));
                } else {
                    assert_eq!(current.idle_reap_token, None);
                }
                drop(
                    gate.take()
                        .expect("one busy attempt before the gate is released"),
                );
            })
            .unwrap();
            assert!(
                gate.is_none(),
                "the reaper retried after contention while {phase}"
            );
            assert_eq!(record(&policy).unwrap().state, EnvState::Stopped);
            assert!(!secret_dir.exists());
            assert_eq!(
                fs::read_to_string(fixture.repo_root.join("idle-reap.log")).unwrap(),
                "stop\n"
            );
        }
    }

    #[test]
    fn closed_incomplete_residue_blocks_container_up_and_down_until_run_acknowledge() {
        use effigy_containers::environment::{EngineRecord, EnvRun};
        use effigy_kernel::id::{Generation, RunId};
        use effigy_kernel::run::{Effect, EffectSpec, EffectState, Fence, Settlement};
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

        const CUSTODY_KIND: &str = "kernel:process";

        let _lock = lock_test();
        let fixture = EnvironmentFixture::new("ack-unblocks", "#!/bin/sh\nexit 0\n");
        let _env = fixture.env();
        let policy = fixture.policy();
        let env = env_ref(&policy).unwrap();
        let ctx = context(&policy.repo_root).unwrap();

        let mut initial = EnvRun::begin(&ctx, &env, Operation::Up, Wait::FailFast).unwrap();
        initial
            .write_record(&EngineRecord::new(Retention::Held, &policy.project_name))
            .unwrap();
        initial.activate(&[]).unwrap();
        initial.finish(true).unwrap();

        let effigy_home = fixture.home.join(".effigy");
        let compose_env_dir = effigy_home.join("compose-env");
        fs::create_dir_all(&compose_env_dir).unwrap();
        fs::set_permissions(&compose_env_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let file_name = "compose-ack-fixture.env";
        let file_path = compose_env_dir.join(file_name);
        {
            let _f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file_path)
                .unwrap();
        }
        let dir_meta = fs::metadata(&compose_env_dir).unwrap();
        let file_meta = fs::metadata(&file_path).unwrap();

        let mut dead_child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let dead_id = effigy_kernel::ident::process_identity(dead_child.id())
            .verified()
            .cloned()
            .unwrap();
        let _ = dead_child.kill();
        let _ = dead_child.wait();

        let state_root =
            crate::runner::task_engine::checkout_effects_root(&fixture.repo_root).unwrap();

        let run_id_str = {
            let conn =
                rusqlite::Connection::open(state_root.path().join(effigy_kernel::store::DB_NAME))
                    .unwrap();
            let (run_id_str, generation): (String, i64) = conn
                .query_row(
                    "SELECT run_id, generation FROM runs WHERE owner=?1 ORDER BY created_at DESC LIMIT 1",
                    [env.resource.name().to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let run_id: RunId = run_id_str.parse().unwrap();
            let fence = Fence {
                resource: env.resource.clone(),
                generation: Generation::new(generation as u64),
            };
            let target_spec = EffectSpec {
                target: fence.clone(),
                kind: CUSTODY_KIND.to_owned(),
                payload: serde_json::Value::Null,
                settlement: Settlement::Release(serde_json::Value::Null),
                required: true,
            };
            let effect = Effect {
                sequence: 1,
                spec: target_spec.clone(),
                state: EffectState::Applied,
                receipt: None,
                reason: None,
            };
            let invocation = "abcdef0123456789abcdef0123456789";
            let binding = serde_json::json!({
                "version": 1,
                "invocation": invocation,
                "run": run_id,
                "fence": fence,
                "effect": effect,
                "original": file_name,
                "location": "Original",
                "file": { "device": file_meta.dev(), "inode": file_meta.ino() },
                "directory": { "device": dir_meta.dev(), "inode": dir_meta.ino() },
                "completed": false,
            });
            {
                use std::io::Write;
                let mut custody = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(compose_env_dir.join(format!(".custody-{invocation}")))
                    .unwrap();
                custody
                    .write_all(&serde_json::to_vec(&binding).unwrap())
                    .unwrap();
            }
            let receipt = serde_json::json!({
                "interpolation_v1": {
                    "identity": dead_id,
                    "pgid": 0,
                    "nonce": "dead-nonce",
                    "grace_millis": 100,
                },
                "binding": binding,
            });
            conn.execute(
                "UPDATE runs SET phase='closed', cleanup_outcome='incomplete', residue_ack=NULL WHERE run_id=?1",
                [&run_id_str],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO events(run_id, seq, effect_id, kind, target, settlement, state, receipt, error, wall_time) \
                 VALUES (?1, 1, '1', ?2, ?3, ?4, 'residue', ?5, 'retained interpolation requires its live descriptor owner', 1)",
                rusqlite::params![
                    run_id_str,
                    CUSTODY_KIND,
                    serde_json::to_string(&target_spec).unwrap(),
                    serde_json::to_string(&Settlement::Release(serde_json::Value::Null)).unwrap(),
                    serde_json::to_string(&receipt).unwrap(),
                ],
            )
            .unwrap();
            run_id_str
        };

        let expected_hint = format!("effigy run acknowledge {run_id_str}");

        // `container up` (`EnvRun::begin`) refuses with the exact acknowledge hint.
        let up_err = EnvRun::begin(&ctx, &env, Operation::Up, Wait::FailFast)
            .map(|_| ())
            .map_err(error)
            .expect_err("residue run must block container up");
        assert!(
            matches!(up_err, RunnerError::PredecessorUnsettled { .. }),
            "{up_err:?}"
        );
        assert!(up_err.to_string().contains(&expected_hint), "{up_err}");
        assert!(
            !up_err.to_string().contains("stop what holds it"),
            "{up_err}"
        );
        let up_human = up_err.human_error().render_text();
        assert!(up_human.contains(&expected_hint), "{up_human}");

        // `container down` (`stop`) refuses with the same acknowledge hint.
        let down_err = stop(&fixture.repo_root, &policy, Operation::Down, |_| Ok(()))
            .expect_err("residue run must block container down");
        assert!(
            matches!(down_err, RunnerError::PredecessorUnsettled { .. }),
            "{down_err:?}"
        );
        assert!(down_err.to_string().contains(&expected_hint), "{down_err}");
        assert!(
            file_path.exists(),
            "orphaned env file stays until acknowledged"
        );

        let ack_json = crate::runner::run_control::run_runs(effigy_cli::RunsArgs {
            repo_override: Some(fixture.repo_root.clone()),
            output_json: true,
            action: effigy_cli::RunsAction::Acknowledge {
                run: run_id_str.clone(),
            },
        })
        .expect("effigy run acknowledge succeeds");
        let parsed: serde_json::Value = serde_json::from_str(&ack_json).unwrap();
        assert_eq!(parsed["acknowledged"], true);
        assert_eq!(parsed["run"], run_id_str);
        assert!(
            !file_path.exists(),
            "orphaned compose-env file should be cleaned on acknowledgement"
        );

        // Both `container down` and subsequent `container up` are now unblocked.
        stop(&fixture.repo_root, &policy, Operation::Down, |_| Ok(()))
            .expect("acknowledged residue unblocks container down");
        let next_up = EnvRun::begin(&ctx, &env, Operation::Up, Wait::FailFast)
            .expect("acknowledged residue unblocks container up");
        drop(next_up);
    }
}
