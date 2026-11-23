//! The hold API (contract 053 §4): how a run attaches to an environment and
//! what its release triggers. This is the attachment surface the run
//! supervisor uses.
//!
//! - `hold` attaches a live run to a running environment under `Gate(Env)`.
//!   The holder's own settle releases it (K5); no environment gate is needed
//!   for that, so a holder and a waiting transition never wait on each other.
//! - A run that finds the environment inactive asks the apply run to attach
//!   it instead (`EnvRun::activate` with requesting holders, §5.9).
//! - A persistent environment (an explicit `container up`) is borrowed: the
//!   holder's end leaves it running. Releasing the last holder can arm a
//!   fenced idle timer or stop the environment immediately.

use effigy_kernel::gate::{Gate, Wait};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{AttachMode, AttachmentTicket, Attachments, Fence, Run, RunError};

use super::reconcile::{EnvContext, EnvError, EnvRef, EnvRun, Operation};
use super::record::{EnvRecord, Retention};

/// One run's attachment to one environment.
#[derive(Debug)]
pub struct Hold {
    pub env: EnvRef,
    pub mode: AttachMode,
    ticket: AttachmentTicket,
}

impl Hold {
    pub fn ticket(&self) -> &AttachmentTicket {
        &self.ticket
    }
}

/// The attachment mode the environment's retention implies.
pub fn mode_for(record: &EnvRecord) -> AttachMode {
    match record.engine().map(|engine| engine.retention) {
        Some(Retention::Held) => AttachMode::Exclusive {
            stop_on_release: true,
        },
        // Persistent, or a record whose retention is unknown: never stop it.
        _ => AttachMode::Borrow,
    }
}

/// Attaches `holder` to `env`. `running` is the runtime's own answer to
/// whether the project's containers are up; a run that needs a stack started
/// asks the environment's apply run to attach it instead.
pub fn hold(
    holder: &mut Run<'_>,
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    wait: Wait,
    running: bool,
) -> Result<Hold, HoldError> {
    for _ in 0..2 {
        let record = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
            Observed::Verified(record, _) => record,
            Observed::Absent(_) => return Err(HoldError::NotActive),
            Observed::Unknown(reason) => {
                return Err(HoldError::Env(EnvError::Invalid(format!(
                    "the environment record cannot be read: {reason}"
                ))))
            }
        };
        if !running {
            return Err(HoldError::NotActive);
        }
        let mode = mode_for(&record);
        let fence = Fence {
            resource: env.resource.clone(),
            generation: record.generation,
        };
        match Attachments::attach_updating_engine(holder, &fence, mode, wait, |engine| {
            let Some(record) = engine.as_object_mut() else {
                return false;
            };
            // A run that attaches and finishes before the old reaper gets
            // the gate must still invalidate that reaper's token.
            let deadline = record.remove("idle_reap_after_epoch_ms");
            let token = record.remove("idle_reap_token");
            deadline.is_some() || token.is_some()
        }) {
            Ok(ticket) => {
                return Ok(Hold {
                    env: env.clone(),
                    mode,
                    ticket,
                })
            }
            // A transition advanced the generation between read and attach.
            Err(RunError::StaleGeneration { .. }) => continue,
            Err(RunError::EnvNotActive) => return Err(HoldError::NotActive),
            Err(error) => return Err(HoldError::Run(error)),
        }
    }
    Err(HoldError::NotActive)
}

#[derive(Debug)]
pub enum HoldError {
    /// The environment is not active at its current generation: ensure it.
    NotActive,
    Env(EnvError),
    Run(RunError),
}

impl std::fmt::Display for HoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotActive => f.write_str("environment is not active"),
            Self::Env(error) => write!(f, "{error}"),
            Self::Run(error) => write!(f, "environment hold refused: {error}"),
        }
    }
}

impl std::error::Error for HoldError {}

/// What a release policy decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Idle {
    /// The held environment had no holders left and was stopped.
    Stopped,
    /// Kept running, with the reason.
    Kept(&'static str),
}

/// Whether the release could record a new idle deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleArm {
    /// The deadline and token were recorded.
    Armed,
    /// The current environment could not be armed.
    Skipped(&'static str),
}

