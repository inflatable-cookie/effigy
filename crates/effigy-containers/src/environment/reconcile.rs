//! The reconciler run: one kernel `Run` owned by the environment, holding
//! `Gate(Env)` for its whole operation (contract 053 §3).
//!
//! Up, down, handover, reset, reap and import each begin one of these runs,
//! so they serialise on the one gate. Every backend mutation goes through a
//! `TransitionPermit`: it is journalled before the adapter runs (K3) and
//! refused while any holder other than a settling exempt run remains (R4).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OperationContext,
    Placement,
};
use effigy_kernel::gate::{GateError, HolderDiag, Wait};
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
    AttachMode, Attachments, Effect, EffectSpec, Fence, Holder, Reconciled, Run, RunError,
    Settlement, SettlementAdapter, TransitionPermit,
};
use serde_json::{json, Value};

use super::backend::{BackendError, BackendSelection};
use super::record::{EngineRecord, EnvRecord, EnvState};

/// Journal kind of a backend apply. The stack outlives the run by design;
/// the record's promotion, not settle, says whether it was applied.
pub const APPLY_EFFECT: &str = "environment:apply";
/// Journal kind of a backend stop. Reported, never replayed (D3).
pub const STOP_EFFECT: &str = "environment:stop";

/// How long a transition waits for `Gate(Env)` held by another one.
pub const GATE_WAIT: Duration = Duration::from_secs(600);
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
    let owner = match holder {
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
    };
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
}

impl<'r> EnvRun<'r> {
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

    fn admit(
        ctx: &EnvContext<'r>,
        env: &EnvRef,
        op: Operation,
        wait: Wait,
        claim: Option<effigy_kernel::workspace_session::ActivationClaim>,
    ) -> Result<Self, EnvError> {
        if ctx.cx.operation().placement() != Placement::Host {
            return Err(EnvError::HostAuthorityRequired);
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
        // The operator watches the gate wait: a transition can queue behind
        // another one for up to ten minutes, and silence reads as a hang.
        let waiting = effigy_ui::steps::step(&format!(
            "waiting for environment `{}` to be free",
            env.name
        ));
        // The original deadline, kept for the refusal's own report.
        let started = Instant::now();
        // Admission settles an interrupted predecessor under a window this
        // call makes once the gate is held, so the wait above (up to ten
        // minutes) cannot spend it (effigy#447).
        let admitted = match claim {
            Some(claim) => Run::begin_claimed(
                &ctx.cx,
                ctx.root,
                &invocation,
                effigy_kernel::run::BeginOpts {
                    id: new_run_id()?,
                    wait,
                    acknowledge: vec![],
                    next: op.next(&env.name)?,
                    custody: None,
                },
                || {
                    ctx.cleanup()
                        .map_err(|error| RunError::broken(error.to_string()))
                },
                &mut EnvSettlement,
                claim,
            ),
            None => Run::begin_with_fresh_cleanup(
                &ctx.cx,
                ctx.root,
                &invocation,
                effigy_kernel::run::BeginOpts {
                    id: new_run_id()?,
                    wait,
                    acknowledge: vec![],
                    next: op.next(&env.name)?,
                    custody: None,
                },
                || {
                    ctx.cleanup()
                        .map_err(|error| RunError::broken(error.to_string()))
                },
                &mut EnvSettlement,
            ),
        }
        .map_err(|error| admission_error(env, wait, started, error));
        let run = match admitted {
            Ok(run) => {
                waiting.succeed(&format!("environment `{}` is free", env.name));
                run
            }
            Err(error) => {
                waiting.fail(&format!("environment `{}` was not entered", env.name));
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
                if let Ok(cleanup) = ctx.cleanup() {
                    let _ = run.settle(&cleanup, &mut EnvSettlement);
                }
                return Err(EnvError::Invalid(format!(
                    "environment record unreadable after admission: {reason}"
                )));
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
    pub fn publish_desired(&mut self, closure: &Closure) -> Result<String, EnvError> {
        let result = Objects::publish_desired(&mut self.run, closure);
        self.kernel(result)
    }

    /// Replaces the engine's fields on the record.
    pub fn write_record(&mut self, record: &EngineRecord) -> Result<(), EnvError> {
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
        match EnvRecord::read(&cx, self.root, &self.env.resource) {
            Observed::Verified(record, _) => record.holders,
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
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
        for holder in holders {
            if !matches!(
                holder.liveness,
                Observed::Verified(effigy_kernel::run::Liveness::Interrupted, _)
            ) {
                continue;
            }
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

    /// The one backend apply: journals the intent under a permit, then runs
    /// `effect` (K3, R4).
    pub fn apply_with(
        &mut self,
        selection: &BackendSelection,
        project: &str,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let payload = json!({
            "engine": selection.engine.as_str(),
            "context": selection.context,
            "project": project,
        });
        self.transition(APPLY_EFFECT, payload, Settlement::Retain, None, effect)
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

    fn transition(
        &mut self,
        kind: &str,
        payload: Value,
        settlement: Settlement,
        exempt: Option<RunId>,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let env = self.env.name.clone();
        self.settle_interrupted_holders()?;
        let mut permit = permit(&mut self.run, exempt).map_err(|error| {
            self.failed = true;
            EnvError::from_run(&env, error)
        })?;
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

    /// Publishes `active` and attaches requesting holders.
    pub fn activate(&mut self, holders: &[(RunId, AttachMode)]) -> Result<(), EnvError> {
        let result = Attachments::publish_active_for(&mut self.run, holders);
        self.kernel(result)?;
        self.published = true;
        Ok(())
    }

    /// Commits a staged workspace activation under this run's held `Gate(Env)`:
    /// the guest publication this run recorded is validated against the staged
    /// plan, and then `active(E)`, the environment's session mapping and the
    /// session's `Borrow` attachment commit in one transaction (§8.7, §8.11).
    /// The live holders of a predecessor generation are left attached: a
    /// successor never stops a predecessor or rebinds its listener.
    pub fn commit_activation(
        &mut self,
        reservation: &effigy_kernel::workspace_session::WorkspaceActivationReservation,
    ) -> Result<effigy_kernel::workspace_session::SessionRef, EnvError> {
        let result = reservation.commit(&self.cx, self.root, &self.run);
        let committed = self.kernel(result)?;
        self.published = true;
        Ok(committed.session())
    }

    /// Ends the run. A run that changed nothing republishes the state it
    /// found, so an inspection never makes an active environment inactive.
    pub fn finish(mut self, ok: bool) -> Result<CleanupOutcome, EnvError> {
        if ok
            && !self.published
            && !self.failed
            && !self.transitioned
            && self.began.state == EnvState::Active
        {
            let result = Attachments::publish_active(&mut self.run);
            self.kernel(result)?;
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
        let cleanup = self.cleanup()?;
        let settling = effigy_ui::steps::step(&format!("settling environment `{env}`"));
        let outcome = match self.run.settle(&cleanup, &mut EnvSettlement) {
            Ok(outcome) => {
                settling.succeed(&format!("environment `{env}` settled"));
                Ok(outcome)
            }
            Err(error) => {
                settling.fail(&format!(
                    "settling environment `{env}` did not complete: {error}"
                ));
                Err(EnvError::from_run(&env, error))
            }
        }?;
        Ok(outcome.cleanup())
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
