//! The reconciler run: one kernel `Run` owned by the environment, holding
//! `Gate(Env)` for its whole operation (contract 053 §3).
//!
//! Up, down, handover, reset, reap and import each begin one of these runs,
//! so they serialise on the one gate. Every backend mutation goes through a
//! `TransitionPermit`: it is journalled before the adapter runs (K3) and
//! refused while any holder other than a settling exempt run remains (R4).
//!
//! One transition is one live step (effigy#461): the wait names the gate and
//! the holder its own body records, the reconcile names the phase now running,
//! and the frame always carries the last thing this run observed.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OperationContext,
    Placement,
};
use effigy_kernel::gate::{Gate, GateError, GateState, HolderDiag, Wait};
use effigy_kernel::id::{
    CheckoutId, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId, Route, RunId,
    Surface, TaskId,
};
use effigy_kernel::ident::ProcessState;
use effigy_kernel::objects::{Closure, Objects};
use effigy_kernel::observe::{AbsenceProof, Observed, UnknownReason};
use effigy_kernel::outcome::{CleanupOutcome, Invocation, RequiredObligation, WorkloadOutcome};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    AttachMode, Attachments, Effect, EffectSpec, Fence, Holder, Liveness, Reconciled, Run,
    RunError, Settlement, SettlementAdapter, TransitionPermit,
};
use effigy_ui::steps::{ProgressSource, Step};
use serde_json::{json, Value};

use super::backend::{BackendError, BackendSelection};
use super::record::{EngineRecord, EnvRecord, EnvState};

/// Journal kind of a backend stop. Reported, never replayed (D3).
pub const STOP_EFFECT: &str = "environment:stop";

/// The phase one apply (and the create it reports) runs in, in the operator's
/// own words.
const APPLY_PHASE: &str = "reconciling the backend";

/// How long a transition waits for `Gate(Env)` held by another one.
pub const GATE_WAIT: Duration = Duration::from_secs(600);
/// How often the gate-wait probe reads the held gate's own body. The kernel's
/// wait is a blocking poll with no callback, so this is what makes the wait
/// live without touching gate authority or its timeout (effigy#461).
const GATE_PROBE_INTERVAL: Duration = Duration::from_millis(250);
/// Every environment run is bounded (K8); bring-up includes image builds.
pub const RUN_BUDGET: Duration = Duration::from_secs(2 * 60 * 60);
/// Each settlement is bounded on its own clock: the cleanup context is made
/// when the settlement starts, so a slow activation never spends it (effigy#447).
pub const CLEANUP_BUDGET: Duration = Duration::from_secs(60);

/// One environment's identity: the checkout it belongs to and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvRef {
    pub checkout: CheckoutId,
    pub name: String,
    pub resource: ResourceId,
}

impl EnvRef {
    pub fn new(checkout: CheckoutId, name: &str) -> Result<Self, EnvError> {
        Ok(Self {
            checkout,
            name: name.to_owned(),
            resource: ResourceId::environment(checkout, name)
                .map_err(|error| EnvError::Invalid(error.to_string()))?,
        })
    }

    /// The environment of a resolved policy. Prepare must already have
    /// established the checkout's identity token.
    pub fn for_policy(policy: &crate::EffectiveContainerPolicy) -> Result<Self, EnvError> {
        let checkout = crate::policy::render::checkout(policy)
            .map_err(|error| EnvError::Invalid(error.to_string()))?;
        Self::new(checkout, &policy.name)
    }
}

/// The operations that own `Gate(Env)`. Each names its recovery command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Up,
    Down,
    Reset,
    Handover,
    Reap,
    Import,
}

impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "environment-up",
            Self::Down => "environment-down",
            Self::Reset => "environment-reset",
            Self::Handover => "environment-handover",
            Self::Reap => "environment-reap",
            Self::Import => "environment-import",
        }
    }

    fn next(self, env: &str) -> Result<Invocation, EnvError> {
        let verb = match self {
            Self::Down | Self::Reset | Self::Handover | Self::Reap => "down",
            Self::Up | Self::Import => "status",
        };
        let mut argv = vec![
            OsString::from("effigy"),
            OsString::from("container"),
            OsString::from(verb),
        ];
        if !env.is_empty() {
            argv.push(OsString::from(env));
        }
        Invocation::validated(argv, |argv| {
            argv.len() >= 3
                && argv[0] == "effigy"
                && argv[1] == "container"
                && matches!(argv[2].to_str(), Some("down" | "status"))
        })
        .map_err(|_| EnvError::Invalid("invalid recovery command".into()))
    }
}

/// The host kernel handles an environment run needs.
#[derive(Debug)]
pub struct EnvContext<'r> {
    pub root: &'r StateRoot,
    pub cx: EffectContext,
    cleanup_budget: Duration,
}

impl<'r> EnvContext<'r> {
    pub fn new(root: &'r StateRoot, cancel: CancelToken) -> Result<Self, EnvError> {
        Self::with_cleanup_budget(root, cancel, CLEANUP_BUDGET)
    }

    /// The same context with a shorter settlement budget. A proof crosses the
    /// boundary the former one-shot budget had with this, instead of waiting
    /// a minute of wall clock.
    pub fn with_cleanup_budget(
        root: &'r StateRoot,
        cancel: CancelToken,
        cleanup_budget: Duration,
    ) -> Result<Self, EnvError> {
        let operation = operation(EffectClass::Mutate, cancel, RUN_BUDGET);
        let cx = operation
            .effects()
            .map_err(|_| EnvError::Invalid("Mutate capability refused".into()))?;
        // An unusable budget is refused when the context is opened, not once
        // the run has already changed something.
        operation
            .cleanup(cleanup_budget)
            .map_err(|_| EnvError::Invalid("cleanup budget overflow".into()))?;
        Ok(Self {
            root,
            cx,
            cleanup_budget,
        })
    }

    /// A child operation that cannot outlive a workspace publication budget.
    pub fn bounded_until(&self, deadline: Instant) -> Result<Self, EnvError> {
        if Instant::now() >= deadline {
            return Err(EnvError::Kernel(RunError::unavailable(
                "workspace publication deadline expired",
            )));
        }
        Ok(Self {
            root: self.root,
            cx: self.cx.child_until(deadline),
            cleanup_budget: self.cleanup_budget,
        })
    }

    /// A fresh bounded cleanup context, made when the settlement that uses it
    /// starts. Each admission, holder settlement and run settle takes its own,
    /// so activation time can never spend it (effigy#447).
    pub fn cleanup(&self) -> Result<CleanupContext, EnvError> {
        cleanup(&self.cx, self.cleanup_budget)
    }

    /// The bounded settlement budget a caller renews for itself.
    pub(super) fn cleanup_budget(&self) -> Duration {
        self.cleanup_budget
    }

    pub fn observe(&self) -> effigy_kernel::ctx::ObserveContext {
        operation(
            EffectClass::Observe,
            CancelToken::default(),
            Duration::from_secs(10),
        )
        .observe()
        .expect("observe context")
    }
}

/// The bounded cleanup context every settlement begins with.
pub(crate) fn cleanup(cx: &EffectContext, budget: Duration) -> Result<CleanupContext, EnvError> {
    cx.operation()
        .cleanup(budget)
        .map_err(|_| EnvError::Invalid("cleanup budget overflow".into()))
}

pub(crate) fn operation(
    class: EffectClass,
    cancel: CancelToken,
    budget: Duration,
) -> OperationContext {
    let placement =
        if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
            Placement::Guest
        } else {
            Placement::Host
        };
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        cancel,
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        class,
        placement,
    )
}

/// One sentence for a refused transition, naming the holder the gate itself
/// reported: its pid and task when the gate recorded them (effigy#313), and
/// what the caller can do about it. Nothing here decides anything, and an
/// unreadable diagnostic is simply not named.
pub(super) fn busy_text(
    env: &str,
    holder: &Observed<HolderDiag>,
    waited: Option<Duration>,
) -> String {
    let owner = describe_gate_holder(holder);
    let mut text = format!("environment `{env}` is busy: {owner} is transitioning it");
    if let Some(waited) = waited {
        text.push_str(&format!(
            ", and the wait ended after {:.1}s",
            waited.as_secs_f64()
        ));
    }
    text.push_str(&format!(
        ". Nothing was changed and no backend was contacted; inspect it with `effigy container {env} status`, then retry"
    ));
    text
}

/// What the gate's own body says about its holder: the run, pid, task and the
/// holder's current process state. Diagnostics only (I-G2), and the same words
/// whether they are read during a wait or in the refusal that ends it.
fn describe_gate_holder(holder: &Observed<HolderDiag>) -> String {
    match holder {
        Observed::Verified(diag, _) => {
            let mut facts = vec![format!("pid {}", diag.pid)];
            if let Some(foreground) = &diag.foreground {
                facts.push(format!("task `{}`", foreground_task(&foreground.task)));
            }
            match &diag.state {
                Observed::Verified(ProcessState::Active, _) => {}
                Observed::Verified(ProcessState::Stopped, _) => {
                    facts.push("stopped (SIGSTOP)".to_owned())
                }
                Observed::Verified(ProcessState::Zombie, _) => facts.push("exited".to_owned()),
                _ => facts.push("state unreadable".to_owned()),
            }
            let run = match &diag.run {
                Some(run) => format!("run {run}"),
                None => "an unrecorded run".to_owned(),
            };
            format!("{run} ({})", facts.join(", "))
        }
        Observed::Absent(_) => "a run whose gate body is absent".to_owned(),
        Observed::Unknown(reason) => format!(
            "a run whose gate body is unreadable ({})",
            unknown_label(reason)
        ),
    }
}

/// One gate observation as the live wait's latest event: the exact holder the
/// gate names, or that it is free. The wait subject is the environment, so the
/// frame reads `waiting for Gate(Env) on `stack`; last: run … still holds it`.
fn publish_gate_state(source: &ProgressSource, env: &str, state: &Observed<GateState>) {
    let event = match state {
        Observed::Verified(GateState::Free, _) => "the gate is free".to_owned(),
        Observed::Verified(GateState::Held(holder), _) => {
            format!("{} still holds it", describe_gate_holder(holder))
        }
        Observed::Absent(_) => "the gate has no holder yet".to_owned(),
        Observed::Unknown(reason) => {
            format!("the gate state is unreadable ({})", unknown_label(reason))
        }
    };
    source.update(
        format!("waiting for Gate(Env) on `{env}`"),
        None,
        Some(event),
    );
}

/// Reads `Gate(Env)` with a momentary shared lock: no create, no authority and
/// nothing retained.
fn observe_gate(root: &StateRoot, resource: &ResourceId) -> Observed<GateState> {
    let cx = operation(
        EffectClass::Observe,
        CancelToken::default(),
        Duration::from_secs(10),
    )
    .observe()
    .expect("observe context");
    Gate::probe(&cx, root, resource)
}

/// The wake-up between gate probes: an admission that ends at once wakes the
/// probe instead of leaving it to notice after a poll interval.
#[derive(Default)]
struct GateProbeSignal {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl GateProbeSignal {
    fn publish_stopped(&self) {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.wake.notify_all();
    }

