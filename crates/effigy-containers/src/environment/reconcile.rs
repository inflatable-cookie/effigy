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
use effigy_kernel::gate::{GateError, Wait};
use effigy_kernel::id::{
    CheckoutId, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId, Route, RunId,
    Surface, TaskId,
};
use effigy_kernel::objects::{Closure, Objects};
use effigy_kernel::observe::{AbsenceProof, Observed, UnknownReason};
use effigy_kernel::outcome::{CleanupOutcome, Invocation, RequiredObligation, WorkloadOutcome};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    AttachMode, Attachments, Effect, EffectSpec, Fence, Holder, Reconciled, Run, RunError,
    Settlement, SettlementAdapter, TransitionPermit,
};
use serde_json::{json, Value};

use super::backend::{BackendError, BackendSelection, EnvironmentBackend, RenderRef};
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
const CLEANUP_BUDGET: Duration = Duration::from_secs(60);

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
    pub cleanup: CleanupContext,
}

impl<'r> EnvContext<'r> {
    pub fn new(root: &'r StateRoot, cancel: CancelToken) -> Result<Self, EnvError> {
        let operation = operation(EffectClass::Mutate, cancel, RUN_BUDGET);
        let cx = operation
            .effects()
            .map_err(|_| EnvError::Invalid("Mutate capability refused".into()))?;
        let cleanup = operation
            .cleanup(CLEANUP_BUDGET)
            .map_err(|_| EnvError::Invalid("cleanup budget overflow".into()))?;
        Ok(Self { root, cx, cleanup })
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

pub(super) fn operation(
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

#[derive(Debug)]
pub enum EnvError {
    /// Another transition holds `Gate(Env)`.
    Busy(String),
    /// Live, interrupted or unknown holders protect the environment (R4).
    HeldBy {
        env: String,
        holders: Vec<Holder>,
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
            Self::Busy(env) => write!(
                f,
                "environment `{env}` is busy: another up, down, handover, reset or reap holds it; retry once it finishes"
            ),
            Self::HeldBy { env, holders } => {
                let runs = holders
                    .iter()
                    .map(|holder| {
                        let liveness = match &holder.liveness {
                            Observed::Verified(effigy_kernel::run::Liveness::Live, _) => "live",
                            Observed::Verified(effigy_kernel::run::Liveness::Interrupted, _) => {
                                "interrupted"
                            }
                            _ => "unknown",
                        };
                        format!("run {} ({liveness})", holder.run)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "refused to stop, restart or recreate environment `{env}` while runs hold it: {runs}. End those runs through their own stop first (`effigy tasks status --all` lists them)"
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
    fn from_run(env: &str, error: RunError) -> Self {
        match error {
            RunError::Gate(GateError::Busy { .. } | GateError::TimedOut { .. }) => {
                Self::Busy(env.to_owned())
            }
            RunError::Gate(GateError::HostAuthorityRequired) | RunError::HostAuthorityRequired => {
                Self::HostAuthorityRequired
            }
            RunError::HeldBy(holders) => Self::HeldBy {
                env: env.to_owned(),
                holders,
            },
            RunError::PredecessorUnsettled { run, effects } => Self::Unsettled(format!(
                "run {run} left {} unresolved effect(s)",
                effects.len()
            )),
            other => Self::Kernel(other),
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
    cleanup: CleanupContext,
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
        let run = Run::begin(
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
            &ctx.cleanup,
            &mut EnvSettlement,
        )
        .map_err(|error| EnvError::from_run(&env.name, error))?;
        // The gate is held: only this run writes the row until it settles.
        let began = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
            Observed::Verified(record, _) => record,
            other => {
                let reason = match other {
                    Observed::Unknown(reason) => format!("{reason:?}"),
                    _ => "row missing after admission".to_owned(),
                };
                let _ = run.settle(&ctx.cleanup, &mut EnvSettlement);
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
            cleanup: ctx.cleanup.clone(),
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

    /// Publishes the environment's render as its desired generation.
    pub fn publish_desired(&mut self, closure: &Closure) -> Result<String, EnvError> {
        let result = Objects::publish_desired(&mut self.run, closure);
        self.kernel(result)
    }

    /// Records the exact backend this environment runs on, and why.
    pub fn set_backend(&mut self, selection: &BackendSelection) -> Result<(), EnvError> {
        let result = self.run.set_backend(&selection.metadata());
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
            match effigy_kernel::run::settle_interrupted(
                &self.cx,
                self.root,
                &owner,
                holder.run,
                next,
                &self.cleanup,
                &mut EnvSettlement,
            ) {
                // A successor holds the owner gate and settles it first (K5).
                Ok(_) | Err(RunError::Gate(_)) => {}
                Err(error) => return Err(EnvError::from_run(&self.env.name, error)),
            }
        }
        Ok(())
    }

    /// Converges the backend on the render through the seam.
    pub fn apply(
        &mut self,
        backend: &mut dyn EnvironmentBackend,
        render: &RenderRef<'_>,
        digest: Option<&str>,
        exempt: Option<RunId>,
    ) -> Result<(), EnvError> {
        let selection = backend.selection().clone();
        self.apply_with(&selection, render.project, digest, exempt, || {
            backend.reconcile(render)
        })
    }

    /// The one backend apply: marks the partial generation, journals the
    /// intent under a permit, then runs `effect` (K3, R4).
    pub fn apply_with(
        &mut self,
        selection: &BackendSelection,
        project: &str,
        digest: Option<&str>,
        exempt: Option<RunId>,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let mut record = self
            .engine_record()
            .cloned()
            .unwrap_or_else(|| EngineRecord::new(super::record::Retention::Held, project));
        record.project = project.to_owned();
        record.applying = digest.map(str::to_owned);
        self.write_record(&record)?;
        let payload = json!({
            "engine": selection.engine.as_str(),
            "context": selection.context,
            "project": project,
            "digest": digest,
        });
        self.transition(APPLY_EFFECT, payload, Settlement::Retain, exempt, effect)
    }

    /// A repair of running workloads (restarting a service that stopped
    /// answering): journalled under a permit like any apply, so it refuses
    /// while a run holds the environment (R4). The record is unchanged.
    pub fn repair_with(
        &mut self,
        selection: &BackendSelection,
        project: &str,
        effect: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<(), EnvError> {
        let payload = json!({
            "engine": selection.engine.as_str(),
            "context": selection.context,
            "project": project,
            "repair": true,
        });
        self.transition(APPLY_EFFECT, payload, Settlement::Retain, None, effect)
    }

    /// Stops the render's workloads through the seam, keeping data.
    pub fn stop(
        &mut self,
        backend: &mut dyn EnvironmentBackend,
        render: &RenderRef<'_>,
        exempt: Option<RunId>,
    ) -> Result<(), EnvError> {
        let selection = backend.selection().clone();
        self.stop_with(&selection, render.project, exempt, || backend.stop(render))
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
        let mut record = self
            .engine_record()
            .cloned()
            .unwrap_or_else(|| EngineRecord::new(super::record::Retention::Held, project));
        record.applying = None;
        record.stopped_at = Some(self.run.fence().generation.get());
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
        // The permit borrows the run; callbacks go through the run itself.
        drop(permit);
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

    /// Promotes the applied generation and publishes `active`, attaching
    /// any runs that requested this ensure in the same transaction.
    pub fn activate(&mut self, holders: &[(RunId, AttachMode)]) -> Result<(), EnvError> {
        if let Some(digest) = self
            .engine_record()
            .and_then(|record| record.applying.clone())
        {
            if self.began.desired.as_deref() == Some(digest.as_str()) || self.desired_is(&digest) {
                let result = Objects::promote(&mut self.run, &digest);
                self.kernel(result)?;
            }
            let mut record = self.engine_record().cloned().expect("applying record");
            record.applying = None;
            self.write_record(&record)?;
        }
        let result = Attachments::publish_active_for(&mut self.run, holders);
        self.kernel(result)?;
        self.published = true;
        Ok(())
    }

    /// Publishes `active` and attaches requesting holders while a pending
    /// desired generation waits for the current holders to end.
    pub fn activate_without_promotion(
        &mut self,
        holders: &[(RunId, AttachMode)],
    ) -> Result<(), EnvError> {
        let result = Attachments::publish_active_for(&mut self.run, holders);
        self.kernel(result)?;
        self.published = true;
        Ok(())
    }

    fn desired_is(&self, digest: &str) -> bool {
        let cx = operation(
            EffectClass::Observe,
            CancelToken::default(),
            Duration::from_secs(10),
        )
        .observe()
        .expect("observe context");
        matches!(
            EnvRecord::read(&cx, self.root, &self.env.resource),
            Observed::Verified(record, _) if record.desired.as_deref() == Some(digest)
        )
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
        let outcome = self
            .run
            .settle(&self.cleanup, &mut EnvSettlement)
            .map_err(|error| EnvError::from_run(&self.env.name, error))?;
        Ok(outcome.cleanup())
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

pub(super) fn new_run_id() -> Result<RunId, EnvError> {
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