/// Record the deadline under Gate(Env), leaving the active generation intact.
///
/// # Errors
///
/// Returns an error if the gate, holder settlement, or record write fails.
pub fn arm_idle_reap(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    deadline_epoch_ms: u64,
    token: &str,
) -> Result<IdleArm, EnvError> {
    let read = || EnvRecord::read(&ctx.observe(), ctx.root, &env.resource);
    if let Err(reason) = idle_record(read()) {
        return Ok(IdleArm::Skipped(reason));
    }
    let gate = match Gate::acquire(
        &ctx.cx,
        ctx.root,
        std::slice::from_ref(&env.resource),
        Wait::FailFast,
    )
    .map_err(|error| EnvError::from_run(&env.name, error.into()))
    {
        Ok(gate) => gate,
        Err(EnvError::Busy(_)) => return Ok(IdleArm::Skipped("another transition holds it")),
        Err(error) => return Err(error),
    };
    let record = match idle_record(read()) {
        Ok(record) => record,
        Err(reason) => return Ok(IdleArm::Skipped(reason)),
    };
    let mut engine = record
        .engine()
        .cloned()
        .expect("idle held environment engine");
    let Observed::Verified(holders, _) = record.holders else {
        return Ok(IdleArm::Skipped("holders remain"));
    };
    if !holders.is_empty() {
        super::reconcile::settle_interrupted_holders(ctx, env, holders, |_, _, _| {})?;
        if read().verified().is_none_or(EnvRecord::protected) {
            return Ok(IdleArm::Skipped("holders remain"));
        }
    }
    engine.idle_reap_after_epoch_ms = Some(deadline_epoch_ms);
    engine.idle_reap_token = Some(token.to_owned());
    let fence = Fence {
        resource: env.resource.clone(),
        generation: record.generation,
    };
    let value =
        serde_json::to_value(engine).map_err(|error| EnvError::Invalid(error.to_string()))?;
    Attachments::set_engine_record_under_gate(&ctx.cx, ctx.root, &gate, &fence, &value)
        .map_err(|error| EnvError::from_run(&env.name, error))?;
    Ok(IdleArm::Armed)
}

/// Holder-based idle stop: after a holder settles, a held environment with
/// no remaining holders stops under its own reap run. A persistent one, one
/// still held, or one a transition is already changing is left alone.
pub fn reap_if_idle(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    stop: impl FnOnce(&mut EnvRun<'_>) -> Result<(), EnvError>,
) -> Result<Idle, EnvError> {
    reap(ctx, env, |_| None, stop)
}

/// Stop only if this timer still belongs to the idle held environment and
/// its deadline has passed. The checks are repeated under the reap gate.
///
/// # Errors
///
/// Returns an error if the gate, holder settlement, or stop transition fails.
pub fn reap_if_idle_due(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    expected_token: &str,
    now_epoch_ms: u64,
    stop: impl FnOnce(&mut EnvRun<'_>) -> Result<(), EnvError>,
) -> Result<Idle, EnvError> {
    reap(
        ctx,
        env,
        |record| match record.engine() {
            Some(engine) if engine.idle_reap_token.as_deref() == Some(expected_token) => (!engine
                .idle_reap_after_epoch_ms
                .is_some_and(|due| now_epoch_ms >= due))
            .then_some("idle deadline has not passed"),
            _ => Some("idle timer changed"),
        },
        stop,
    )
}

fn reap(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    wait_for_timer: impl Fn(&EnvRecord) -> Option<&'static str>,
    stop: impl FnOnce(&mut EnvRun<'_>) -> Result<(), EnvError>,
) -> Result<Idle, EnvError> {
    // A pure pre-check keeps a non-idle environment's generation unchanged.
    let record = match idle_record(EnvRecord::read(&ctx.observe(), ctx.root, &env.resource)) {
        Ok(record) => record,
        Err(reason) => return Ok(Idle::Kept(reason)),
    };
    if let Some(reason) = wait_for_timer(&record) {
        return Ok(Idle::Kept(reason));
    }
    let mut run = match EnvRun::begin(ctx, env, Operation::Reap, Wait::FailFast) {
        Ok(run) => run,
        Err(EnvError::Busy(_)) => return Ok(Idle::Kept("another transition holds it")),
        Err(error) => return Err(error),
    };
    run.settle_interrupted_holders()?;
    // Re-read under the gate: an attach may have landed after the pre-check.
    let fresh = match run.holders() {
        Observed::Verified(holders, _) => holders.is_empty(),
        _ => false,
    };
    let kept = match keep(run.record()) {
        Some("holders remain") | None => (!fresh).then_some("holders remain"),
        other => other,
    };
    if let Some(kept) = kept.or_else(|| wait_for_timer(run.record())) {
        run.finish(true)?;
        return Ok(Idle::Kept(kept));
    }
    let result = stop(&mut run);
    let ok = result.is_ok();
    run.finish(ok)?;
    result.map(|()| Idle::Stopped)
}

fn idle_record(observed: Observed<EnvRecord>) -> Result<EnvRecord, &'static str> {
    match observed {
        Observed::Verified(record, _) => match keep(&record) {
            None => Ok(record),
            Some(reason) => Err(reason),
        },
        Observed::Absent(_) => Err("no environment record"),
        Observed::Unknown(_) => Err("environment record unreadable"),
    }
}

fn keep(record: &EnvRecord) -> Option<&'static str> {
    if !record.active() {
        return Some("not active");
    }
    match record.engine().map(|engine| engine.retention) {
        Some(Retention::Held) => {}
        Some(Retention::Persistent) => return Some("persistent"),
        None => return Some("retention unknown"),
    }
    // Interrupted holders are settled by the reap itself (I-A2).
    let blocking = match &record.holders {
        Observed::Verified(holders, _) => holders.iter().any(|holder| {
            !matches!(
                holder.liveness,
                Observed::Verified(effigy_kernel::run::Liveness::Interrupted, _)
            )
        }),
        _ => true,
    };
    blocking.then_some("holders remain")
}

/// The holder for a command with no task run of its own (an exec, a shell,
/// a data command): one kernel run that holds the environment while the
/// command uses it. Closing it settles the run, which releases the hold.
#[derive(Debug)]
pub struct Session<'r> {
    run: Option<Run<'r>>,
    /// The capability and the settlement budget the session renews on close:
    /// a shell can hold an environment far longer than one bounded cleanup.
    cx: effigy_kernel::ctx::EffectContext,
    cleanup_budget: std::time::Duration,
}