    /// Whether the admission has finished, waiting at most one probe interval.
    fn stopped(&self) -> bool {
        let stopped = self.stopped.lock().unwrap_or_else(PoisonError::into_inner);
        let (stopped, _) = self
            .wake
            .wait_timeout_while(stopped, GATE_PROBE_INTERVAL, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *stopped
    }
}

/// Stops the gate-wait probe, including when the admission it watches never
/// returns because the caller unwound.
struct GateProbeStop(Arc<GateProbeSignal>);

impl Drop for GateProbeStop {
    fn drop(&mut self) {
        self.0.publish_stopped();
    }
}

/// Runs `admit` under a live probe of `Gate(Env)` (effigy#461).
///
/// The probe only reads: it takes the gate's momentary shared lock, publishes
/// what the body records, and stops when the admission returns.
fn admit_under_gate_probe<T>(
    root: &StateRoot,
    env: &EnvRef,
    source: &ProgressSource,
    admit: impl FnOnce() -> T,
) -> T {
    let signal = Arc::new(GateProbeSignal::default());
    std::thread::scope(|scope| {
        scope.spawn(|| loop {
            publish_gate_state(source, &env.name, &observe_gate(root, &env.resource));
            if signal.stopped() {
                return;
            }
        });
        let _stop = GateProbeStop(Arc::clone(&signal));
        admit()
    })
}

/// The record's holder rows in the operator's words: how many runs are
/// attached and which of them are live, so a deferral or refusal names its
/// blocker before it happens. Diagnostics only.
fn attached_summary(holders: &[Holder]) -> String {
    if holders.is_empty() {
        return "no run is attached to it".to_owned();
    }
    let live: Vec<String> = holders
        .iter()
        .filter(|holder| matches!(holder.liveness, Observed::Verified(Liveness::Live, _)))
        .map(|holder| holder.run.to_string())
        .collect();
    if live.is_empty() {
        return format!("{} attached run(s) are interrupted", holders.len());
    }
    const NAMED: usize = 3;
    let mut names = live
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if live.len() > NAMED {
        names.push_str(&format!(" and {} more", live.len() - NAMED));
    }
    format!("{} live run(s) hold it: {names}", live.len())
}

/// The backend effect's own names, from the payload every transition journals.
fn backend_event(payload: &Value) -> String {
    format!(
        "engine {}, context `{}`, project `{}`",
        payload["engine"].as_str().unwrap_or("unnamed"),
        payload["context"].as_str().unwrap_or("unnamed"),
        payload["project"].as_str().unwrap_or("unnamed"),
    )
}

/// Closed vocabulary: the category only, never the free-form detail, which
/// can name paths.
fn unknown_label(reason: &UnknownReason) -> &'static str {
    match reason {
        UnknownReason::TimedOut { .. } => "timed-out",
        UnknownReason::Busy { .. } => "busy",
        UnknownReason::Cancelled(_) => "cancelled",
        UnknownReason::PermissionDenied => "permission-denied",
        UnknownReason::ToolUnavailable(_) => "tool-unavailable",
        UnknownReason::Unparsable(_) => "unparsable",
        UnknownReason::Inconsistent(_) => "inconsistent",
        UnknownReason::GuardUnavailable => "guard-unavailable",
        UnknownReason::Injected => "injected",
        UnknownReason::Untrusted(_) => "untrusted",
        UnknownReason::HostAuthorityRequired => "host-authority-required",
        UnknownReason::StoreUnreadable(_) => "store-unreadable",
    }
}

fn foreground_task(task: &TaskId) -> String {
    match task.catalog() {
        "" => task.task().to_owned(),
        catalog => format!("{catalog}/{}", task.task()),
    }
}

#[derive(Debug)]
pub enum EnvError {
    /// Another transition holds `Gate(Env)`: one sentence naming the holder
    /// the refusal observed.
    Busy(String),
    /// Live written holders protect the environment (R4).
    HeldBy {
        env: String,
        holders: Vec<RunId>,
    },
    /// A guest never holds host authority (R1).
    HostAuthorityRequired,
    /// A previous run of this environment left unresolved effects (K5).
    Unsettled(String),
    Backend(BackendError),
    Invalid(String),
    Kernel(RunError),
    /// Preserve the original refusal when cleanup also needs recovery.
    Cleanup {
        cause: Box<EnvError>,
        failures: Vec<EnvError>,
    },
}

impl std::fmt::Display for EnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy(text) => f.write_str(text),
            Self::HeldBy { env, holders } => {
                let count = holders.len();
                write!(
                    f,
                    "refused to stop, restart or recreate environment `{env}` while {count} run{} hold{} it ({}); stop it with `effigy run stop <id>` or `effigy container {env} status` shows each run, then retry",
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "s" } else { "" },
                    holders
                        .iter()
                        .map(|run| run.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
            Self::HostAuthorityRequired => f.write_str(
                "environment changes need host authority: run this on the host, not inside the workspace container",
            ),
            Self::Unsettled(detail) => write!(
                f,
                "the environment's previous transition left effects that could not be proven settled: {detail}. Inspect with `effigy container status`"
            ),
            Self::Backend(error) => write!(f, "{error}"),
            Self::Invalid(message) => f.write_str(message),
            Self::Kernel(error) => write!(f, "environment record: {error}"),
            Self::Cleanup { cause, failures } => {
                write!(f, "{cause}")?;
                for error in failures { write!(f, "; cleanup requires recovery: {error}")?; }
                Ok(())
            }
        }
    }
}

impl std::error::Error for EnvError {}

impl EnvError {
    /// A refusal that retained no gate evidence keeps its own variant,
    /// distinct from `HeldBy` and from kernel journal contention.
    pub(crate) fn from_run(env: &str, error: RunError) -> Self {
        match error {
            RunError::Gate(GateError::Busy { .. } | GateError::TimedOut { .. }) => Self::Busy(
                format!(
                    "environment `{env}` is busy: another transition holds its gate. Nothing was changed; inspect it with `effigy container {env} status`, then retry"
                ),
            ),
            RunError::Gate(GateError::HostAuthorityRequired) => Self::HostAuthorityRequired,
            RunError::HeldBy(holders) => Self::HeldBy {
                env: env.to_owned(),
                holders: holders.into_iter().map(|holder| holder.run).collect(),
            },
            RunError::PredecessorUnsettled { run, effects } => Self::Unsettled(format!(
                "run {run} left {} unresolved effect(s)",
                effects.len()
            )),
            other => Self::Kernel(other),
        }
    }
}

/// A refused admission onto `Gate(Env)`: names the holder the kernel's own
/// gate evidence recorded. Nothing is acquired, retried, waited for again or
/// released here.
fn admission_error(env: &EnvRef, wait: Wait, started: Instant, error: RunError) -> EnvError {
    match error {
        RunError::Gate(GateError::Busy { holder, .. }) => {
            EnvError::Busy(busy_text(&env.name, &holder, None))
        }
        RunError::Gate(GateError::TimedOut { holder, waited, .. }) => {
            EnvError::Busy(busy_text(&env.name, &holder, Some(waited)))
        }
        other => {
            let _ = (wait, started);
            EnvError::from_run(&env.name, other)
        }
    }
}

/// Environment effects settle by disposition only: an apply is retained and
/// a stop is irreversible. Any other effect is not this engine's: it stays
/// residue rather than being guessed away (R3).
#[derive(Debug, Default)]
pub struct EnvSettlement;

impl SettlementAdapter for EnvSettlement {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "no environment adapter settles this effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "no environment adapter settles this effect".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "no environment adapter settles this effect".into(),
        ))
    }
}

/// One operation holding `Gate(Env)`.
#[derive(Debug)]
pub struct EnvRun<'r> {
    pub(super) run: Run<'r>,
    cx: EffectContext,
    root: &'r StateRoot,
    env: EnvRef,
    cleanup_budget: Duration,
    began: EnvRecord,
    published: bool,
    failed: bool,
    /// A backend effect was journalled: the run changed the environment.
    transitioned: bool,
    /// The operator's live view of this transition: the phase now running,
    /// how far through it is and the last thing it observed (effigy#461).
    progress: ProgressSource,
    step: Step,
}

/// The exact environment run and its progress state while Env is free or
/// while its reacquired gate is awaiting validated evidence.
#[derive(Debug)]
pub struct ParkedEnvRun<'r> {
    gate: ParkedEnvGate<'r>,
    parts: ParkedEnvParts<'r>,
}

pub(super) struct EnvAdmissionFailure<'r> {
    pub error: EnvError,
    pub owner: Option<EnvAdmissionOwner<'r>>,
}

pub(super) enum EnvAdmissionOwner<'r> {
    Run(Box<Run<'r>>),
    Guarded(Box<effigy_kernel::run::GuardedWorkspaceCleanup<'r>>),
}

impl From<EnvError> for EnvAdmissionFailure<'_> {
    fn from(error: EnvError) -> Self {
        Self { error, owner: None }
    }
}

#[derive(Debug)]
enum ParkedEnvGate<'r> {
    Parked(Box<effigy_kernel::run::ParkedWorkspaceRun<'r>>),
    Guarded(Box<effigy_kernel::run::ReacquiredWorkspaceRun<'r>>),
}

#[derive(Debug)]
struct ParkedEnvParts<'r> {
    cx: EffectContext,
    root: &'r StateRoot,
    env: EnvRef,
    cleanup_budget: Duration,
    began: EnvRecord,
    published: bool,
    failed: bool,
    transitioned: bool,
    progress: ProgressSource,
    step: Step,
    receipt: super::CreationReceipt,
    target: effigy_kernel::workspace_session::ContainerIdentity,
    plan: effigy_kernel::workspace_session::ActivationPlan,
}

impl<'r> ParkedEnvRun<'r> {
    pub(super) fn repark(self) -> Result<Self, (EnvError, Box<Self>)> {
        let Self { gate, parts } = self;
        match gate {
            ParkedEnvGate::Parked(run) => Ok(Self {
                gate: ParkedEnvGate::Parked(run),
                parts,
            }),
            ParkedEnvGate::Guarded(run) => match run.repark() {
                Ok(run) => Ok(Self {
                    gate: ParkedEnvGate::Parked(Box::new(run)),
                    parts,
                }),
                Err((error, run)) => Err((
                    EnvError::Kernel(error),
                    Box::new(Self {
                        gate: ParkedEnvGate::Guarded(run),
                        parts,
                    }),
                )),
            },
        }
    }

    pub(super) fn integration_snapshot(
        &self,
        session: &Run<'_>,
    ) -> Result<
        std::collections::BTreeMap<
            effigy_kernel::run::WorkspaceIntegrationStep,
            effigy_kernel::run::WorkspaceIntegrationState,
        >,
        EnvError,
    > {
        let cleanup = cleanup(&self.parts.cx, self.parts.cleanup_budget)?;
        let cx = cleanup.operation().effects().map_err(|error| {
            EnvError::Invalid(format!("cleanup authority unavailable: {error}"))
        })?;
        match &self.gate {
            ParkedEnvGate::Parked(run) => run
                .integration_snapshot(session, &cx)
                .map_err(EnvError::Kernel),
            ParkedEnvGate::Guarded(_) => Err(EnvError::Invalid(
                "workspace integration requires Env to be free".into(),
            )),
        }
    }

    pub(super) fn record_integration_step(
        &self,
        session: &mut Run<'_>,
        cx: &EffectContext,
        step: effigy_kernel::run::WorkspaceIntegrationStep,
        state: effigy_kernel::run::WorkspaceIntegrationState,
    ) -> Result<(), EnvError> {
        match &self.gate {
            ParkedEnvGate::Parked(run) => run
                .record_integration_step(session, cx, step, state)
                .map_err(EnvError::Kernel),
            ParkedEnvGate::Guarded(_) => Err(EnvError::Invalid(
                "workspace integration requires Env to be free".into(),
            )),
        }
    }

    pub(super) fn owns_root(&self, root: &StateRoot) -> bool {
        std::ptr::eq(self.parts.root, root)
    }

    pub(super) fn settle(
        self,
        cleanup: &CleanupContext,
    ) -> Result<CleanupOutcome, (EnvError, effigy_kernel::run::WorkspaceCleanupCustody<'r>)> {
        use effigy_kernel::run::WorkspaceCleanupCustody;
        match self.gate {
            ParkedEnvGate::Parked(run) => run.settle(cleanup, &mut EnvSettlement),
            ParkedEnvGate::Guarded(run) => run
                .settle(cleanup, &mut EnvSettlement)
                .map_err(|(error, custody)| (error, WorkspaceCleanupCustody::Guarded(custody))),
        }
        .map(|outcome| outcome.cleanup())
        .map_err(|(error, custody)| (EnvError::Kernel(error), custody))
    }

