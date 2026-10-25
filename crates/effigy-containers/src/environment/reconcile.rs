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

    fn holder_facts(&self) -> HolderFacts {
        HolderFacts::observe(&self.observe(), self.root)
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

/// What a refused transition could observe about the run holding
/// `Gate(Env)`. Diagnostics only (I-G2): the kernel's own gate evidence and
/// one pure record read, never a second probe and never authority.
#[derive(Debug, Clone)]
pub struct BusyDiagnostics {
    pub env: String,
    /// The gate's diagnostic body exactly as the refusal found it. `None`
    /// when the refusal came from inside an admitted run, which retains no
    /// gate observation.
    pub holder: Option<Box<Observed<HolderDiag>>>,
    /// The environment record as a pure read found it (R2). `None` when no
    /// read was made.
    pub record: Option<Observed<BusyRecord>>,
    /// How long the original wait ran and the deadline it was given. Both
    /// are `None` for a fail-fast refusal, which never waited.
    pub waited: Option<Duration>,
    pub wait_limit: Option<Duration>,
}

/// The record facts a refusal reports: no digest, backend, argv or secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusyRecord {
    pub state: EnvState,
    pub generation: u64,
}

impl BusyDiagnostics {
    /// A refusal that retained no observation: same shape, nothing claimed.
    fn unobserved(env: &str) -> Self {
        Self {
            env: env.to_owned(),
            holder: None,
            record: None,
            waited: None,
            wait_limit: None,
        }
    }

    /// The observed owner and state. It names what the gate and record say
    /// and never turns an unverified identity into a live claim.
    pub fn text(&self) -> String {
        let mut text = format!("environment `{}` is busy: {}", self.env, self.owner_text());
        if let Some(record) = &self.record {
            text.push_str(&Self::record_text(record));
        }
        text.push('.');
        if let Some(waited) = self.waited {
            match self.wait_limit {
                Some(limit) => text.push_str(&format!(
                    " The wait ended after {:.1}s of its {:.1}s budget.",
                    waited.as_secs_f64(),
                    limit.as_secs_f64()
                )),
                None => text.push_str(&format!(
                    " The wait ended after {:.1}s.",
                    waited.as_secs_f64()
                )),
            }
        }
        text.push_str(
            " Nothing was changed and no backend was contacted. Inspect the observed state with ",
        );
        text.push_str(&format!("`effigy container {} status`", self.env));
        text.push_str("; retry once that transition finishes.");
        text
    }

    fn owner_text(&self) -> String {
        match &self.holder {
            Some(holder) => match holder.as_ref() {
                Observed::Verified(diag, _) => {
                    let owner = match &diag.run {
                        Some(run) => format!("transition run {run}"),
                        None => "an unrecorded run".to_owned(),
                    };
                    format!("its gate is held by {owner} ({})", observe_text(diag))
                }
                Observed::Unknown(reason) => format!(
                    "its gate names a holder whose diagnostics are unreadable ({})",
                    unknown_label(reason)
                ),
                Observed::Absent(_) => "its gate reports no holder".to_owned(),
            },
            None => "another transition holds its gate, and its holder was not observed".to_owned(),
        }
    }

    fn record_text(record: &Observed<BusyRecord>) -> String {
        match record {
            Observed::Verified(record, _) => format!(
                "; the environment record reads {} at generation {}",
                state_label(record.state),
                record.generation
            ),
            Observed::Unknown(reason) => format!(
                "; the environment record is unreadable ({})",
                unknown_label(reason)
            ),
            Observed::Absent(_) => "; the environment has no record yet".to_owned(),
        }
    }

    /// The stable `effigy.environment-busy.v1` details for `--json`. Additive
    /// holder and transition facts only: no argv, secret or command text.
    pub fn details(&self) -> Value {
        json!({
            "schema": "effigy.environment-busy.v1",
            "schema_version": 1,
            "environment": self.env,
            "refusal": if self.waited.is_some() { "timed-out" } else { "busy" },
            "waited_ms": self.waited.map(|waited| waited.as_millis() as u64),
            "wait_budget_ms": self.wait_limit.map(|limit| limit.as_millis() as u64),
            "holder": self.holder.as_ref().map(|holder| holder_details(holder.as_ref())),
            "environment_state": self.record.as_ref().map(record_details),
        })
    }
}