impl<'r> Session<'r> {
    /// Admits a fresh session run of `env`'s checkout. Each session owns a
    /// distinct identity, so concurrent commands never wait on each other.
    pub fn begin(ctx: &EnvContext<'r>, env: &EnvRef, label: &str) -> Result<Self, EnvError> {
        use effigy_kernel::ctx::{EffectClass, Placement};
        use effigy_kernel::id::{ExecutionTarget, InvocationSpec, ResolvedInvocation, Route};
        let nonce = super::reconcile::new_run_id()?;
        let task = effigy_kernel::id::TaskId::new(
            env.checkout,
            "",
            effigy_kernel::id::Surface::Published,
            "@environment-session",
            Some(&nonce.to_string()),
        )
        .map_err(|error| EnvError::Invalid(error.to_string()))?;
        let invocation = ResolvedInvocation::new(InvocationSpec {
            task: task.clone(),
            route: Route::Task,
            argv: vec![std::ffi::OsString::from(label)],
            cwd: String::new(),
            target: ExecutionTarget::Host,
            env: Default::default(),
            owner: task.resource(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: Default::default(),
        })
        .map_err(|error| EnvError::Invalid(error.to_string()))?;
        let cleanup = ctx.cleanup()?;
        let run = Run::begin(
            &ctx.cx,
            ctx.root,
            &invocation,
            effigy_kernel::run::BeginOpts {
                id: nonce,
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: effigy_kernel::outcome::Invocation::validated(
                    vec![
                        std::ffi::OsString::from("effigy"),
                        std::ffi::OsString::from("container"),
                        std::ffi::OsString::from("status"),
                    ],
                    |_| true,
                )
                .map_err(|_| EnvError::Invalid("invalid recovery command".into()))?,
                custody: None,
            },
            &cleanup,
            &mut super::reconcile::EnvSettlement,
        )
        .map_err(|error| EnvError::Invalid(format!("environment session refused: {error}")))?;
        Ok(Self {
            run: Some(run),
            cx: ctx.cx.clone(),
            cleanup_budget: ctx.cleanup_budget(),
        })
    }

    pub fn id(&self) -> effigy_kernel::id::RunId {
        self.run.as_ref().expect("open session").id()
    }

    pub fn run_mut(&mut self) -> &mut Run<'r> {
        self.run.as_mut().expect("open session")
    }

    /// Ends the session: the settle releases its hold (no gate needed).
    pub fn close(mut self) -> effigy_kernel::outcome::CleanupOutcome {
        self.settle()
    }

    fn settle(&mut self) -> effigy_kernel::outcome::CleanupOutcome {
        let Some(mut run) = self.run.take() else {
            return effigy_kernel::outcome::CleanupOutcome::NotNeeded;
        };
        let _ = run.finish(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
        // The session's own settlement gets a budget that starts now: the
        // command it held for may have run for hours (effigy#447).
        let Ok(cleanup) = super::reconcile::cleanup(&self.cx, self.cleanup_budget) else {
            return effigy_kernel::outcome::CleanupOutcome::Incomplete;
        };
        match run.settle(&cleanup, &mut super::reconcile::EnvSettlement) {
            Ok(outcome) => outcome.cleanup(),
            Err(_) => effigy_kernel::outcome::CleanupOutcome::Incomplete,
        }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.settle();
    }
}