    pub(super) fn reacquire(
        self,
        session: &Run<'_>,
        backend: &mut dyn super::EnvironmentBackend,
        observe: impl FnOnce(
            &effigy_kernel::run::WorkspaceResumeView<'_>,
            Instant,
            &mut dyn super::EnvironmentBackend,
        ) -> Observed<effigy_kernel::run::WorkspaceResumeFacts>,
    ) -> Result<EnvRun<'r>, (EnvError, Box<Self>)> {
        let Self { gate, parts } = self;
        let guarded = match gate {
            ParkedEnvGate::Parked(run) => match run.reacquire() {
                Ok(guarded) => Box::new(guarded),
                Err((error, parked)) => {
                    return Err((
                        EnvError::Kernel(error),
                        Box::new(Self {
                            gate: ParkedEnvGate::Parked(parked),
                            parts,
                        }),
                    ))
                }
            },
            ParkedEnvGate::Guarded(guarded) => guarded,
        };
        let observation = (|| -> Result<_, EnvError> {
            let view = guarded.view().map_err(EnvError::Kernel)?;
            let facts = view.facts();
            let mut ids = parts
                .receipt
                .created_ids()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            ids.sort();
            if facts.target != parts.target
                || facts.session != *parts.plan.binding()
                || parts.plan.environment_run() != Some(facts.environment_run)
                || facts.reservation != parts.plan.reservation_id()
                || facts.environment != *parts.plan.environment()
                || facts.environment_generation != parts.plan.environment_generation()
                || facts.created_ids != ids
            {
                return Err(EnvError::Invalid(
                    "parked workspace evidence differs from its owner snapshot".into(),
                ));
            }
            // Decode the entire v5 record through #510's owner, then compare
            // the reconstructed receipt with the one retained before parking.
            let stored = super::creation::decode_stored(&facts.creator_receipt)
                .map_err(EnvError::Invalid)?;
            let decoded = super::creation::outcome(
                &parts.env.resource,
                facts.environment_run,
                stored.record().clone(),
            );
            if decoded.receipt() != Some(&parts.receipt) {
                return Err(EnvError::Invalid(
                    "workspace creator receipt changed while parked".into(),
                ));
            }
            parts
                .receipt
                .reobserve(backend, view.deadline())
                .map_err(|error| EnvError::Backend(BackendError::Uncertain(error.to_string())))?;
            let observed = observe(&view, view.deadline(), backend);
            parts
                .receipt
                .reobserve(backend, view.deadline())
                .map_err(|error| EnvError::Backend(BackendError::Uncertain(error.to_string())))?;
            Ok(observed)
        })();
        let observed = match observation {
            Ok(observed) => observed,
            Err(error) => {
                return Err((
                    error,
                    Box::new(Self {
                        gate: ParkedEnvGate::Guarded(guarded),
                        parts,
                    }),
                ))
            }
        };
        let run = match guarded.resume(session, observed) {
            Ok(run) => run,
            Err((error, guarded)) => {
                return Err((
                    EnvError::Kernel(error),
                    Box::new(Self {
                        gate: ParkedEnvGate::Guarded(guarded),
                        parts,
                    }),
                ))
            }
        };
        Ok(EnvRun {
            run,
            cx: parts.cx,
            root: parts.root,
            env: parts.env,
            cleanup_budget: parts.cleanup_budget,
            began: parts.began,
            published: parts.published,
            failed: parts.failed,
            transitioned: parts.transitioned,
            progress: parts.progress,
            step: parts.step,
        })
    }
}