fn observe_text(diag: &HolderDiag) -> String {
    match &diag.state {
        Observed::Verified(ProcessState::Active, _) => {
            format!("pid {} on this host, reported live", diag.pid)
        }
        Observed::Verified(ProcessState::Stopped, _) => format!(
            "pid {} on this host, stopped (SIGSTOP); it keeps the gate until resumed",
            diag.pid
        ),
        Observed::Verified(ProcessState::Zombie, _) => format!(
            "pid {} on this host, whose process has exited and is not yet reaped",
            diag.pid
        ),
        Observed::Unknown(reason) => format!(
            "pid {} on this host, whose process identity is unverifiable ({})",
            diag.pid,
            unknown_label(reason)
        ),
        Observed::Absent(_) => format!(
            "pid {} on this host, whose process state could not be observed",
            diag.pid
        ),
    }
}

fn holder_details(holder: &Observed<HolderDiag>) -> Value {
    match holder {
        Observed::Verified(diag, _) => {
            let mut details = json!({
                "observed": "verified",
                "run": diag.run,
                "pid": diag.pid,
                "state": holder_state_label(&diag.state),
            });
            if let Observed::Unknown(reason) = &diag.state {
                details["state_reason"] = json!(unknown_label(reason));
            }
            details
        }
        Observed::Absent(_) => json!({"observed": "absent"}),
        Observed::Unknown(reason) => {
            json!({"observed": "unknown", "reason": unknown_label(reason)})
        }
    }
}

fn record_details(record: &Observed<BusyRecord>) -> Value {
    match record {
        Observed::Verified(record, _) => json!({
            "observed": "verified",
            "state": state_label(record.state),
            "generation": record.generation,
        }),
        Observed::Absent(_) => json!({"observed": "absent"}),
        Observed::Unknown(reason) => {
            json!({"observed": "unknown", "reason": unknown_label(reason)})
        }
    }
}

fn state_label(state: EnvState) -> &'static str {
    match state {
        EnvState::Active => "active",
        EnvState::Stopped => "stopped",
        EnvState::Pending => "pending",
    }
}

fn holder_state_label(state: &Observed<ProcessState>) -> &'static str {
    match state {
        Observed::Verified(ProcessState::Active, _) => "running",
        Observed::Verified(ProcessState::Stopped, _) => "stopped",
        Observed::Verified(ProcessState::Zombie, _) => "exited",
        _ => "unknown",
    }
}