impl<'r> EnvRun<'r> {
    pub(super) fn park_for_cleanup(
        self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        session: &Run<'_>,
        cleanup: &effigy_kernel::ctx::CleanupContext,
    ) -> Result<effigy_kernel::run::ParkedWorkspaceRun<'r>, (EnvError, Box<Self>)> {
        let Self {
            run,
            cx,
            root,
            env,
            cleanup_budget,
            began,
            published,
            failed,
            transitioned,
            progress,
            step,
        } = self;
        match run.park_workspace_for_cleanup(reservation, session, cleanup) {
            Ok(parked) => Ok(parked),
            Err((error, run)) => Err((
                EnvError::Kernel(error),
                Box::new(Self {
                    run: *run,
                    cx,
                    root,
                    env,
                    cleanup_budget,
                    began,
                    published,
                    failed,
                    transitioned,
                    progress,
                    step,
                }),
            )),
        }
    }

    /// Park only after target binding and a complete journalled creator receipt.
    /// Any refusal returns the still-guarded owner for explicit settlement.
    pub(super) fn park_workspace(
        mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        session: &effigy_kernel::workspace_session::SessionRef,
        deadline: Instant,
    ) -> Result<ParkedEnvRun<'r>, (EnvError, Box<Self>)> {
        let deadline = self.observation_deadline(deadline);
        let snapshot = (|| {
            if Instant::now() >= deadline {
                return Err(EnvError::Invalid(
                    "workspace activation deadline expired".into(),
                ));
            }
            let plan = self.workspace_plan_until(reservation, deadline)?;
            if &plan.session() != session {
                return Err(EnvError::Invalid(
                    "activation session changed before parking".into(),
                ));
            }
            let target = self
                .run
                .workspace_bound_target(reservation)
                .map_err(EnvError::Kernel)?;
            let receipt = self
                .journalled_creation()?
                .and_then(|outcome| outcome.receipt().cloned())
                .ok_or_else(|| {
                    EnvError::Invalid("activation has no complete creator receipt".into())
                })?;
            if receipt.workspace_id() != Some(target.container_id.as_str()) {
                return Err(EnvError::Invalid(
                    "bound target differs from the creator receipt".into(),
                ));
            }
            if Instant::now() >= deadline {
                return Err(EnvError::Invalid(
                    "workspace activation deadline expired".into(),
                ));
            }
            Ok::<_, EnvError>((plan, target, receipt))
        })();
        let (plan, target, receipt) = match snapshot {
            Ok(value) => value,
            Err(error) => return Err((error, Box::new(self))),
        };
        let Self {
            run,
            cx,
            root,
            env,
            cleanup_budget,
            began,
            published,
            failed,
            transitioned,
            progress,
            step,
        } = self;
        match run.park_workspace(reservation, deadline) {
            Ok(run) => Ok(ParkedEnvRun {
                gate: ParkedEnvGate::Parked(Box::new(run)),
                parts: ParkedEnvParts {
                    cx,
                    root,
                    env,
                    cleanup_budget,
                    began,
                    published,
                    failed,
                    transitioned,
                    progress,
                    step,
                    receipt,
                    target,
                    plan,
                },
            }),
            Err((error, run)) => Err((
                EnvError::Kernel(error),
                Box::new(Self {
                    run: *run,
                    cx,
                    root,
                    env,
                    cleanup_budget,
                    began,
                    published,
                    failed,
                    transitioned,
                    progress,
                    step,
                }),
            )),
        }
    }
    /// Admits the operation: takes `Gate(Env)`, settles an interrupted
    /// predecessor (K5) and commits `Admitted` before any effect (K3).
    pub fn begin(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
    ) -> Result<Self, EnvError> {
        Self::admit(ctx, env, op, wait, None)
    }

    /// Admits the operation at the generation a workspace activation reserved
    /// (§8.7). The reservation is bound to this run in the admission
    /// transaction, so this is the only admission that may take a generation a
    /// workspace activation reserved; a competing activation is refused by name
    /// instead of taking it.
    pub fn begin_claimed(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<Self, EnvError> {
        let claim = reservation
            .claim()
            .map_err(|error| EnvError::from_run(&env.name, error))?;
        Self::admit(ctx, env, op, wait, Some(claim))
    }

    pub(super) fn begin_claimed_recoverable(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<Self, EnvAdmissionFailure<'r>> {
        let claim = reservation
            .claim()
            .map_err(|error| EnvError::from_run(&env.name, error))?;
        Self::admit_recoverable(ctx, env, op, wait, Some(claim))
    }

    fn admit(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
        claim: Option<effigy_kernel::workspace_session::ActivationClaim>,
    ) -> Result<Self, EnvError> {
        Self::admit_recoverable(ctx, env, op, wait, claim).map_err(|failure| {
            if let Some(owner) = failure.owner {
                if let Ok(cleanup) = ctx.cleanup() {
                    match owner {
                        EnvAdmissionOwner::Run(run) => {
                            let _ = run.settle(&cleanup, &mut EnvSettlement);
                        }
                        EnvAdmissionOwner::Guarded(guarded) => {
                            let _ = guarded.settle(&cleanup, &mut EnvSettlement);
                        }
                    }
                }
            }
            failure.error
        })
    }

    fn admit_recoverable(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
        claim: Option<effigy_kernel::workspace_session::ActivationClaim>,
    ) -> Result<Self, EnvAdmissionFailure<'r>> {
        if ctx.cx.operation().placement() != Placement::Host {
            return Err(EnvError::HostAuthorityRequired.into());
        }
        let invocation = ResolvedInvocation::new(InvocationSpec {
            task: TaskId::new(env.checkout, "", Surface::Published, op.name(), None)
                .map_err(|error| EnvError::Invalid(error.to_string()))?,
            route: Route::Task,
            argv: vec![],
            cwd: String::new(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            owner: env.resource.clone(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .map_err(|error| EnvError::Invalid(error.to_string()))?;
        let id = new_run_id()?;
        let next = op.next(&env.name)?;
        // The operator watches one step per transition. The gate wait can last
        // up to ten minutes behind another transition, and silence reads as a
        // hang, so the wait names the gate and the probe reads the holder its
        // body records (effigy#461).
        let progress =
            ProgressSource::waiting_for(format!("environment `{}` to be free", env.name));
        let step = effigy_ui::steps::live_step(progress.clone());
        // The original deadline, kept for the refusal's own report.
        let started = Instant::now();
        // Admission settles an interrupted predecessor under a window this
        // call makes once the gate is held, so the wait above (up to ten
        // minutes) cannot spend it (effigy#447).
        let options = effigy_kernel::run::BeginOpts {
            id,
            wait,
            acknowledge: vec![],
            next,
            custody: None,
        };
        let admitted = admit_under_gate_probe(ctx.root, env, &progress, || match claim {
            Some(claim) => Run::begin_claimed_recoverable(
                &ctx.cx,
                ctx.root,
                &invocation,
                options,
                || {
                    ctx.cleanup()
                        .map_err(|error| RunError::broken(error.to_string()))
                },
                &mut EnvSettlement,
                claim,
            )
            .map_err(|(error, guarded)| EnvAdmissionFailure {
                error: admission_error(env, wait, started, error),
                owner: guarded.map(EnvAdmissionOwner::Guarded),
            }),
            None => Run::begin_with_fresh_cleanup(
                &ctx.cx,
                ctx.root,
                &invocation,
                options,
                || {
                    ctx.cleanup()
                        .map_err(|error| RunError::broken(error.to_string()))
                },
                &mut EnvSettlement,
            )
            .map_err(|error| EnvAdmissionFailure {
                error: admission_error(env, wait, started, error),
                owner: None,
            }),
        });
        let run = match admitted {
            Ok(run) => {
                progress.update(
                    "reading the environment record",
                    None,
                    Some(format!("run {} holds the environment gate", run.id())),
                );
                run
            }
            Err(error) => {
                step.fail(&format!("environment `{}` was not entered", env.name));
                return Err(error);
            }
        };
        // The gate is held: only this run writes the row until it settles.
        let began = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
            Observed::Verified(record, _) => record,
            other => {
                let reason = match other {
                    Observed::Unknown(reason) => format!("{reason}"),
                    _ => "row missing after admission".to_owned(),
                };
                step.fail(&format!(
                    "environment `{}` record is unreadable after admission",
                    env.name
                ));
                return Err(EnvAdmissionFailure {
                    error: EnvError::Invalid(format!(
                        "environment record unreadable after admission: {reason}"
                    )),
                    owner: Some(EnvAdmissionOwner::Run(Box::new(run))),
                });
            }
        };
        Ok(Self {
            run,
            cx: ctx.cx.clone(),
            root: ctx.root,
            env: env.clone(),
            cleanup_budget: ctx.cleanup_budget,
            began,
            published: false,
            failed: false,
            transitioned: false,
            progress,
            step,
        })
    }

    pub fn id(&self) -> RunId {
        self.run.id()
    }

    pub fn fence(&self) -> &Fence {
        self.run.fence()
    }

    pub fn env(&self) -> &EnvRef {
        &self.env
    }

    /// The record as this run found it (after admission, under the gate).
    pub fn record(&self) -> &EnvRecord {
        &self.began
    }

    pub fn engine_record(&self) -> Option<&EngineRecord> {
        self.began.engine()
    }

    /// Publishes the environment's render as the object it points at.
    pub(super) fn verify_compose_inputs(
        &self,
        closure: &Closure,
        files: &[std::path::PathBuf],
        workdir: &std::path::Path,
    ) -> Result<(), EnvError> {
        let expected_checkout = self
            .env
            .checkout
            .token()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let actual_checkout = effigy_core::worktree_scope::checkout_token(workdir)
            .map_err(|error| EnvError::Invalid(error.to_string()))?;
        if actual_checkout.as_deref() != Some(expected_checkout.as_str()) {
            return Err(EnvError::Invalid(
                "Compose workdir belongs to another checkout".into(),
            ));
        }
        let object = match Objects::resolve(
            &self
                .cx
                .operation()
                .observe()
                .map_err(|_| EnvError::Invalid("cannot observe sealed render".into()))?,
            self.root,
            &self.env.resource,
            effigy_kernel::objects::Role::Desired,
        ) {
            Observed::Verified(object, _) if object.closure.digest() == closure.digest() => object,
            _ => {
                return Err(EnvError::Invalid(
                    "sealed desired render is unavailable".into(),
                ))
            }
        };
        let metadata = closure
            .files()
            .get("inputs/environment.json")
            .ok_or_else(|| {
                EnvError::Invalid("sealed render has no Compose input inventory".into())
            })?;
        let metadata: serde_json::Value = serde_json::from_slice(metadata)
            .map_err(|error| EnvError::Invalid(error.to_string()))?;
        let names: Vec<String> =
            serde_json::from_value(metadata.get("compose_files").cloned().ok_or_else(|| {
                EnvError::Invalid("sealed render has no Compose input inventory".into())
            })?)
            .map_err(|error| EnvError::Invalid(error.to_string()))?;
        if names.is_empty()
            || names.iter().any(|name| !closure.files().contains_key(name))
            || files
                != names
                    .iter()
                    .map(|name| object.path.join(name))
                    .collect::<Vec<_>>()
        {
            return Err(EnvError::Invalid(
                "Compose input paths or order differ from the sealed closure".into(),
            ));
        }
        Ok(())
    }

    pub fn publish_desired(&mut self, closure: &Closure) -> Result<String, EnvError> {
        self.phase("publishing the desired render", None);
        let result = Objects::publish_desired(&mut self.run, closure);
        let id = self.kernel(result)?;
        self.progress.event(format!("published render {id}"));
        Ok(id)
    }

    /// Publish a preflight seal's closure as this environment's desired render.
    ///
    /// This is the same fenced publication as `publish_desired` — one journalled
    /// intent, one objects gate, one reference transaction — but it consumes the
    /// kernel receipt a preflight verified, which the publisher re-verifies under
    /// this run's `Gate(Env)` before promoting. No other route can turn a seal
    /// into canonical bytes.
    pub fn publish_sealed_desired(
        &mut self,
        sealed: &effigy_kernel::objects::SealedPreflight,
        closure: &Closure,
    ) -> Result<String, EnvError> {
        self.phase("publishing the sealed render", None);
        let result = Objects::publish_sealed_desired(&mut self.run, sealed, closure);
        let id = self.kernel(result)?;
        self.progress.event(format!("published sealed render {id}"));
        Ok(id)
    }

    /// Replaces the engine's fields on the record.
    pub fn write_record(&mut self, record: &EngineRecord) -> Result<(), EnvError> {
        self.phase("writing the environment record", None);
        self.progress.event(format!(
            "retention: {}, project `{}`",
            record.retention.label(),
            record.project
        ));
        let value =
            serde_json::to_value(record).map_err(|error| EnvError::Invalid(error.to_string()))?;
        let result = self.run.set_engine_record(&value);
        self.kernel(result)?;
        self.began.engine = Some(Observed::Verified(
            record.clone(),
            super::record::evidence(),
        ));
        Ok(())
    }

    /// Current holders, read fresh while this run holds the gate.
    pub fn holders(&self) -> Observed<Vec<Holder>> {
        let cx = operation(
            EffectClass::Observe,
            CancelToken::default(),
            Duration::from_secs(10),
        )
        .observe()
        .expect("observe context");
        let observed = match EnvRecord::read(&cx, self.root, &self.env.resource) {
            Observed::Verified(record, _) => record.holders,
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        };
        // The attach check is the refusal's own evidence: the operator sees a
        // live holder named here before the permit refuses (effigy#461).
        let event = match &observed {
            Observed::Verified(holders, _) => attached_summary(holders),
            Observed::Absent(_) => "no environment record names a holder".to_owned(),
            Observed::Unknown(reason) => {
                format!("the holder list is unreadable ({})", unknown_label(reason))
            }
        };
        self.phase(
            format!(
                "checking the runs attached to environment `{}`",
                self.env.name
            ),
            None,
        );
        self.progress.event(event);
        observed
    }

    /// Settles every interrupted holder as this transition's gate holder
    /// (I-A2): its owner gate is taken `FailFast`, so a live successor that
    /// is settling it already is left alone. Live and unknown holders stay.
    pub fn settle_interrupted_holders(&mut self) -> Result<(), EnvError> {
        let Observed::Verified(holders, _) = self.holders() else {
            return Ok(());
        };
        let cx = operation(
            EffectClass::Observe,
            CancelToken::default(),
            Duration::from_secs(10),
        )
        .observe()
        .expect("observe context");
        let reader = match effigy_kernel::store::Store::read(&cx, self.root) {
            Observed::Verified(reader, _) => reader,
            _ => return Ok(()),
        };
        let interrupted = holders
            .into_iter()
            .filter(|holder| {
                matches!(
                    holder.liveness,
                    Observed::Verified(Liveness::Interrupted, _)
                )
            })
            .collect::<Vec<_>>();
        let total = interrupted.len();
        for (index, holder) in interrupted.into_iter().enumerate() {
            self.phase(
                format!(
                    "settling interrupted run {} ({} of {total})",
                    holder.run,
                    index + 1
                ),
                Some((index as u64, total as u64)),
            );
            let Observed::Verified(summary, _) = effigy_kernel::run::summary(&reader, holder.run)
            else {
                continue;
            };
            let Some(owner) = summary
                .invocation
                .map(|invocation| invocation.spec().owner.clone())
            else {
                continue;
            };
            let next = Invocation::validated(
                vec![
                    OsString::from("effigy"),
                    OsString::from("tasks"),
                    OsString::from("status"),
                    OsString::from("--all"),
                ],
                |_| true,
            )
            .map_err(|_| EnvError::Invalid("invalid recovery command".into()))?;
            // A holder settlement is its own bounded cleanup: it is made now,
            // not when the environment run opened (effigy#447).
            let cleanup = self.cleanup()?;
            match effigy_kernel::run::settle_interrupted(
                &self.cx,
                self.root,
                &owner,
                holder.run,
                next,
                &cleanup,
                &mut EnvSettlement,
            ) {
                // A successor holds the owner gate and settles it first (K5).
                Ok(_) | Err(RunError::Gate(_)) => {}
                Err(error) => return Err(EnvError::from_run(&self.env.name, error)),
            }
        }
        Ok(())
    }

    /// Journal one ordinary Compose apply and its complete gate-scoped ID
    /// difference. An incomplete observation produces no attribution receipt.
    pub fn create_with(
        &mut self,
        backend: &mut dyn super::backend::EnvironmentBackend,
        selection: &BackendSelection,
        request: super::creation::CreationRequest<'_>,
        apply: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<super::creation::CreationOutcome, EnvError> {
        self.create_with_owned(backend, selection, request, |_, _, _| apply())
    }

    pub(super) fn create_with_owned(
        &mut self,
        backend: &mut dyn super::backend::EnvironmentBackend,
        selection: &BackendSelection,
        request: super::creation::CreationRequest<'_>,
        apply: impl FnOnce(&mut Run<'_>, &EffectContext, Instant) -> Result<(), BackendError>,
    ) -> Result<super::creation::CreationOutcome, EnvError> {
        use super::creation::{self, RecordedCreation};
        let mut request = request;
        if let Deadline::At(deadline) = self.cx.operation().deadline() {
            request.deadline = request.deadline.min(deadline);
        }
        let env = self.env.name.clone();
        self.settle_interrupted_holders()?;
        let checked = permit(&mut self.run, None).map(|_| ());
        self.kernel(checked)?;
        if let Some(outcome) = self.journalled_creation()? {
            return Ok(outcome);
        }
        let payload = json!({
            "engine": selection.engine.as_str(),
            "context": selection.context,
            "project": request.project,
            "primary_service": request.primary_service,
        });
        let mut transition_permit = permit(&mut self.run, None).map_err(|error| {
            self.failed = true;
            EnvError::from_run(&env, error)
        })?;
        let event = backend_event(&payload);
        let spec = EffectSpec {
            target: transition_permit.fence().clone(),
            kind: creation::CREATE_EFFECT.to_owned(),
            payload,
            settlement: Settlement::Retain,
            required: true,
        };
        let ticket = match transition_permit.intend(spec) {
            Ok(ticket) => {
                self.transitioned = true;
                self.phase(APPLY_PHASE, None);
                self.progress.event(event);
                ticket
            }
            Err(error) => {
                self.failed = true;
                return Err(EnvError::from_run(&env, error));
            }
        };
        // Both observations and the one apply run inside this EnvRun's gate.
        // An incomplete window can never mint an attribution receipt.
        let window = if request.required {
            creation::CreationWindow::open(backend, request)
        } else {
            Err(super::CreationRefusal::Unknown(
                "ordinary apply requested no sealed workspace attribution".into(),
            ))
        };
        let checked = permit(&mut self.run, None).map(|_| ());
        self.kernel(checked)?;
        if Instant::now() >= request.deadline {
            self.failed = true;
            return Err(EnvError::Backend(BackendError::NotStarted(
                "apply deadline expired during pre-observation".into(),
            )));
        }
        if request.required {
            if let Err(reason) = &window {
                let message = reason.to_string();
                let recorded = self.run.failed(
                    &ticket,
                    AbsenceProof::NoFile {
                        name: message.clone(),
                    },
                );
                self.kernel(recorded)?;
                self.failed = true;
                return Err(EnvError::Backend(BackendError::NotStarted(message)));
            }
        }
        let outcome = match fault("EnvBackendBefore") {
            Err(error) => Err(BackendError::NotStarted(error.to_string())),
            Ok(()) => {
                let outcome = apply(&mut self.run, &self.cx, request.deadline);
                match (outcome, fault("EnvBackendAfter")) {
                    (Ok(()), Err(error)) => Err(BackendError::Uncertain(error.to_string())),
                    (outcome, _) => outcome,
                }
            }
        };
        self.phase(APPLY_PHASE, None);
        if let Err(error) = outcome {
            self.progress.event(match &error {
                BackendError::NotStarted(message) => {
                    format!("the backend was never contacted: {message}")
                }
                BackendError::Uncertain(message) => {
                    format!("the backend's answer is unproven: {message}")
                }
            });
            let recorded = match &error {
                BackendError::NotStarted(message) => self.run.failed(
                    &ticket,
                    AbsenceProof::NoFile {
                        name: message.clone(),
                    },
                ),
                BackendError::Uncertain(message) => self
                    .run
                    .unknown(&ticket, UnknownReason::Inconsistent(message.clone())),
            };
            if let Err(error) = recorded {
                self.failed = true;
                return Err(EnvError::from_run(&env, error));
            }
            self.failed = true;
            return Err(EnvError::Backend(error));
        }
        let checked = permit(&mut self.run, None).map(|_| ());
        self.kernel(checked)?;
        let record = match window.and_then(|window| window.close(backend, request)) {
            Ok(creation::InventoryDifference {
                expected_created,
                workspace_predecessor,
                image_identities,
                reference_resolutions,
                compose_files,
                created,
                attached,
                observed,
            }) => RecordedCreation::Attributed(Box::new(creation::AttributedCreation {
                engine: selection.engine.as_str().to_owned(),
                context: selection.context.clone(),
                project: request.project.to_owned(),
                primary_service: request.primary_service.to_owned(),
                generation: self.fence().generation.get(),
                observed,
                workdir: request.workdir.to_path_buf(),
                expected_created,
                workspace_predecessor,
                image_identities,
                reference_resolutions,
                compose_files,
                created,
                attached,
            })),
            Err(reason) if request.required => {
                let message = reason.to_string();
                let recorded = self
                    .run
                    .unknown(&ticket, UnknownReason::Inconsistent(message.clone()));
                self.kernel(recorded)?;
                self.failed = true;
                return Err(EnvError::Backend(BackendError::Uncertain(message)));
            }
            Err(reason) => RecordedCreation::Unproven {
                project: request.project.to_owned(),
                reason: reason.to_string(),
            },
        };
        let recorded = creation::stored(record.clone())
            .map_err(|error| EnvError::Invalid(error.to_string()))?;
        let written = self
            .run
            .applied(&ticket, recorded)
            .and_then(|()| self.run.completed(&ticket, RequiredObligation::Applied));
        if let Err(error) = written {
            self.failed = true;
            return Err(EnvError::from_run(&env, error));
        }
        Ok(creation::outcome(&self.env.resource, self.run.id(), record))
    }

    /// The record this run already journalled, or nothing. A retry of one
    /// activation never runs a second apply, and a create without an applied
    /// result stays unproven rather than being re-invented from the engine.
    fn journalled_creation(
        &mut self,
    ) -> Result<Option<super::creation::CreationOutcome>, EnvError> {
        use super::creation;
        use effigy_kernel::run::EffectState;
        let journalled = self.kernel(self.run.effects())?;
        let Some(existing) = journalled
            .into_iter()
            .find(|effect| effect.spec.kind == creation::CREATE_EFFECT)
        else {
            return Ok(None);
        };
        match (existing.state, existing.receipt) {
            // A retained effect is an applied one whose settlement only recorded
            // that the stack outlives the run.
            (EffectState::Applied | EffectState::Retained, Some(receipt)) => {
                let stored = creation::decode_stored(&receipt).map_err(EnvError::Invalid)?;
                Ok(Some(creation::outcome(
                    &self.env.resource,
                    self.run.id(),
                    stored.record().clone(),
                )))
            }
            (state, _) => Err(EnvError::Backend(BackendError::Uncertain(format!(
                "this activation's create effect is {} and has no authenticated result; the next \
                 activation reconciles it instead of attributing a container this run never \
                 recorded creating",
                state.label()
            )))),
        }
    }

    /// The complete creator receipt already recorded by this exact run.
    pub fn workspace_creation_receipt(&mut self) -> Result<super::CreationReceipt, EnvError> {
        self.journalled_creation()?
            .and_then(|outcome| outcome.receipt().cloned())
            .ok_or_else(|| EnvError::Invalid("activation has no complete creator receipt".into()))
    }

    /// The one backend stop: journalled under a permit, then published
    /// `stopped` with a fresh holder check in the same transaction.
    pub fn stop_with(
        &mut self,
        selection: &BackendSelection,
        project: &str,
        exempt: Option<RunId>,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let payload = json!({
            "engine": selection.engine.as_str(),
            "context": selection.context,
            "project": project,
        });
        self.transition(
            STOP_EFFECT,
            "stopping the backend",
            payload,
            Settlement::Irreversible,
            exempt,
            effect,
        )?;
        let result = permit(&mut self.run, exempt).and_then(|mut permit| permit.publish_stopped());
        self.kernel(result)?;
        self.published = true;
        let record = self
            .engine_record()
            .cloned()
            .unwrap_or_else(|| EngineRecord::new(super::record::Retention::Held, project));
        self.write_record(&record)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &mut self,
        kind: &str,
        phase: &str,
        payload: Value,
        settlement: Settlement,
        exempt: Option<RunId>,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let env = self.env.name.clone();
        self.settle_interrupted_holders()?;
        // The permit is the attach check: a refusal keeps the live holder the
        // attach check just named, so the frame and the refusal agree.
        let mut permit = permit(&mut self.run, exempt).map_err(|error| {
            self.failed = true;
            EnvError::from_run(&env, error)
        })?;
        let event = backend_event(&payload);
        let spec = EffectSpec {
            target: permit.fence().clone(),
            kind: kind.to_owned(),
            payload,
            settlement,
            required: true,
        };
        let ticket = permit
            .intend(spec)
            .map_err(|error| EnvError::from_run(&env, error));
        let ticket = match ticket {
            Ok(ticket) => {
                self.transitioned = true;
                // The permit's borrow of the run ended with `intend`; the
                // phase publishes once the effect is journalled.
                self.phase(phase, None);
                self.progress.event(event);
                ticket
            }
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        let outcome = match fault("EnvBackendBefore") {
            // Nothing reached the backend: verified non-occurrence.
            Err(error) => Err(BackendError::NotStarted(error.to_string())),
            Ok(()) => {
                let outcome = effect();
                match (outcome, fault("EnvBackendAfter")) {
                    // The backend acted; only the journal callback was lost.
                    (Ok(()), Err(error)) => Err(BackendError::Uncertain(error.to_string())),
                    (outcome, _) => outcome,
                }
            }
        };
        // The permit's borrow of the run ends with its last use above;
        // callbacks go through the run itself.
        self.phase(phase, None);
        self.progress.event(match &outcome {
            Ok(()) if kind == STOP_EFFECT => "the backend stopped".to_owned(),
            Ok(()) => "the backend converged".to_owned(),
            Err(BackendError::NotStarted(message)) => {
                format!("the backend was never contacted: {message}")
            }
            Err(BackendError::Uncertain(message)) => {
                format!("the backend's answer is unproven: {message}")
            }
        });
        let recorded = match &outcome {
            Ok(()) => self
                .run
                .applied(&ticket, Value::Bool(true))
                .and_then(|()| self.run.completed(&ticket, RequiredObligation::Applied)),
            Err(BackendError::NotStarted(message)) => self.run.failed(
                &ticket,
                AbsenceProof::NoFile {
                    name: message.clone(),
                },
            ),
            Err(BackendError::Uncertain(message)) => self
                .run
                .unknown(&ticket, UnknownReason::Inconsistent(message.clone())),
        };
        if let Err(error) = recorded {
            self.failed = true;
            return Err(EnvError::from_run(&env, error));
        }
        outcome.map_err(|error| {
            self.failed = true;
            EnvError::Backend(error)
        })
    }

    /// One required, additive guest publication under this activation's owner
    /// gate. The intent commits before I/O; its receipt commits before active.
    pub fn publish_guest_with(
        &mut self,
        publish: impl FnOnce(
            &effigy_kernel::run::Fence,
        )
            -> Result<effigy_kernel::run::GuestPublicationReceipt, BackendError>,
    ) -> Result<(), EnvError> {
        let intent = Attachments::intend_guest_publication(&mut self.run);
        let ticket = self.kernel(intent)?;
        self.transitioned = true;
        let fence = self.run.fence().clone();
        match publish(&fence) {
            Ok(receipt) => {
                let recorded = Attachments::guest_published(&mut self.run, &ticket, receipt);
                self.kernel(recorded)
            }
            Err(error) => {
                self.failed = true;
                let recorded = Attachments::guest_publication_unknown(
                    &mut self.run,
                    &ticket,
                    UnknownReason::Inconsistent(error.to_string()),
                );
                self.kernel(recorded)?;
                Err(EnvError::Backend(error))
            }
        }
    }

    /// Read this held activation's plan from its own root and capability.
    pub fn workspace_plan(
        &self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<effigy_kernel::workspace_session::ActivationPlan, EnvError> {
        self.workspace_plan_until(reservation, Instant::now() + Duration::from_secs(30))
    }

    /// Read the target already bound by this exact reserved owner.
    pub fn workspace_bound_target(
        &self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<effigy_kernel::workspace_session::ContainerIdentity, EnvError> {
        self.run
            .workspace_bound_target(reservation)
            .map_err(EnvError::Kernel)
    }

    fn workspace_plan_until(
        &self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        deadline: Instant,
    ) -> Result<effigy_kernel::workspace_session::ActivationPlan, EnvError> {
        let observe = self
            .cx
            .child_until(deadline)
            .operation()
            .observe()
            .map_err(|_| {
                EnvError::Kernel(RunError::refused(
                    "workspace activation cannot observe its plan",
                ))
            })?;
        let reader = match effigy_kernel::store::Store::read(&observe, self.root) {
            Observed::Verified(reader, _) => reader,
            _ => {
                return Err(EnvError::Kernel(RunError::unavailable(
                    "workspace activation journal is unreadable",
                )))
            }
        };
        match reservation.plan(&reader) {
            Observed::Verified(Some(plan), _)
                if plan.environment_run() == Some(self.id()) && plan.fence() == *self.fence() =>
            {
                Ok(plan)
            }
            _ => Err(EnvError::Kernel(RunError::unavailable(
                "workspace activation plan is not bound to this owner",
            ))),
        }
    }

    /// Start the required publication before the caller's sequenced staging
    /// and channel I/O. The sealed ticket is still owned by this exact Env run.
    pub fn intend_workspace_publication(
        &mut self,
        plan: &effigy_kernel::workspace_session::ActivationPlan,
    ) -> Result<effigy_kernel::run::GuestPublicationTicket, EnvError> {
        if plan.environment_run() != Some(self.run.id()) || plan.fence() != *self.run.fence() {
            return self.kernel(Err(RunError::refused(
                "workspace publication plan names another activation",
            )));
        }
        let result = Attachments::intend_guest_publication(&mut self.run);
        let ticket = self.kernel(result)?;
        self.transitioned = true;
        Ok(ticket)
    }

    pub fn acknowledge_workspace_publication(
        &mut self,
        plan: &effigy_kernel::workspace_session::ActivationPlan,
        ticket: &effigy_kernel::run::GuestPublicationTicket,
        receipt: effigy_kernel::workspace_session::WorkspacePublicationReceipt,
    ) -> Result<(), EnvError> {
        let result = plan
            .validate_receipt(&receipt)
            .and_then(|()| Attachments::workspace_guest_published(&mut self.run, ticket, receipt));
        self.kernel(result)
    }

    pub fn unknown_workspace_publication(
        &mut self,
        ticket: &effigy_kernel::run::GuestPublicationTicket,
        reason: UnknownReason,
    ) -> Result<(), EnvError> {
        self.failed = true;
        let result = Attachments::guest_publication_unknown(&mut self.run, ticket, reason);
        self.kernel(result)
    }

    /// Publish the exact frozen successor session and record the guest's
    /// acknowledgement before `commit_activation`. The callback returns the
    /// identity actually published; the host never fills it into a generic
    /// receipt after publication.
    pub fn publish_workspace_guest_with(
        &mut self,
        plan: &effigy_kernel::workspace_session::ActivationPlan,
        publish: impl FnOnce(
            &effigy_kernel::workspace_session::ActivationPlan,
        ) -> Result<
            effigy_kernel::workspace_session::WorkspacePublicationReceipt,
            BackendError,
        >,
    ) -> Result<(), EnvError> {
        if plan.environment_run() != Some(self.run.id()) || plan.fence() != *self.run.fence() {
            return self.kernel(Err(RunError::refused(
                "workspace publication plan names another activation",
            )));
        }
        let intent = Attachments::intend_guest_publication(&mut self.run);
        let ticket = self.kernel(intent)?;
        self.transitioned = true;
        match publish(plan) {
            Ok(receipt) => {
                let recorded = plan.validate_receipt(&receipt).and_then(|()| {
                    Attachments::workspace_guest_published(&mut self.run, &ticket, receipt)
                });
                self.kernel(recorded)
            }
            Err(error) => {
                self.failed = true;
                let recorded = Attachments::guest_publication_unknown(
                    &mut self.run,
                    &ticket,
                    UnknownReason::Inconsistent(error.to_string()),
                );
                self.kernel(recorded)?;
                Err(EnvError::Backend(error))
            }
        }
    }

    /// Publishes `active` and attaches requesting holders.
    pub fn activate(&mut self, holders: &[(RunId, AttachMode)]) -> Result<(), EnvError> {
        self.phase_with_count("activating the environment", (0, holders.len() as u64));
        self.progress
            .event(format!("attaching {} run(s)", holders.len()));
        let result = Attachments::publish_active_for(&mut self.run, holders);
        self.kernel(result)?;
        self.published = true;
        self.phase_with_count(
            "activating the environment",
            (holders.len() as u64, holders.len() as u64),
        );
        self.progress.event("the environment is active");
        Ok(())
    }

    /// Preserve the caller's one absolute budget across external observations.
    pub fn observation_deadline(&self, requested: Instant) -> Instant {
        match self.cx.operation().deadline() {
            Deadline::At(original) => requested.min(original),
            Deadline::Open(_) => requested,
        }
    }

    /// Recheck the apply's journalled inventory under its exact claimed
    /// reservation, before the caller binds a target. Missing evidence refuses.
    pub fn recheck_creation(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        backend: &mut dyn super::backend::EnvironmentBackend,
        deadline: Instant,
    ) -> Result<super::CreationReceipt, EnvError> {
        let deadline = self.observation_deadline(deadline);
        if reservation.environment() != &self.fence().resource
            || reservation.generation() != self.fence().generation
        {
            return Err(EnvError::Invalid(
                "creation receipt targets a stale reservation generation".into(),
            ));
        }
        let checked = permit(&mut self.run, None).map(|_| ());
        self.kernel(checked)?;
        let checked = self.run.verify_workspace_reservation(reservation);
        self.kernel(checked)?;
        let outcome = self.journalled_creation()?.ok_or_else(|| {
            EnvError::Invalid("activation has no journalled apply inventory".into())
        })?;
        let receipt = outcome.receipt().ok_or_else(|| {
            EnvError::Invalid("activation has no complete gate-scoped attribution".into())
        })?;
        if receipt.generation() != reservation.generation().get() || receipt.run() != self.id() {
            return Err(EnvError::Invalid(
                "creation receipt belongs to another activation".into(),
            ));
        }
        receipt
            .reobserve(backend, deadline)
            .map_err(|error| EnvError::Backend(BackendError::Uncertain(error.to_string())))?;
        let checked = permit(&mut self.run, None).map(|_| ());
        self.kernel(checked)?;
        let checked = self.run.verify_workspace_reservation(reservation);
        self.kernel(checked)?;
        // The journal's binding transaction checks the live gate and fence;
        // callers must still bind within this EnvRun, never carry this result
        // into a later owner.
        Ok(receipt.clone())
    }

    /// Bind only the apply inventory for this run's claimed activation. The
    /// caller cannot supply a different run or permit as authority.
    pub fn bind_target(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        candidates: Observed<Vec<effigy_kernel::workspace_session::TargetCandidate>>,
    ) -> Result<effigy_kernel::workspace_session::ContainerIdentity, EnvError> {
        let outcome = self
            .journalled_creation()?
            .ok_or_else(|| EnvError::Invalid("target has no journalled apply inventory".into()))?;
        let receipt = outcome.receipt().ok_or_else(|| {
            EnvError::Invalid("target has no complete gate-scoped apply inventory".into())
        })?;
        let selection =
            match BackendSelection::from_metadata(&effigy_kernel::store::BackendMetadata {
                engine: receipt.engine().into(),
                context: receipt.context().into(),
                source: "gate-scoped apply journal".into(),
            }) {
                Observed::Verified(selection, _) => selection,
                _ => {
                    return Err(EnvError::Invalid(
                        "creation journal has an unavailable backend".into(),
                    ))
                }
            };
        let deadline = match self.cx.operation().deadline() {
            Deadline::At(deadline) => deadline,
            Deadline::Open(_) => Instant::now() + RUN_BUDGET,
        };
        self.bind_target_observing(
            reservation,
            candidates,
            selection.adapter().as_mut(),
            deadline,
        )
    }

    /// The same binding with backend I/O injected for deterministic proofs.
    /// Creation authority still comes only from this run's own journal.
    pub fn bind_target_observing(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        candidates: Observed<Vec<effigy_kernel::workspace_session::TargetCandidate>>,
        backend: &mut dyn super::backend::EnvironmentBackend,
        deadline: Instant,
    ) -> Result<effigy_kernel::workspace_session::ContainerIdentity, EnvError> {
        let outcome = self
            .journalled_creation()?
            .ok_or_else(|| EnvError::Invalid("target has no journalled apply inventory".into()))?;
        let receipt = outcome.receipt().ok_or_else(|| {
            EnvError::Invalid("target has no complete gate-scoped apply inventory".into())
        })?;
        let candidates = match candidates {
            Observed::Verified(mut candidates, evidence) if candidates.len() == 1 => {
                let candidate = &mut candidates[0];
                if candidate.project != receipt.project()
                    || Some(candidate.identity.container_id.as_str()) != receipt.workspace_id()
                {
                    return Err(EnvError::Invalid(
                        "target is not the workspace in the complete apply inventory".into(),
                    ));
                }
                let created_here = receipt
                    .created_by(&candidate.project, &candidate.identity.container_id)
                    .is_some_and(|container| container.service() == receipt.primary_service());
                candidate.created_by = created_here.then_some(self.id());
                if matches!(
                    reservation.inputs().map(|inputs| &inputs.target),
                    Some(effigy_kernel::workspace_session::TargetEvidence::Absent { .. })
                ) && !created_here
                {
                    return Err(EnvError::Invalid(
                        "fresh target is not in this activation's created workspace ID set".into(),
                    ));
                }
                Observed::Verified(candidates, evidence)
            }
            _ => {
                return Err(EnvError::Invalid(
                    "target inventory is unavailable or ambiguous".into(),
                ))
            }
        };
        self.recheck_creation(reservation, backend, deadline)?;
        let result = reservation.bind_target(&mut self.run, candidates);
        self.kernel(result)
    }

    /// Freeze the backend's image evidence using this run's held authority.
    pub fn bind_platform(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        observed: Observed<effigy_kernel::workspace_session::BoundPlatform>,
    ) -> Result<effigy_kernel::workspace_session::BoundPlatform, EnvError> {
        let result = reservation.bind_platform(&mut self.run, observed);
        self.kernel(result)
    }

    /// Issue the required stage from journal facts, before the caller writes.
    pub fn intend_guest_binary(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<effigy_kernel::run::GuestBinaryTicket, EnvError> {
        let stage = self.kernel(reservation.guest_binary_stage(&self.run))?;
        let result = Attachments::intend_guest_binary(&mut self.run, &stage);
        self.kernel(result)
    }

    pub fn guest_binary_staged(
        &mut self,
        ticket: &effigy_kernel::run::GuestBinaryTicket,
        receipt: Observed<effigy_kernel::workspace_session::GuestBinaryReceipt>,
    ) -> Result<(), EnvError> {
        let result = Attachments::guest_binary_staged(&mut self.run, ticket, receipt);
        self.kernel(result)
    }

    pub fn guest_binary_unknown(
        &mut self,
        ticket: &effigy_kernel::run::GuestBinaryTicket,
        reason: effigy_kernel::observe::UnknownReason,
    ) -> Result<(), EnvError> {
        let result = Attachments::guest_binary_unknown(&mut self.run, ticket, reason);
        self.kernel(result)
    }

    /// Commits a staged workspace activation under this run's held `Gate(Env)`:
    /// the guest publication this run recorded and the validated guest selection
    /// are validated against the staged plan, and then `active(E)`, the
    /// environment's session mapping, the session's `Borrow` attachment and the
    /// durable `Pending` selection obligation commit in one transaction (§8.7,
    /// §8.11). The live holders of a predecessor generation are left attached: a
    /// successor never stops a predecessor or rebinds its listener. The caller
    /// retains the owning session Run from Task admission through this commit and
    /// the launch that published the selection's locator; the borrowed private
    /// slot closes the session-death/publication race.
    pub fn commit_activation(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        session_run: &effigy_kernel::run::Run<'_>,
        selection: &effigy_kernel::workspace_session::SelectionReceipt,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
    ) -> Result<effigy_kernel::workspace_session::SessionRef, EnvError> {
        self.commit_workspace_selection(reservation, session_run, selection, launch)
            .map(|committed| committed.session())
    }

    /// Return the durable selection obligation so the caller can perform CAS
    /// and record readback before reporting successful activation.
    pub fn commit_workspace_selection(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
        session_run: &effigy_kernel::run::Run<'_>,
        selection: &effigy_kernel::workspace_session::SelectionReceipt,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
    ) -> Result<effigy_kernel::workspace_session::CommittedActivation, EnvError> {
        let result = reservation.commit_selection(
            &self.cx,
            self.root,
            &self.run,
            session_run,
            selection,
            launch,
        );
        let committed = self.kernel(result)?;
        self.published = true;
        Ok(committed)
    }

    pub fn workspace_selection_switched(
        &mut self,
        obligation: &effigy_kernel::workspace_session::SelectionObligation,
        launch: &effigy_kernel::workspace_session::SessionLaunch,
        reader: &mut dyn effigy_kernel::workspace_session::SelectorReader,
    ) -> Result<(), EnvError> {
        let result = Attachments::selection_switched(&mut self.run, obligation, launch, reader);
        self.kernel(result)
    }

    /// Ends the run. A run that changed nothing republishes the state it
    /// found, so an inspection never makes an active environment inactive.
    /// For a live mapped workspace this retains the predecessor's applied
    /// generation, including after HeldByRuns; only its successor commit replaces it.
    pub fn finish(self, ok: bool) -> Result<CleanupOutcome, EnvError> {
        self.finish_recoverable(ok).map_err(|(error, _run)| error)
    }

    /// Retain the exact Env run if its settlement fails.
    pub fn finish_recoverable(self, ok: bool) -> Result<CleanupOutcome, (EnvError, Box<Run<'r>>)> {
        self.finish_with_policy(ok, false)
    }

    /// A workspace activation must retain the run even when cleanup is
    /// incomplete; ordinary environment settlement keeps its existing policy.
    pub(super) fn finish_workspace_recoverable(
        self,
        ok: bool,
    ) -> Result<CleanupOutcome, (EnvError, Box<Run<'r>>)> {
        self.finish_with_policy(ok, true)
    }

    pub(super) fn owns_root(&self, root: &StateRoot) -> bool {
        std::ptr::eq(self.root, root)
    }

    fn finish_with_policy(
        mut self,
        ok: bool,
        workspace: bool,
    ) -> Result<CleanupOutcome, (EnvError, Box<Run<'r>>)> {
        if ok
            && !self.published
            && !self.failed
            && !self.transitioned
            && self.began.state == EnvState::Active
        {
            let result = Attachments::publish_active(&mut self.run);
            if let Err(error) = self.kernel(result) {
                return Err((error, Box::new(self.run)));
            }
        }
        let workload = if ok && !self.failed {
            WorkloadOutcome::Succeeded
        } else {
            WorkloadOutcome::Failed { code: 1 }
        };
        // A poisoned run cannot record its outcome; settle still closes it.
        let _ = self.run.finish(workload);
        let env = self.env.name.clone();
        // Settlement begins here, so its budget starts here: an activation that
        // outlived the budget handed to the run's opening still gets the whole
        // of it (effigy#447). It is bounded, and it dispatches no backend
        // effect, so it neither extends the owner gate nor acts after a stop.
        self.phase(format!("settling run {}", self.run.id()), None);
        let cleanup = match self.cleanup() {
            Ok(cleanup) => cleanup,
            Err(error) => {
                self.step
                    .fail(&format!("environment `{env}` could not settle: {error}"));
                return Err((error, Box::new(self.run)));
            }
        };
        let settled = if workspace {
            self.run
                .settle_workspace_recoverable(&cleanup, &mut EnvSettlement)
        } else {
            self.run.settle_recoverable(&cleanup, &mut EnvSettlement)
        };
        let outcome = match settled {
            Ok(outcome) => {
                self.step.succeed(&format!("environment `{env}` settled"));
                Ok(outcome)
            }
            Err((error, run)) => {
                self.step.fail(&format!(
                    "settling environment `{env}` did not complete: {error}"
                ));
                Err((EnvError::from_run(&env, error), run))
            }
        }?;
        Ok(outcome.cleanup())
    }

    /// The phase this transition is in now.
    pub(super) fn phase(&self, phase: impl Into<String>, count: Option<(u64, u64)>) {
        self.progress.update(phase, count, None);
    }

    /// Publish the last thing this transition observed, for a phase a sibling
    /// module of this one drives (the reset's own object drop).
    pub(super) fn publish_event(&self, event: impl Into<String>) {
        self.progress.event(event);
    }

    /// The same, with a known count or ratio.
    fn phase_with_count(&self, phase: impl Into<String>, count: (u64, u64)) {
        self.progress.update(phase, Some(count), None);
    }

    /// A fresh bounded cleanup context for this run's own settlement work.
    fn cleanup(&self) -> Result<CleanupContext, EnvError> {
        cleanup(&self.cx, self.cleanup_budget)
    }

    pub(super) fn kernel<T>(&mut self, result: Result<T, RunError>) -> Result<T, EnvError> {
        result.map_err(|error| {
            self.failed = true;
            EnvError::from_run(&self.env.name, error)
        })
    }
}

fn permit<'a, 'r>(
    run: &'a mut Run<'r>,
    exempt: Option<RunId>,
) -> Result<TransitionPermit<'a, 'r>, RunError> {
    match exempt {
        Some(exempt) => Attachments::permit_exempt(run, exempt),
        None => Attachments::permit(run),
    }
}

/// An engine crossing for the §12.1 harness; always `Ok` in product builds.
fn fault(point: &'static str) -> std::io::Result<()> {
    #[cfg(feature = "fault-injection")]
    {
        effigy_kernel::fault_injection::point(point)
    }
    #[cfg(not(feature = "fault-injection"))]
    {
        let _ = point;
        Ok(())
    }
}

pub(crate) fn new_run_id() -> Result<RunId, EnvError> {
    let mut entropy = [0; 10];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut entropy))
        .map_err(|error| EnvError::Invalid(format!("run id entropy: {error}")))?;
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    RunId::new(milliseconds, entropy).map_err(|error| EnvError::Invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::record::Retention;
    use super::super::tests::{bring_up, closure, env, holder, home, root, Fake};
    use super::*;

    /// The shipped adapter must be able to commit a staged activation: it
    /// threads the validated selection (and the launch that published its
    /// locator) into the kernel's one commit, in the held-run order.
    #[test]
    fn a_staged_activation_commits_through_the_environment_run_adapter() {
        use super::super::tests::checkout;
        use effigy_kernel::ctx::CleanupContext;
        use effigy_kernel::ctx::{EffectClass, Placement};
        use effigy_kernel::id::{ExecutionTarget, Route, Surface, TaskId};
        use effigy_kernel::observe::{Evidence, Observed};
        use effigy_kernel::run::{AttachmentId, BeginOpts, Reconciled, SettlementAdapter};
        use effigy_kernel::workspace_session::{
            ActivationDecision, ArtifactEvidence, ChannelListener, GuestLocatorTarget, GuestObject,
            GuestSelection, HostChannelListener, PublishInputs, SelectionObjects, SelectionReceipt,
            SelectorChange, ServingKey, SessionRef, TargetEvidence,
        };
        use std::collections::BTreeMap;
        use std::time::{Duration, Instant};

        #[derive(Debug)]
        struct NoEffects;
        impl SettlementAdapter for NoEffects {
            fn observe(
                &mut self,
                effect: &effigy_kernel::run::Effect,
                _cx: &CleanupContext,
            ) -> Observed<effigy_kernel::run::Fence> {
                Observed::Verified(effect.spec.target.clone(), evidence())
            }
            fn release(
                &mut self,
                _effect: &effigy_kernel::run::Effect,
                _cx: &CleanupContext,
            ) -> Observed<()> {
                Observed::Verified((), evidence())
            }
            fn reconcile(
                &mut self,
                _effect: &effigy_kernel::run::Effect,
                _cx: &CleanupContext,
            ) -> Observed<Reconciled> {
                Observed::Verified(Reconciled::Released, evidence())
            }
        }
        fn evidence() -> Evidence {
            Evidence::new("container adapter".into(), None).expect("evidence")
        }

        let home = home();
        let state = root(home.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("environment context");
        let stack = env("adapter");
        let task = TaskId::new(checkout(), "", Surface::Published, "workspace", None)
            .expect("workspace task")
            .resource();
        let binary = effigy_kernel::id::Digest::of(b"adapter artifact");
        let inputs = PublishInputs {
            platform: None,
            guest: Some(effigy_kernel::workspace_session::GuestContextInputs {
                version: env!("CARGO_PKG_VERSION").into(),
                protocol: 1,
                host_user: "1000".into(),
                checkout_id: checkout().text(),
                environment_resource: stack.resource.name().to_string(),
                profile: "fixture".into(),
                runtime: None,
                vm_type: None,
                routes: vec![],
                paths: effigy_kernel::workspace_session::GuestPaths {
                    host_checkout: "/host".into(),
                    guest_checkout: "/workspace".into(),
                    workdir: "/workspace".into(),
                    user: "1000".into(),
                    home: "/home/worker".into(),
                },
                sources: vec![],
            }),
            artifact: ArtifactEvidence::Selected {
                digest: binary.clone(),
                object: format!("/run/effigy/objects/bin/{binary}"),
            },
            selection: effigy_kernel::id::Digest::of(b"adapter selection"),
            inputs_digest: effigy_kernel::id::Digest::of(b"adapter inputs"),
            target: TargetEvidence::Absent {
                project: super::super::tests::PROJECT.to_owned(),
                backend: "colima".to_owned(),
            },
        };
        let reservation =
            match effigy_kernel::workspace_session::WorkspaceActivationReservation::decide(
                &ctx.cx,
                &state,
                &stack.resource,
                &inputs,
                Wait::FailFast,
            )
            .expect("activation decision")
            {
                ActivationDecision::Activate(reservation) => *reservation,
                ActivationDecision::Attach(_) => {
                    panic!("a fresh environment reserves a generation")
                }
            };
        let session_run_id = effigy_kernel::id::RunId::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0),
            [3; 10],
        )
        .expect("session id");
        reservation
            .stage_session_binding(&ctx.cx, &state, &task, session_run_id)
            .expect("frozen successor identity");
        let invocation =
            effigy_kernel::id::ResolvedInvocation::new(effigy_kernel::id::InvocationSpec {
                task: TaskId::new(checkout(), "", Surface::Published, "workspace", None)
                    .expect("task"),
                route: Route::Task,
                argv: vec![],
                cwd: "".into(),
                target: ExecutionTarget::Host,
                env: BTreeMap::new(),
                owner: task.clone(),
                gates: vec![],
                attachments: vec![],
                class: EffectClass::Mutate,
                placement: Placement::Host,
                provenance: BTreeMap::new(),
            })
            .expect("invocation");
        let cleanup = ctx
            .cx
            .operation()
            .cleanup(Duration::from_secs(10))
            .expect("cleanup budget");
        let (session_run, session) = SessionRef::admit_reserved(
            &ctx.cx,
            &state,
            &invocation,
            BeginOpts {
                id: session_run_id,
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: effigy_kernel::outcome::Invocation::validated(
                    vec!["recover".into()],
                    |args| args == ["recover"],
                )
                .expect("recovery"),
                custody: None,
            },
            &cleanup,
            &mut NoEffects,
            &reservation,
            effigy_kernel::supervise::Placement::Detached,
        )
        .expect("reserved session");
        let mut session_run =
            effigy_kernel::workspace_session::SessionSupervisor::adopt(session_run, "supervisor-1")
                .expect("detached custody");
        let mut run =
            EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
                .expect("claimed admission");
        let target = effigy_kernel::workspace_session::ContainerIdentity {
            container_id: "a1".repeat(32),
            vm_boot_id: "boot-0001".into(),
            backend: "colima".into(),
        };
        use super::super::tests::creation as creation_fixture;
        let render = creation_fixture::render(&["workspace"]);
        let mut row = creation_fixture::row("workspace", 'a');
        row.id = target.container_id.clone();
        let mut backend = creation_fixture::Fake::new(vec![], vec![row.clone()]);
        run.create_with(
            &mut backend,
            &super::super::tests::selection(),
            creation_fixture::request(&render),
            || Ok(()),
        )
        .expect("journalled apply inventory");
        let mut recheck = creation_fixture::Fake::new(vec![row.clone()], vec![row]);
        let creating_run = run.id();
        run.bind_target_observing(
            &reservation,
            Observed::Verified(
                vec![effigy_kernel::workspace_session::TargetCandidate {
                    identity: target.clone(),
                    project: super::super::tests::PROJECT.into(),
                    created_by: Some(creating_run),
                }],
                evidence(),
            ),
            &mut recheck,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("bound target");
        let platform = effigy_kernel::workspace_session::ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
            variant: "v8".into(),
            loader: effigy_kernel::workspace_session::LoaderFamily::Musl,
        };
        // The document names the machine type and loader family of the platform
        // this test binds; a fixed spelling could only pass on one host.
        let machine = match platform.architecture.as_str() {
            "amd64" => "x86_64",
            "arm64" => "aarch64",
            other => panic!("fixture platform `{other}`"),
        };
        let libc = match platform.loader {
            effigy_kernel::workspace_session::LoaderFamily::Gnu => "gnu",
            effigy_kernel::workspace_session::LoaderFamily::Musl => "musl",
        };
        let binding = effigy_kernel::workspace_session::BoundPlatform {
            target: target.clone(),
            platform: platform.clone(),
            artifact_platform: platform.clone(),
            artifact_version: env!("CARGO_PKG_VERSION").into(),
            artifact: effigy_kernel::workspace_session::GuestObject {
                path: format!("/run/effigy/objects/bin/{binary}"),
                digest: binary.clone(),
            },
            execution_available: true,
        };
        run.bind_platform(
            &reservation,
            Observed::Verified(binding.clone(), evidence()),
        )
        .expect("bound platform");
        let binary_ticket = run
            .intend_guest_binary(&reservation)
            .expect("binary intent");
        run.guest_binary_staged(
            &binary_ticket,
            Observed::Verified(
                effigy_kernel::workspace_session::GuestBinaryReceipt {
                    target,
                    platform,
                    object: binding.artifact,
                },
                evidence(),
            ),
        )
        .expect("binary readback");
        let plan = {
            let observe = ctx.cx.operation().observe().expect("observe");
            let reader = match effigy_kernel::store::Store::read(&observe, &state) {
                Observed::Verified(reader, _) => reader,
                _ => panic!("the kernel store is not readable here"),
            };
            reservation
                .plan(&reader)
                .verified()
                .cloned()
                .flatten()
                .expect("staged plan")
        };
        let publication = effigy_kernel::workspace_session::WorkspacePublicationReceipt {
            publication: effigy_kernel::run::GuestPublicationReceipt {
                environment: plan.fence(),
                container_id: "a1".repeat(32),
                vm_boot_id: "boot-0001".to_owned(),
                artifact_digest: format!("sha256:{}", binary.hex()),
                context_generation: 1,
            },
            activation_run: run.id(),
            session: plan.binding().clone(),
        };
        run.publish_workspace_guest_with(&plan, |_| Ok(publication.clone()))
            .expect("guest publication");
        // The adapter's own commit path: the selection and the launch that
        // published its locator are threaded through, not bypassed.
        let listener =
            HostChannelListener::bind("127.0.0.1:0".parse().expect("address")).expect("bind");
        let key = ServingKey::new(vec![9u8; 32], effigy_kernel::id::Digest::of(b"adapter pin"))
            .expect("key");
        let mut launch = effigy_kernel::workspace_session::SessionLaunch::launch(
            &mut session_run,
            &plan.session(),
            "supervisor-1",
            key,
            Box::new(listener),
        )
        .expect("launch");
        // Colima can expose a guest peer distinct from the numeric host bind.
        // This fake transport answers the owning launch's exact challenge.
        let guest_target = GuestLocatorTarget {
            container_id: publication.publication.container_id.clone(),
            vm_boot_id: publication.publication.vm_boot_id.clone(),
        };
        let guest_peer = "192.0.2.2".parse().expect("resolved guest peer");
        launch
            .qualify_guest(
                &session_run,
                guest_target.clone(),
                guest_peer,
                |challenge| {
                    challenge.verified(
                        &guest_target,
                        std::net::SocketAddr::new(guest_peer, challenge.host_bind().port()),
                        &challenge.endpoint(),
                        challenge.pin(),
                    )
                },
            )
            .expect("guest qualified");
        let _ = std::time::Instant::now() as Instant;
        let lease = {
            let bound = launch.publish(&session_run).expect("published");
            bound.lease().clone()
        };
        assert_eq!(lease.socket(), launch.endpoint());
        assert_eq!(
            lease.endpoint(),
            format!(
                "effigy-host:{}",
                launch.endpoint().expect("bound listener").port()
            )
        );
        let document = serde_json::to_vec(&serde_json::json!({
            "schema": "effigy.workspace-context.v2",
            "protocol": 1, "host_user": "1000", "checkout_id": checkout().text(),
            "environment_resource": stack.resource.name().to_string(), "profile": "fixture",
            "runtime": null, "vm_type": null, "routes": [], "platform": machine, "libc": libc,
            "version": inputs.guest.as_ref().expect("guest").version,
            "paths": inputs.guest.as_ref().expect("guest").paths,
            "sources": inputs.guest.as_ref().expect("guest").sources,
            "binary_path": format!("/run/effigy/objects/bin/{binary}"),
            "backend": "colima",
            "context_generation": 1,
            "environment_generation": plan.environment_generation().get(),
            "session": {
                "run_id": plan.session_run().to_string(),
                "generation": plan.session_generation().get(),
            },
            "artifact_digest": format!("sha256:{}", binary.hex()),
            "container_id": publication.publication.container_id,
            "vm_boot_id": publication.publication.vm_boot_id,
            "credential": {
                "generation": 1,
                "path": "/run/effigy/objects/enrol/1",
                "digest": effigy_kernel::id::Digest::of(b"enrol").hex(),
            },
            "locator": { "endpoint": lease.endpoint(), "pin": format!("sha256:{}", lease.pin().hex()) },
            "selection_digest": inputs.selection.hex(),
        }))
        .expect("context document");
        let context = effigy_kernel::id::Digest::of(&document);
        struct EmptyGuest;
        impl effigy_kernel::workspace_session::SelectorReader for EmptyGuest {
            fn target(&self) -> effigy_kernel::workspace_session::ContainerIdentity {
                effigy_kernel::workspace_session::ContainerIdentity {
                    container_id: "a1".repeat(32),
                    vm_boot_id: "boot-0001".into(),
                    backend: "colima".into(),
                }
            }
            fn read_selector(&mut self, _: &str) -> Observed<String> {
                Observed::Absent(effigy_kernel::observe::AbsenceProof::NoRow {
                    table: "selector".into(),
                })
            }
            fn read_object(&mut self, _: &str) -> Observed<effigy_kernel::id::Digest> {
                panic!("prior snapshot reads no objects")
            }
        }
        let selection = SelectionReceipt::new(
            &plan,
            publication,
            GuestSelection {
                backend: "colima".to_owned(),
                objects: SelectionObjects {
                    binary: GuestObject {
                        path: format!("/run/effigy/objects/bin/{binary}"),
                        digest: binary.clone(),
                    },
                    context: GuestObject {
                        path: format!("/run/effigy/objects/context/{context}"),
                        digest: context,
                    },
                    credential: GuestObject {
                        path: "/run/effigy/objects/enrol/1".to_owned(),
                        digest: effigy_kernel::id::Digest::of(b"enrol"),
                    },
                    context_generation: effigy_kernel::id::Generation::new(1),
                    credential_generation: effigy_kernel::id::Generation::new(1),
                },
                switches: [
                    SelectorChange {
                        path: effigy_kernel::workspace_session::BINARY_SELECTOR.to_owned(),
                        expected: None,
                        next: format!("/run/effigy/objects/bin/{binary}"),
                    },
                    SelectorChange {
                        path: effigy_kernel::workspace_session::ENTRY_SELECTOR.to_owned(),
                        expected: None,
                        next: format!("/run/effigy/objects/bin/{binary}"),
                    },
                    SelectorChange {
                        path: effigy_kernel::workspace_session::CONTEXT_SELECTOR.to_owned(),
                        expected: None,
                        next: "1".to_owned(),
                    },
                ],
            },
            &document,
            &session_run,
            &mut launch,
            &mut EmptyGuest,
        )
        .expect("selection receipt");
        let committed = run
            .commit_activation(&reservation, &session_run, &selection, &launch)
            .expect("the adapter commits the staged activation");
        assert_eq!(committed, session);
        let _ = AttachmentId::new([1; 16]);
        let _ = (
            ctx.cx.operation().remaining(),
            Duration::from_secs(1),
            Instant::now(),
        );
        let _ = ChannelListener::phase(
            &HostChannelListener::bind("127.0.0.1:0".parse().expect("address")).expect("bind"),
        );
        run.finish(true).expect("settle the activation");
    }

    #[test]
    fn claimed_admission_keeps_the_reserved_fence_under_the_live_gate_probe() {
        let home = home();
        let state = root(home.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("environment context");
        let stack = env("reserved-probe");
        let reservation =
            effigy_kernel::workspace_session::WorkspaceActivationReservation::reserve(
                &ctx.cx,
                &state,
                &stack.resource,
                Wait::FailFast,
            )
            .expect("reserved environment generation");
        assert!(EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).is_err());
        let run = EnvRun::begin_claimed(&ctx, &stack, Operation::Up, Wait::FailFast, &reservation)
            .expect("claimed admission under the progress probe");
        assert_eq!(run.fence().generation, reservation.generation());
        run.finish(false).expect("settle claimed admission");
        reservation
            .cancel(&ctx.cx, &state, Wait::FailFast)
            .expect("release the settled reservation");
    }

    /// effigy#461: at a fake clock, the frames an operator sees name the held
    /// gate and the holder its body records, each reconcile phase, and the
    /// live holder behind the refusal. Nothing here waits for wall-clock time.
    #[test]
    fn fake_clock_frames_name_the_gate_the_reconcile_phase_and_the_refusal() {
        let home = home();
        let state = root(home.path());
        let ctx = EnvContext::new(&state, CancelToken::default()).expect("environment context");
        let stack = env("stack");
        let fake = Fake::new();
        let task = holder(&ctx, "dev", 1);
        bring_up(
            &ctx,
            &stack,
            &fake,
            &closure("one"),
            Retention::Held,
            &[(
                task.id(),
                AttachMode::Exclusive {
                    stop_on_release: true,
                },
            )],
        )
        .expect("the first bring-up attaches the holder");
        let held = task.id().to_string();
        let mut run =
            EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).expect("gate admitted");

        // A held gate: the probe reads the body the kernel wrote for this run.
        let waiting = ProgressSource::waiting_for("environment `stack` to be free");
        publish_gate_state(
            &waiting,
            &stack.name,
            &observe_gate(&state, &stack.resource),
        );
        let frame = waiting.frame_at(Duration::from_secs(4));
        for needle in [
            "waiting for Gate(Env) on `stack`".to_owned(),
            format!("run {}", run.id()),
            format!("pid {}", std::process::id()),
            "still holds it".to_owned(),
            "4s elapsed".to_owned(),
        ] {
            assert!(frame.contains(&needle), "{needle}: {frame}");
        }

        // The reconcile advances: the render is published, then the record.
        run.publish_desired(&closure("two")).expect("publish");
        let frame = run.progress.frame_at(Duration::from_secs(9));
        assert!(
            frame.starts_with("publishing the desired render"),
            "{frame}"
        );
        assert!(frame.contains("last: published render "), "{frame}");
        // The event and the awaited phase are both named.
        assert!(frame.contains("; waiting for "), "{frame}");
        assert!(frame.ends_with("; 9s elapsed"), "{frame}");

        let mut record = run
            .engine_record()
            .cloned()
            .unwrap_or_else(|| EngineRecord::new(Retention::Held, "acme-stack"));
        record.retention = Retention::Persistent;
        run.write_record(&record).expect("record");
        let frame = run.progress.frame_at(Duration::from_secs(11));
        assert!(
            frame.starts_with("writing the environment record"),
            "{frame}"
        );
        assert!(frame.contains("retention: persistent"), "{frame}");
        assert!(frame.contains("project `acme-stack`"), "{frame}");

        // The refusal's own evidence: the attach check names the live holder,
        // and the permit refuses without reaching the backend.
        let holders = run.holders();
        assert!(
            matches!(&holders, Observed::Verified(rows, _) if rows.len() == 1),
            "{holders:?}"
        );
        let mut backend = super::super::tests::Fake::new();
        let refused = run.create_with(
            &mut backend,
            &super::super::tests::selection(),
            super::super::tests::create_request(),
            || panic!("a refused transition never reaches the backend"),
        );
        assert!(
            matches!(refused, Err(EnvError::HeldBy { .. })),
            "{refused:?}"
        );
        let frame = run.progress.frame_at(Duration::from_secs(12));
        assert!(
            frame.starts_with("checking the runs attached to environment `stack`"),
            "{frame}"
        );
        assert!(
            frame.contains(&format!("1 live run(s) hold it: {held}")),
            "{frame}"
        );
        assert!(frame.ends_with("; 12s elapsed"), "{frame}");

        run.finish(false).expect("the run settles");
    }
}