/// Closed vocabulary: the category only, never the free-form detail, which
/// can name paths.
fn unknown_label(reason: &UnknownReason) -> &'static str {
    match reason {
        UnknownReason::TimedOut { .. } => "timed-out",
        UnknownReason::Busy { .. } => "busy",
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

#[derive(Debug)]
pub enum EnvError {
    /// Another transition holds `Gate(Env)`. The diagnostics are carried
    /// through from the refusal and never decide anything; boxed so the
    /// refusal stays small on every `Result` it passes through.
    Busy(Box<BusyDiagnostics>),
    /// Live, interrupted or unknown holders protect the environment (R4).
    HeldBy {
        env: String,
        holders: Vec<HeldHolder>,
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
            Self::Busy(diagnostics) => f.write_str(&diagnostics.text()),
            Self::HeldBy { env, holders } => {
                let count = holders.len();
                write!(
                    f,
                    "refused to stop, restart or recreate environment `{env}` while {count} run{} hold{} it:",
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "s" } else { "" },
                )?;
                for holder in holders {
                    write!(f, "\n- {}", holder.text())?;
                }
                if let Some(first) = holders.first() {
                    write!(
                        f,
                        "\nInspect it with `effigy run status {}`, or list every run with `effigy runs`; then retry the command",
                        first.run
                    )?;
                } else {
                    f.write_str(
                        "\nList every run with `effigy runs`; then retry the command",
                    )?;
                }
                Ok(())
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
    /// The stable structured details of a busy refusal, for error envelopes.
    pub fn busy_details(&self) -> Option<String> {
        match self {
            Self::Busy(diagnostics) => Some(diagnostics.details().to_string()),
            _ => None,
        }
    }

    /// A refusal that retained no gate evidence still keeps its own variant,
    /// distinct from `HeldBy` and from kernel journal contention.
    ///
    /// Without a context the holders are named by run and liveness only; the
    /// admission path supplies one, which adds the task, pid, age and whether
    /// the run is attached to a terminal (effigy#313).
    pub(crate) fn from_run(facts: Option<&HolderFacts>, env: &str, error: RunError) -> Self {
        match error {
            RunError::Gate(GateError::Busy { .. } | GateError::TimedOut { .. }) => {
                Self::Busy(Box::new(BusyDiagnostics::unobserved(env)))
            }
            RunError::Gate(GateError::HostAuthorityRequired) | RunError::HostAuthorityRequired => {
                Self::HostAuthorityRequired
            }
            RunError::HeldBy(holders) => Self::HeldBy {
                env: env.to_owned(),
                holders: match facts {
                    Some(facts) => holders
                        .into_iter()
                        .map(|holder| facts.holder(holder))
                        .collect(),
                    None => holders.into_iter().map(HeldHolder::unobserved).collect(),
                },
            },
            RunError::PredecessorUnsettled { run, effects } => Self::Unsettled(format!(
                "run {run} left {} unresolved effect(s)",
                effects.len()
            )),
            other => Self::Kernel(other),
        }
    }
}

/// A refused admission onto `Gate(Env)`: carries the kernel's own gate
/// evidence and one pure read of the record. Nothing is acquired, retried,
/// waited for again or released here.
fn admission_error(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    wait: Wait,
    started: Instant,
    error: RunError,
) -> EnvError {
    let diagnostics =
        |holder: Box<Observed<HolderDiag>>, waited: Option<Duration>| BusyDiagnostics {
            env: env.name.clone(),
            holder: Some(holder),
            record: Some(observed_record(ctx, env)),
            waited,
            wait_limit: waited.and(match wait {
                Wait::FailFast => None,
                Wait::Until(deadline) => Some(deadline.saturating_duration_since(started)),
            }),
        };
    match error {
        RunError::Gate(GateError::Busy { holder, .. }) => {
            EnvError::Busy(Box::new(diagnostics(holder, None)))
        }
        RunError::Gate(GateError::TimedOut { holder, waited, .. }) => {
            EnvError::Busy(Box::new(diagnostics(holder, Some(waited))))
        }
        other => EnvError::from_run(Some(&ctx.holder_facts()), &env.name, other),
    }
}

/// What the refusal may read to name a holder: the same observation context
/// and state root the transition already holds. No second probe is taken.
#[derive(Debug, Default, Clone)]
pub(crate) struct HolderFacts {
    /// Every run holding a foreground (terminal) gate: its task and pid.
    foreground: BTreeMap<String, (Option<String>, u32)>,
    /// When each journaled run started.
    started: BTreeMap<String, i64>,
}

impl HolderFacts {
    /// One pure read of the foreground gates and the run journal. Never
    /// probes a process and never decides anything.
    pub(crate) fn observe(cx: &effigy_kernel::ctx::ObserveContext, root: &StateRoot) -> Self {
        use effigy_kernel::gate::Gate;
        let mut facts = Self::default();
        if let Observed::Verified(rows, _) = Gate::foreground_holders(cx, root) {
            for row in rows {
                let Some(run) = row.holder.run.clone() else {
                    continue;
                };
                let coordinates = &row.coordinates.task;
                let task = if coordinates.catalog().is_empty() {
                    coordinates.task().to_owned()
                } else {
                    format!("{}/{}", coordinates.catalog(), coordinates.task())
                };
                facts.foreground.insert(run, (Some(task), row.holder.pid));
            }
        }
        if let Observed::Verified(reader, _) = effigy_kernel::store::Store::read(cx, root) {
            for (run, summary) in facts
                .foreground
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|run| {
                    let id: RunId = run.parse().ok()?;
                    match effigy_kernel::run::summary(&reader, id) {
                        Observed::Verified(summary, _) => Some((run, summary)),
                        _ => None,
                    }
                })
            {
                facts.started.insert(run, summary.created_at_ms);
            }
        }
        facts
    }

    fn holder(&self, holder: Holder) -> HeldHolder {
        let key = holder.run.to_string();
        let foreground = self.foreground.get(&key);
        HeldHolder {
            run: holder.run,
            liveness: holder.liveness,
            task: foreground.and_then(|(task, _)| task.clone()),
            pid: foreground.map(|(_, pid)| *pid),
            started_at_ms: self.started.get(&key).copied(),
            foreground: foreground.is_some(),
        }
    }
}

/// One run holding the environment, named for a person.
///
/// A refusal names the run, and — when the journal and the foreground gate can
/// be read — its task, pid, age and whether it was started from a terminal,
/// because that is the difference between `Ctrl-C` and `effigy run stop`
/// (effigy#313). Every field is a fact from a pure read; an unreadable one is
/// simply absent, never guessed.
#[derive(Debug, Clone)]
pub struct HeldHolder {
    pub run: RunId,
    pub liveness: Observed<effigy_kernel::run::Liveness>,
    pub task: Option<String>,
    pub pid: Option<u32>,
    pub started_at_ms: Option<i64>,
    /// The run holds a foreground (terminal) gate of its own.
    pub foreground: bool,
}

impl HeldHolder {
    /// The holder as the kernel named it, with nothing observed about it.
    fn unobserved(holder: Holder) -> Self {
        Self {
            run: holder.run,
            liveness: holder.liveness,
            task: None,
            pid: None,
            started_at_ms: None,
            foreground: false,
        }
    }

    /// One line: `run abc (`dev`, pid 99290, started 1h06m ago, attached to a
    /// terminal); stop it with `effigy run stop abc` or Ctrl-C in its terminal`.
    pub fn text(&self) -> String {
        let mut facts = Vec::new();
        if let Some(task) = self.task.as_deref() {
            facts.push(format!("`{task}`"));
        }
        if let Some(pid) = self.pid {
            facts.push(format!("pid {pid}"));
        }
        if let Some(age) = self.started_at_ms.and_then(age_text) {
            facts.push(format!("started {age} ago"));
        }
        facts.push(if self.foreground {
            "attached to a terminal".to_owned()
        } else {
            "not attached to a terminal".to_owned()
        });
        let liveness = match &self.liveness {
            Observed::Verified(effigy_kernel::run::Liveness::Interrupted, _) => {
                " (interrupted)".to_owned()
            }
            _ => String::new(),
        };
        let mut line = format!("run {}{liveness}", self.run);
        if !facts.is_empty() {
            line.push_str(&format!(" ({})", facts.join(", ")));
        }
        if self.foreground {
            line.push_str(&format!(
                "; press Ctrl-C in that terminal, or run `effigy run stop {}`",
                self.run
            ));
        } else {
            line.push_str(&format!("; stop it with `effigy run stop {}`", self.run));
        }
        line
    }
}

fn age_text(started_at_ms: i64) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let elapsed = now.saturating_sub(started_at_ms).max(0) as u64;
    Some(effigy_ui::human::human_duration(Duration::from_millis(
        elapsed,
    )))
}

/// One pure read of the exact environment record (R2): no row, gate or
/// backend is created, and an unreadable record stays unknown.
fn observed_record(ctx: &EnvContext<'_>, env: &EnvRef) -> Observed<BusyRecord> {
    EnvRecord::read(&ctx.observe(), ctx.root, &env.resource).map(|record| BusyRecord {
        state: record.state,
        generation: record.generation.get(),
    })
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
        // The original deadline, kept for the refusal's own report.
        let started = Instant::now();
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
        .map_err(|error| admission_error(ctx, env, wait, started, error))?;
        // The gate is held: only this run writes the row until it settles.
        let began = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
            Observed::Verified(record, _) => record,
            other => {
                let reason = match other {
                    Observed::Unknown(reason) => format!("{reason}"),
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
                Err(error) => {
                    return Err(EnvError::from_run(
                        Some(&self.holder_facts()),
                        &self.env.name,
                        error,
                    ))
                }
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
        let facts = self.holder_facts();
        let mut permit = permit(&mut self.run, exempt).map_err(|error| {
            self.failed = true;
            EnvError::from_run(Some(&facts), &env, error)
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
            .map_err(|error| EnvError::from_run(Some(&facts), &env, error));
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
            return Err(EnvError::from_run(Some(&facts), &env, error));
        }
        outcome.map_err(|error| {
            self.failed = true;
            EnvError::Backend(error)
        })
    }

    /// Publishes `active` and attaches requesting holders.
    pub fn activate(&mut self, holders: &[(RunId, AttachMode)]) -> Result<(), EnvError> {
        let result = Attachments::publish_active_for(&mut self.run, holders);
        self.kernel(result)?;
        self.published = true;
        Ok(())
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
        let facts = self.holder_facts();
        let env = self.env.name.clone();
        let outcome = self
            .run
            .settle(&self.cleanup, &mut EnvSettlement)
            .map_err(|error| EnvError::from_run(Some(&facts), &env, error))?;
        Ok(outcome.cleanup())
    }

    /// The run journal and foreground gates this transition can read, so a
    /// refusal names its holders without a second probe.
    fn holder_facts(&self) -> HolderFacts {
        HolderFacts::observe(
            &self
                .cx
                .operation()
                .clone()
                .observe()
                .expect("observe context"),
            self.root,
        )
    }

    pub(super) fn kernel<T>(&mut self, result: Result<T, RunError>) -> Result<T, EnvError> {
        result.map_err(|error| {
            self.failed = true;
            EnvError::from_run(Some(&self.holder_facts()), &self.env.name, error)
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
