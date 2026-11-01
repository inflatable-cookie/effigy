//! Durable intent, generation fencing and one recoverable settle (§5.6).
//!
//! Engines supply effect observations and idempotent undo. A `Run` is the only
//! writer of its journal; dropping it releases liveness, never claims cleanup.
mod aliases;
mod attachments;
mod children;
mod facts;
mod foreground;
pub use facts::{FactOptions, MachineFacts};
mod history;
pub use foreground::ForegroundRun;
mod checkout_custody;
mod journal;
mod lifecycle;
mod logs;
mod machine;
mod memory;
pub use lifecycle::{metadata, RunMetadata, SentinelStatus};
pub use machine::{
    machine_completion, record_gateway_operator, record_gateway_stop, try_record_gateway_operator,
    MachineCompletion,
};
pub use memory::{ForegroundRecord, Incarnation};
#[path = "../objects/run.rs"]
mod objects;
mod provenance;
pub use provenance::{Caller, OperatorAction};
mod settle;
mod socket;
pub use socket::{connect_control_socket, control_socket_path};
mod terminal;
pub use terminal::{
    read as terminal_record, recent as terminal_records, record as record_terminal, TerminalRecord,
    EVIDENCE as TERMINAL_RECORD_EVIDENCE, KEEP_MS as TERMINAL_RECORD_KEEP_MS,
};

pub(crate) use attachments::{check_activation_fence, holder_attachment, publish_active_state};
/// Disposable-fixture support: record one environment's own `active`
/// publication exactly as an activation commit does, through the same fence
/// check and the same transaction, so a process-lifetime proof can admit a
/// session run without a container activation.
///
/// This is never part of an ordinary build (`test-support` is off by default),
/// and it adds no authority: it writes the row the activation itself writes, for
/// the exact owner fence it is given, and refuses when that fence disagrees with
/// the journal.
#[cfg(feature = "test-support")]
pub fn publish_active_for_fixture(
    cx: &crate::ctx::EffectContext,
    root: &crate::root::StateRoot,
    owner: &Fence,
) -> Result<(), RunError> {
    let mut store = crate::store::Store::open(cx, root)?;
    store.transaction(cx, |txn| {
        attachments::check_activation_fence(txn.conn(), owner)?;
        attachments::publish_active_state(txn, owner)
    })
}

pub use attachments::{
    unfinished_guest_binary, AttachMode, AttachmentTicket, Attachments, GuestBinaryTicket,
    GuestPublicationReceipt, GuestPublicationTicket, Holder, Liveness, TransitionPermit,
    GUEST_BINARY_KIND, GUEST_PUBLICATION_KIND,
};
pub use history::{children, history, recent, summary, unclosed, RunSummary};
pub use logs::{read_log, LogRead};
pub use workspace::{
    attach_secondary, lookup, reserve, resume_owner, resume_secondary, AdmissionKey,
    AdmissionTicket, Attachment, AttachmentId, Chunk, Client, Gap, InvalidHexId, RecordedOutcome,
    Recovered, Release, RequestId, Reservation, Session, UnknownAdmission, CONTROL_MAX, MAX_CHUNK,
    SECONDARY_MAX,
};
mod workspace;
pub(crate) use settle::settle_interrupted_wait;
pub use settle::{settle_interrupted, Reconciled, Settled, SettlementAdapter};

use crate::ctx::{CleanupContext, EffectContext, Interrupt};
use crate::gate::{Gate, GateError, GateGuard, Wait};
use crate::human::Category;
use crate::id::{Generation, ResolvedInvocation, ResourceId, RunId};
use crate::observe::UnknownReason;
use crate::outcome::{Invocation, Outcome, WorkloadOutcome};
use crate::root::StateRoot;
use crate::store::{BusyWait, Store, StoreError, SETTLE_BUSY_WAIT};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Every adapter request and callback carries this resource generation (K7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fence {
    pub resource: ResourceId,
    pub generation: Generation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Settlement {
    Release(Value),
    Reconcile(Value),
    Retain,
    Irreversible,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSpec {
    pub target: Fence,
    /// Engine-owned adapter discriminator; no command grammar in the kernel.
    pub kind: String,
    pub payload: Value,
    pub settlement: Settlement,
    pub required: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Intended,
    Applied,
    Failed,
    Unknown,
    Released,
    Retained,
    Irreversible,
    Residue,
}
impl EffectState {
    /// The state in the words a person reads in a refusal. A settlement report
    /// says what an effect is, not which variant it carries (effigy#402).
    pub fn label(self) -> &'static str {
        match self {
            Self::Intended => "intended and not applied yet",
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Released => "released",
            Self::Retained => "retained",
            Self::Irreversible => "irreversible",
            Self::Residue => "left as residue",
        }
    }
    pub(crate) fn terminal(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Released | Self::Retained | Self::Irreversible
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub sequence: i64,
    pub spec: EffectSpec,
    pub state: EffectState,
    pub receipt: Option<Value>,
    pub reason: Option<String>,
}
/// Not constructible outside the kernel. A ticket belongs to one run and owner
/// generation; callbacks additionally check the target's current generation.
#[derive(Debug, Clone)]
pub struct EffectTicket {
    run: RunId,
    owner: Fence,
    sequence: i64,
}
impl EffectTicket {
    pub(crate) fn sequence(&self) -> i64 {
        self.sequence
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Admitted,
    Starting,
    Active,
    Stopping,
    Closed,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Stopping => "stopping",
            Self::Closed => "closed",
        }
    }
    fn parse(value: &str) -> Result<Self, RunError> {
        match value {
            "admitted" => Ok(Self::Admitted),
            "starting" => Ok(Self::Starting),
            "active" => Ok(Self::Active),
            "stopping" => Ok(Self::Stopping),
            "closed" => Ok(Self::Closed),
            _ => Err(RunError::broken(format!("run phase `{value}`"))),
        }
    }
}
/// Every kernel and run error is exactly one of `human::Category`'s three, and
/// reads through one `Display` per category (§5.5, K4).
#[derive(Debug)]
pub enum RunError {
    Store(StoreError),
    Gate(GateError),
    Interrupted(Interrupt),
    /// A rule said no: another run holds it, a conflict, an inactive
    /// environment, a placement without host authority, an effect class the
    /// operation may not use. The request was well formed.
    Refused(String),
    /// A step the run needed did not answer or could not be reached: a store or
    /// capability failure, I/O, a helper that exited. Nothing is broken.
    Unavailable(String),
    /// Effigy's own data or an invariant is wrong. `check` names it.
    Broken(String),
    StaleGeneration {
        expected: Fence,
        current: Option<Generation>,
    },
    PredecessorUnsettled {
        run: RunId,
        effects: Vec<Effect>,
    },
    HeldBy(Vec<Holder>),
    /// Its environment is not active at the current generation: ensure it.
    EnvNotActive,
    /// A helper's own refusal, with the category it read and the sentence it
    /// already rendered. A parent prints it verbatim, so a helper's refusal is
    /// never framed twice (K4).
    Reported {
        category: Category,
        message: String,
    },
}
impl RunError {
    /// The request was well formed and a rule said no; `rule` names the rule and
    /// any holder.
    pub fn refused(rule: impl Into<String>) -> Self {
        Self::Refused(rule.into())
    }
    /// A step did not answer; the text names what was waited on and why.
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unavailable(detail.into())
    }
    /// A step did not answer, named with the step and its cause.
    pub fn step_failed(step: impl std::fmt::Display, cause: impl std::fmt::Display) -> Self {
        Self::Unavailable(format!("{step} failed ({cause})"))
    }
    /// Effigy's own data or an invariant is wrong; `check` names the check.
    pub fn broken(check: impl Into<String>) -> Self {
        Self::Broken(check.into())
    }
    /// The refusal a helper reported over its event pipe: the category it read
    /// and the sentence it rendered, which this process prints verbatim. A
    /// child's refusal is therefore never relabelled or re-framed by its
    /// parent.
    pub fn reported(category: Category, message: impl Into<String>) -> Self {
        Self::Reported {
            category,
            message: message.into(),
        }
    }
    pub fn category(&self) -> Category {
        match self {
            Self::Store(error) => error.category(),
            Self::Gate(error) => error.category(),
            // A cancellation is not a rule refusing the request: the run was
            // stopped, and nothing about it has to change (F7).
            Self::Interrupted(_) => Category::Unavailable,
            Self::Refused(_) | Self::StaleGeneration { .. } => Category::Refused,
            Self::PredecessorUnsettled { .. } | Self::HeldBy(_) | Self::EnvNotActive => {
                Category::Refused
            }
            Self::Unavailable(_) => Category::Unavailable,
            Self::Reported { category, .. } => *category,
            Self::Broken(_) => Category::Broken,
        }
    }
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A cancellation says what ended the run and asks nothing else of the
        // reader: no category's next step applies (F7).
        if let Self::Interrupted(interrupt @ Interrupt::Cancelled { .. }) = self {
            return write!(f, "the run was {interrupt}");
        }
        let detail = match self {
            Self::Refused(rule) => rule.clone(),
            Self::EnvNotActive => "its environment is not active".to_owned(),
            Self::Unavailable(detail) => detail.clone(),
            Self::Broken(check) => check.clone(),

            Self::StaleGeneration { expected, current } => match current {
                Some(current) => format!(
                    "it expected generation {} of resource `{}`, but the resource is now at generation {}",
                    expected.generation.get(),
                    expected.resource.label(),
                    current.get()
                ),
                None => format!(
                    "it expected generation {} of resource `{}`, which no longer exists",
                    expected.generation.get(),
                    expected.resource.label()
                ),
            },
            Self::PredecessorUnsettled { run, effects } => format!(
                "run {run} has {} effect(s) still unsettled, so settle that run before starting another",
                effects.len()
            ),
            Self::HeldBy(holders) => {
                let named = holders
                    .iter()
                    .map(|holder| holder.run.to_string())
                    .collect::<Vec<_>>();
                format!(
                    "{} still holds a resource it needs ({})",
                    named.len(),
                    named.join(", ")
                )
            }
            Self::Interrupted(interrupt @ Interrupt::TimedOut { .. }) => interrupt.to_string(),
            // Cancellation rendered above.
            Self::Interrupted(Interrupt::Cancelled { .. }) => unreachable!(),
            Self::Store(error) => return write!(f, "{error}"),
            Self::Gate(error) => return write!(f, "{error}"),
            Self::Reported { message, .. } => return f.write_str(message),
        };
        f.write_str(&self.category().render("the run", &detail))
    }
}
impl std::error::Error for RunError {}
impl From<StoreError> for RunError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<GateError> for RunError {
    fn from(e: GateError) -> Self {
        Self::Gate(e)
    }
}
impl From<rusqlite::Error> for RunError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<std::io::Error> for RunError {
    fn from(e: std::io::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<Interrupt> for RunError {
    fn from(e: Interrupt) -> Self {
        Self::Interrupted(e)
    }
}
impl From<serde_json::Error> for RunError {
    fn from(e: serde_json::Error) -> Self {
        Self::Store(StoreError::Broken(format!(
            "a value this store wrote is not readable ({e})"
        )))
    }
}

#[derive(Debug)]
pub struct BeginOpts {
    /// Supplied entropy and time are engine-owned. Reuse is refused by the store.
    pub id: RunId,
    pub wait: Wait,
    /// An explicit operator acknowledgement, scoped to these predecessors only.
    pub acknowledge: Vec<RunId>,
    pub next: Invocation,
    /// Planned custody requires a re-executable sentinel before any workload.
    pub custody: Option<crate::supervise::LifecyclePlan>,
}
pub struct Run<'a> {
    root: &'a StateRoot,
    store: Store,
    guard: GateGuard,
    cx: EffectContext,
    id: RunId,
    owner: Fence,
    next: Invocation,
    /// A failed journal write prevents further effects until settle.
    poisoned: bool,
    // Natural completion evidence is local to the live owner. Recovery never
    // manufactures success; only finish durably records the validated outcome.
    completed: std::collections::BTreeMap<i64, crate::outcome::RequiredObligation>,
    sentinel: Option<crate::supervise::SentinelLease>,
    machine_startup: Option<machine::Startup>,
    checkout_custody: Option<checkout_custody::CheckoutCustody>,
    ancestor_closure: Option<crate::closure::ClosurePin>,
    closed_effects: Option<Vec<Effect>>,
    processes: std::collections::BTreeMap<
        i64,
        std::sync::Arc<std::sync::Mutex<crate::custody::ProcessCore>>,
    >,
    /// The session's one live listener claim, when this run is a workspace
    /// session. A second launch for the same session refuses instead of opening
    /// a second listener and key under one session's custody.
    session_launch: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
}

impl std::fmt::Debug for Run<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run")
            .field("id", &self.id)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}
impl<'a> Run<'a> {
    pub fn begin(
        cx: &EffectContext,
        root: &'a StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Self, RunError> {
        Self::begin_under(
            cx,
            root,
            inv,
            opts,
            || Ok(cleanup.clone()),
            adapter,
            None,
            None,
            None,
            None,
        )
    }

    /// [`Run::begin`] with the cleanup window made once the gate is held.
    ///
    /// `cleanup` is called after `Gate(Env)` is acquired and before any
    /// predecessor is settled, so a wait for the gate cannot spend the budget
    /// that settlement needs (effigy#447). A caller whose admission waits on a
    /// real deadline uses this; every other caller keeps [`Run::begin`].
    pub fn begin_with_fresh_cleanup(
        cx: &EffectContext,
        root: &'a StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: impl FnOnce() -> Result<CleanupContext, RunError>,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Self, RunError> {
        Self::begin_under(
            cx, root, inv, opts, cleanup, adapter, None, None, None, None,
        )
    }

    /// Admit a run bound to a keyed workspace request (§8.11). The reservation,
    /// the run row and the key's `admitted` state commit in one transaction, so
    /// no reader ever sees an admitted binding without its run. A reservation
    /// that never reaches this commit remains Unknown and names no run.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_admitted(
        cx: &EffectContext,
        root: &'a StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        ticket: AdmissionTicket,
    ) -> Result<Self, RunError> {
        if ticket.run() != opts.id {
            return Err(RunError::broken(
                "an admission ticket names a different run than the one it admits",
            ));
        }
        Self::begin_under(
            cx,
            root,
            inv,
            opts,
            || Ok(cleanup.clone()),
            adapter,
            None,
            None,
            Some(ticket),
            None,
        )
    }

    /// Admit the environment activation run at the generation a workspace
    /// activation reserved (§8.7), binding that reservation in the admission
    /// transaction.
    ///
    /// The adopted generation is the one the environment's counter would
    /// advance to next, so a reservation grants nothing the gate and the counter
    /// do not already confirm: it only names E early enough for the Task-owned
    /// session to be admitted (under `Gate(Task)` alone) before this run takes
    /// `Gate(Env)`. A reservation that already names another run, one whose
    /// generation another activation consumed, or one that is committed or
    /// cancelled refuses before the run row exists.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_claimed(
        cx: &EffectContext,
        root: &'a StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: impl FnOnce() -> Result<CleanupContext, RunError>,
        adapter: &mut impl SettlementAdapter,
        claim: crate::workspace_session::ActivationClaim,
    ) -> Result<Self, RunError> {
        Self::begin_under(
            cx,
            root,
            inv,
            opts,
            cleanup,
            adapter,
            None,
            None,
            None,
            Some(claim),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_under(
        cx: &EffectContext,
        root: &'a StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: impl FnOnce() -> Result<CleanupContext, RunError>,
        adapter: &mut impl SettlementAdapter,
        parent: Option<RunId>,
        planned: Option<EffectSpec>,
        admission: Option<AdmissionTicket>,
        claim: Option<crate::workspace_session::ActivationClaim>,
    ) -> Result<Self, RunError> {
        crate::fault_point!(BeginBefore)?;
        if root.is_machine()
            && (inv.spec().task.checkout().kind() != crate::id::CheckoutKind::Machine
                || !inv.spec().owner.is_machine_fact()
                || inv.spec().gates.iter().any(|id| !id.is_machine_fact())
                || !inv.spec().attachments.is_empty())
        {
            return Err(RunError::refused("machine runs require shared-fact owners and machine identity; checkout work belongs in its own journal"));
        }
        if inv.spec().task.checkout().kind() == crate::id::CheckoutKind::Filesystem {
            return Err(RunError::refused(
                "filesystem checkout identities cannot own durable runs",
            ));
        }
        if cx.operation().placement() != crate::ctx::Placement::Host
            || inv.spec().placement != crate::ctx::Placement::Host
        {
            return Err(RunError::refused(
                "a managed run needs host authority, and this placement is a guest",
            ));
        }
        if cx.operation().class() != inv.spec().class {
            return Err(RunError::refused("invocation class differs from context"));
        }
        cx.operation().check("begin")?;
        let session_binding = claim.as_ref().and_then(|claim| claim.session_binding());
        // Only a frozen workspace-session claim may use its private liveness
        // slot. It keeps the same logical Task owner and admits no generic
        // predecessor recovery. No Env gate is acquired on this path.
        let gate_owner = match session_binding {
            Some(binding) => {
                if !inv.spec().gates.is_empty() || !inv.spec().attachments.is_empty() {
                    return Err(RunError::refused(
                        "reserved workspace admission takes only its session slot",
                    ));
                }
                let store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
                crate::workspace_session::session_run_gate(
                    store.conn(),
                    &binding.owner,
                    binding.run,
                )?
            }
            None => inv.spec().owner.clone(),
        };
        let ids = if session_binding.is_some() {
            aliases::gates(root, &gate_owner, std::slice::from_ref(&gate_owner))?
        } else {
            let mut ids = inv.spec().gates.clone();
            ids.push(inv.spec().owner.clone());
            aliases::gates(root, &inv.spec().owner, &ids)?
        };
        let mut guard = Gate::acquire(cx, root, &ids, opts.wait)?;
        crate::fault_point!(BeginGated)?;
        // Only an owner gate may span recovery callbacks or later effects (K14).
        guard.retain_owner(&gate_owner);
        // #275: one bounded admission sweep, never a close-time scan. A
        // refused alias gate creates no journal merely to run maintenance.
        if !root.is_machine() {
            crate::retention::sweep(cx, root);
        }
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        // A refused writer wait names this run, not a nameless process.
        store.set_run(opts.id);
        // The gate is held: only now does the settlement window start, so the
        // wait above cannot spend it (effigy#447).
        let cleanup = cleanup()?;
        if root.is_machine() {
            settle::compact_machine_closed(&mut store, &inv.spec().owner, &cleanup)?;
        }
        let mut predecessors = Vec::new();
        if session_binding.is_none() {
            crate::workspace_session::refuse_session_overlap(store.conn(), &inv.spec().owner)?;
        }
        for owner in guard.ids().filter(|_| session_binding.is_none()) {
            crate::workspace_session::refuse_bound_reservation_recovery(store.conn(), owner)?;
            predecessors.extend(
                journal::predecessors(store.conn(), owner)?
                    .into_iter()
                    .map(|record| (owner.clone(), record)),
            );
        }
        let admit = |txn: &mut crate::store::Txn<'_>| {
            if session_binding.is_none() {
                let reserved: bool = txn.conn().query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspace_session_runs WHERE run_id=?1)",
                    [opts.id.to_string()],
                    |row| row.get(0),
                )?;
                if reserved {
                    return Err(RunError::refused(
                        "this run identity belongs to a frozen workspace session",
                    ));
                }
            }
            if admission.is_none() {
                // A keyed workspace reservation keeps its run identity until it
                // is admitted or its session generation ends.
                workspace::refuse_unkeyed(txn, opts.id)?;
            }
            let identity = if admission.is_some() || claim.is_some() {
                Some(serde_json::to_string(
                    &crate::ident::process_identity(std::process::id())
                        .verified()
                        .ok_or_else(|| RunError::unavailable("owner identity unavailable"))?,
                )?)
            } else {
                None
            };
            txn.resource(&inv.spec().owner, "run")?;
            let generation = if let Some(binding) = session_binding {
                binding.generation
            } else {
                crate::workspace_session::refuse_session_overlap(txn.conn(), &inv.spec().owner)?;
                txn.next_generation(&inv.spec().owner)?
            };
            match &claim {
                // The reserved generation is claimed in this transaction: a
                // crash leaves either no run and an unclaimed reservation, or
                // one admitted run bound to it.
                Some(claim) => {
                    claim.bind(txn, &inv.spec().owner, generation, opts.id)?;
                }
                // A reserved generation is not admitted around its
                // reservation: a competing environment activation is refused
                // by name instead of taking the generation the staged session
                // is labelled with.
                None => crate::workspace_session::refuse_reserved_generation(
                    txn.conn(),
                    &inv.spec().owner,
                    generation,
                )?,
            }
            txn.conn().execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,invocation,digest,created_at,updated_at,workload_outcome,parent_run,owner_identity) \
                VALUES (?1,?2,?3,?4,'host','admitted',?5,?6,?7,?7,?8,?9,?10)", params![opts.id.to_string(),
                inv.spec().owner.name().to_string(), generation.get() as i64,
                if opts.custody.is_some() {"custody"}else{"operation"}, serde_json::to_string(inv)?, inv.digest().0.as_slice(), txn.now(), if opts.custody.is_some() {Some(serde_json::to_string(&WorkloadOutcome::NotStarted)?)}else{None}, parent.map(|id| id.to_string()), identity])?;
            if let Some(ticket) = &admission {
                workspace::bind(txn, ticket)?;
            }
            if let Some(planned) = &planned {
                let fence = Fence {
                    resource: inv.spec().owner.clone(),
                    generation,
                };
                journal::insert_effect(txn, opts.id, &socket::intent(fence.clone()))?;
                let mut planned = planned.clone();
                planned.target = fence;
                journal::insert_effect(txn, opts.id, &planned)?;
            }
            Ok::<_, RunError>(generation)
        };
        // A keyed run and its consumed ticket commit before predecessor effects.
        // A crash during recovery leaves one admitted run to recover, never a
        // reservation that could be handed off again after effects occurred.
        let admitted_generation = if admission.is_some() {
            let generation =
                store.transaction(&cx.child(std::time::Duration::from_secs(30)), admit)?;
            guard.set_run(&opts.id);
            Some(generation)
        } else {
            None
        };
        for (owner, predecessor) in predecessors {
            let mut previous = Self {
                root,
                store,
                guard,
                cx: cx.clone(),
                id: predecessor.id,
                owner: Fence {
                    resource: owner,
                    generation: predecessor.generation,
                },
                next: opts.next.clone(),
                poisoned: false,
                completed: Default::default(),
                sentinel: None,
                closed_effects: None,
                machine_startup: None,
                checkout_custody: None,
                ancestor_closure: None,
                processes: Default::default(),
                session_launch: Default::default(),
            };
            previous.settle_recovering(
                &cleanup,
                adapter,
                admitted_generation.map(|generation| (opts.id, generation)),
            )?;
            let residue = previous
                .effects()?
                .into_iter()
                .filter(|e| !e.state.terminal())
                .collect::<Vec<_>>();
            if !residue.is_empty() && !predecessor.acknowledged {
                if !opts.acknowledge.contains(&predecessor.id) {
                    return Err(RunError::PredecessorUnsettled {
                        run: predecessor.id,
                        effects: residue,
                    });
                }
                previous.store.transaction_within(
                    &settle::cleanup_effects(&cleanup)?,
                    BusyWait::AtLeast(SETTLE_BUSY_WAIT),
                    |txn| {
                        txn.conn().execute(
                            "UPDATE runs SET residue_ack=?2 WHERE run_id=?1",
                            params![predecessor.id.to_string(), txn.now()],
                        )?;
                        Ok::<_, RunError>(())
                    },
                )?;
            }
            store = previous.store;
            guard = previous.guard;
        }
        crate::fault_point!(BeginRecovered)?;
        let generation = match admitted_generation {
            Some(generation) => generation,
            None => store.transaction(&cx.child(std::time::Duration::from_secs(30)), admit)?,
        };
        crate::fault_point!(BeginAdmitted)?;
        guard.set_run(&opts.id); // Diagnostics never affect outcome (K10).
        let mut run = Self {
            root,
            store,
            guard,
            cx: cx.clone(),
            id: opts.id,
            owner: Fence {
                resource: inv.spec().owner.clone(),
                generation,
            },
            next: opts.next.clone(),
            poisoned: false,
            completed: Default::default(),
            sentinel: None,
            closed_effects: None,
            machine_startup: planned.map(|_| machine::Startup::default()),
            checkout_custody: None,
            ancestor_closure: None,
            processes: Default::default(),
            session_launch: Default::default(),
        };
        if inv.spec().owner.kind() == crate::id::ResourceKind::Env {
            if let crate::id::Scope::Checkout(checkout) = inv.spec().owner.scope() {
                run.protect_checkout(checkout, opts.wait)?;
            }
        }
        if let Some(plan) = opts.custody {
            let registration = crate::supervise::spawn_sentinel(&run, root, &plan, &opts.next);
            match registration.and_then(|registration| run.register_lifecycle(registration, &plan))
            {
                Ok(()) => {}
                Err(error) => {
                    run.settle_inner(&cleanup, adapter)?;
                    return Err(error);
                }
            }
        }
        Ok(run)
    }
    fn holds_owner_gate(&self) -> Result<bool, RunError> {
        let gate = crate::workspace_session::session_run_gate(
            self.store.conn(),
            &self.owner.resource,
            self.id,
        )?;
        Ok(self.guard.covers(&gate))
    }

    /// Borrowing this exact owner keeps its Task-ranked liveness slot held
    /// through activation commit. A cross-process probe cannot provide this
    /// guarantee: the probed process may die before the transaction starts.
    /// The owning proof a channel listener may be launched under.
    ///
    /// Only a run whose private session slot the journal records as an admitted
    /// workspace session yields one, so a listener and key are always launched
    /// for the detached session owner itself, never for a binding a caller
    /// assembled or for an environment run.
    /// Claim the session's one live listener for this run.
    ///
    /// A workspace session has exactly one listener and key: a second launch for
    /// a session whose listener is already live refuses here, so one session can
    /// never serve through two owners. The claim is released when the launch
    /// stops or its run settles.
    pub(crate) fn claim_session_launch(
        &self,
    ) -> Result<
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        crate::workspace_session::ServingRefusal,
    > {
        use std::sync::atomic::Ordering;
        let mut slot = self.session_launch.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            if existing.load(Ordering::SeqCst) {
                return Err(crate::workspace_session::ServingRefusal::AlreadyPublished);
            }
        }
        let claim = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        *slot = Some(claim.clone());
        Ok(claim)
    }

    pub fn session_owner(&mut self) -> Result<crate::workspace_session::SessionOwner, RunError> {
        self.can_effect()?;
        if self.owner.resource.kind() != crate::id::ResourceKind::Task {
            return Err(RunError::refused(
                "only an admitted workspace session owns a channel listener",
            ));
        }
        // Custody is a detached registration the serving supervisor made, on a
        // run the journal records as this workspace session.
        let registered: bool = self.store.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_session_custody WHERE run_id=?1)",
            params![self.id.to_string()],
            |row| row.get(0),
        )?;
        if !registered {
            return Err(RunError::refused(
                "this run holds no detached session custody registration",
            ));
        }
        let known: bool = self.store.conn().query_row(
            "SELECT (EXISTS(SELECT 1 FROM workspace_session_runs \
             WHERE run_id=?1 AND owner=?2 AND generation=?3) \
             OR EXISTS(SELECT 1 FROM workspace_sessions \
             WHERE run_id=?1 AND session_generation=?3)) \
             AND EXISTS(SELECT 1 FROM runs WHERE run_id=?1)",
            params![
                self.id.to_string(),
                self.owner.resource.name().to_string(),
                self.owner.generation.get() as i64
            ],
            |row| row.get(0),
        )?;
        if !known {
            return Err(RunError::refused(
                "only an admitted workspace session owns a channel listener",
            ));
        }
        // The proof names the exact detached run. The session's gate stays with
        // that run, so settlement releases it; a launch only serves while the
        // run it names is still open (and can abandon the proof explicitly).
        Ok(crate::workspace_session::SessionOwner::of(
            self.id,
            self.owner.clone(),
        ))
    }

    /// Register this run as a detached session's custody owner, on behalf of
    /// the serving supervisor that holds it. The detached supervisor performs
    /// this when it adopts the session; a run without it is not a custody
    /// owner, however it was admitted.
    pub(crate) fn register_session_custody(&mut self, supervisor_id: &str) -> Result<(), RunError> {
        self.can_effect()?;
        if self.owner.resource.kind() != crate::id::ResourceKind::Task
            || supervisor_id.trim().is_empty()
        {
            return Err(RunError::refused(
                "only a detached workspace session registers custody, under a named supervisor",
            ));
        }
        if crate::workspace_session::session_of_run(self.store.conn(), self.id)?.is_none() {
            return Err(RunError::refused(
                "only a recorded workspace session registers custody",
            ));
        }
        self.store.transaction(&self.cx, |txn| {
            txn.conn().execute(
                "INSERT INTO workspace_session_custody(run_id,supervisor,created_at) VALUES (?1,?2,?3) \
                 ON CONFLICT(run_id) DO UPDATE SET supervisor=excluded.supervisor",
                params![self.id.to_string(), supervisor_id, txn.now()],
            )?;
            Ok::<_, RunError>(())
        })
    }

    /// Re-read the exact reserved activation while this run holds its Env gate.
    /// A caller may use this before an external observation, but it is not a
    /// substitute for revalidating target or creator evidence afterwards.
    pub fn verify_workspace_reservation(
        &self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        reservation.verify_claimed_run(self.store.conn(), self.id, &self.owner)?;
        Ok(())
    }

    pub(crate) fn owns_workspace_session(
        &self,
        root: &StateRoot,
        binding: &crate::workspace_session::SessionBinding,
    ) -> Result<bool, RunError> {
        self.can_effect()?;
        Ok(self.root.path() == root.path()
            && self.id == binding.run
            && self.owner.resource == binding.owner
            && self.owner.generation == binding.generation
            && self.holds_owner_gate()?)
    }

    pub(crate) fn capture_log(
        &self,
        sequence: i64,
        stream: crate::custody::Stream,
        limits: &crate::ctx::Limits,
    ) -> std::io::Result<Option<crate::custody::capture::Log>> {
        if self.root.is_machine() {
            return Ok(None);
        }
        crate::custody::capture::Log::open(self.root, self.id, sequence, stream, limits).map(Some)
    }
    /// The reserved capture-failure effect is owned by the kernel, never an
    /// engine-provided kind. It survives missing/unusable output files.
    pub(crate) fn record_capture_failure(&mut self, reason: &'static str) -> Result<(), RunError> {
        self.verify_custody()?;
        let ticket = self.intend_inner(EffectSpec {
            target: self.fence().clone(),
            kind: "kernel:capture-truncated".into(),
            payload: serde_json::json!({"version":1,"sequence":crate::supervise::OUTPUT_SEQUENCE,"reason":reason}),
            settlement: Settlement::Retain,
            required: false,
        })?;
        self.applied(&ticket, serde_json::json!({"truncated":true}))
    }
    /// Capture bounds of the run's own effect context.
    pub(crate) fn limits(&self) -> crate::ctx::Limits {
        self.cx.operation().limits().clone()
    }
    /// Recheck the filesystem generations retained by this run before continuing work.
    pub fn verify_custody(&self) -> Result<(), RunError> {
        self.verify_checkout_custody()
    }
    pub(crate) fn has_sentinel(&self) -> bool {
        self.sentinel.is_some()
    }
    pub(crate) fn intend_remote(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.intend_inner(spec)
    }
    pub(crate) fn intend_process(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.intend_inner(spec)
    }
    pub(crate) fn retain_process(
        &mut self,
        sequence: i64,
        core: std::sync::Arc<std::sync::Mutex<crate::custody::ProcessCore>>,
    ) {
        self.processes.insert(sequence, core);
    }
    pub(crate) fn rollback_processes(
        &mut self,
        cx: &CleanupContext,
    ) -> Vec<crate::custody::StopReport> {
        self.processes
            .values()
            .rev()
            .map(|p| {
                p.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .stop(cx)
            })
            .collect()
    }
    pub fn id(&self) -> RunId {
        self.id
    }
    /// Whether this run holds a sentinel, so its death is settled from the
    /// journal. An engine that admits a run with an obligation to settle (a
    /// process group) must have one: without it, a killed owner leaves the run
    /// `starting` forever and nothing is cleaned up.
    pub fn owns_sentinel(&self) -> bool {
        self.sentinel.is_some()
    }
    pub fn fence(&self) -> &Fence {
        &self.owner
    }
    /// The opaque engine payload of this run's gated resource. Observers use
    /// `StoreReader::resource`; this read also works before first publication.
    pub fn resource_state(&self) -> Result<Option<Value>, RunError> {
        let state: Option<String> = self.store.conn().query_row(
            "SELECT state FROM resources WHERE name=?1",
            [self.owner.resource.name().to_string()],
            |r| r.get(0),
        )?;
        Ok(state.map(|s| serde_json::from_str(&s)).transpose()?)
    }
    pub fn phase(&self) -> Result<Phase, RunError> {
        if self.closed_effects.is_some() {
            return Ok(Phase::Closed);
        }
        Ok(journal::run(self.store.conn(), self.id)?.phase)
    }
    pub fn effects(&self) -> Result<Vec<Effect>, RunError> {
        if let Some(effects) = &self.closed_effects {
            return Ok(effects.clone());
        }
        journal::effects(self.store.conn(), self.id)
    }

    /// Publish a file using this run's held store gate. The engine records
    /// its intent before calling this; the gate itself never escapes the run.
    pub fn replace_record(
        &self,
        dir: &crate::fs::DirFd,
        name: &crate::fs::FileName,
        bytes: &[u8],
        opts: crate::record::ReplaceOpts,
    ) -> Result<crate::record::Replaced, RunError> {
        self.can_effect()?;
        Ok(crate::record::atomic_replace(
            &self.cx,
            &self.guard,
            dir,
            name,
            bytes,
            opts,
        )?)
    }

    pub fn intend(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        // Destructive environment mutation needs TransitionPermit. Only the typed
        // Attachments guest-publication operation admits additive immutable writes.
        if spec.target.resource.kind() == crate::id::ResourceKind::Env {
            return Err(RunError::broken(
                "environment effect needs a transition permit",
            ));
        }
        if spec.kind.starts_with("kernel:") {
            return Err(RunError::refused("reserved effect kind"));
        }
        self.intend_inner(spec)
    }
    fn intend_inner(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.can_effect()?;
        if self
            .machine_startup
            .as_ref()
            .is_some_and(|startup| !startup.active)
        {
            return self.planned_intent(&spec);
        }
        let result = (|| {
            crate::fault_point!(IntendBefore)?;
            let sequence = self.store.transaction(
                &self.cx.child(std::time::Duration::from_secs(30)),
                |txn| {
                    journal::check_fence(txn.conn(), &self.owner)?;
                    if spec.target != self.owner {
                        journal::check_intent_target(txn.conn(), self.id, &spec)?;
                    }
                    journal::insert_effect(txn, self.id, &spec)
                },
            )?;
            crate::fault_point!(IntendAfter)?;
            Ok(EffectTicket {
                run: self.id,
                owner: self.owner.clone(),
                sequence,
            })
        })();
        // A stale target is refused inside the transaction before any row is
        // written: the caller may re-read the fence and intend again, as an
        // attach retries. Every other failure is ambiguous and poisons.
        if result
            .as_ref()
            .is_err_and(|error| !matches!(error, RunError::StaleGeneration { .. }))
        {
            self.poisoned = true;
        }
        result
    }
    pub fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
        if self
            .machine_startup
            .as_ref()
            .is_some_and(|startup| !startup.active)
        {
            return self.planned_receipt(ticket, receipt);
        }
        self.callback(ticket, EffectState::Applied, Some(receipt), None)
    }
    /// Only verified non-occurrence permits Failed; uncertain attempts use unknown.
    pub fn failed(
        &mut self,
        ticket: &EffectTicket,
        proof: crate::observe::AbsenceProof,
    ) -> Result<(), RunError> {
        self.callback(
            ticket,
            EffectState::Failed,
            Some(serde_json::to_value(proof)?),
            None,
        )
    }
    pub fn unknown(
        &mut self,
        ticket: &EffectTicket,
        reason: UnknownReason,
    ) -> Result<(), RunError> {
        self.callback(
            ticket,
            EffectState::Unknown,
            None,
            Some(format!("{reason}")),
        )
    }
    fn callback(
        &mut self,
        ticket: &EffectTicket,
        state: EffectState,
        receipt: Option<Value>,
        reason: Option<String>,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        if ticket.run != self.id || ticket.owner != self.owner {
            return Err(RunError::broken("foreign effect ticket"));
        }
        let result = (|| {
            if state == EffectState::Failed {
                crate::fault_point!(FailedBefore)?;
            } else {
                crate::fault_point!(AppliedBefore)?;
            }
            self.store
                .transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                    let effect = journal::effect(txn.conn(), self.id, ticket.sequence)?;
                    journal::check_run_target(txn.conn(), self.id, &self.owner, &effect)?;
                    if !matches!(effect.state, EffectState::Intended | EffectState::Unknown) {
                        return Err(RunError::broken("effect already completed"));
                    }
                    journal::write_effect(
                        txn,
                        self.id,
                        ticket.sequence,
                        state,
                        receipt.as_ref(),
                        reason.as_deref(),
                    )
                })?;
            if state == EffectState::Failed {
                crate::fault_point!(FailedAfter)?;
            } else {
                crate::fault_point!(AppliedAfter)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Whether this run may still record an effect. The workspace activation
    /// seam asks it before a commit publishes any state from the run.
    pub(crate) fn can_effect(&self) -> Result<(), RunError> {
        self.require_environment_gate()?;
        self.cx.operation().check("effect")?;
        if self.poisoned || matches!(self.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::broken("run must settle before more effects"));
        }
        Ok(())
    }
    fn require_environment_gate(&self) -> Result<(), RunError> {
        if self.owner.resource.kind() == crate::id::ResourceKind::Env
            && !self.guard.covers(&self.owner.resource)
        {
            return Err(RunError::refused(
                "environment run does not hold its owner gate",
            ));
        }
        Ok(())
    }
    pub fn advance(&mut self, phase: Phase) -> Result<(), RunError> {
        self.can_effect()?;
        let previous = self.phase()?;
        if self.machine_lifecycle() && previous == Phase::Active && phase == Phase::Active {
            return Ok(());
        }
        if !matches!(
            (previous, phase),
            (Phase::Starting, Phase::Active) | (_, Phase::Stopping)
        ) {
            return Err(RunError::broken("invalid phase transition"));
        }
        if phase == Phase::Active
            && self
                .effects()?
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(RunError::broken("startup obligations unresolved"));
        }
        let result = (|| {
            crate::fault_point!(PhaseBefore)?;
            self.store
                .transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                    txn.conn().execute(
                        "UPDATE runs SET phase=?2,updated_at=?3 WHERE run_id=?1",
                        params![self.id.to_string(), phase.name(), txn.now()],
                    )?;
                    Ok::<_, RunError>(())
                })?;
            crate::fault_point!(PhaseAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Fenced natural completion callback. Merely applying an effect (for
    /// example, spawning a process) is not successful completion (R5).
    pub fn completed(
        &mut self,
        ticket: &EffectTicket,
        obligation: crate::outcome::RequiredObligation,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        if ticket.run != self.id || ticket.owner != self.owner {
            return Err(RunError::broken("foreign completion ticket"));
        }
        journal::check_run_fence(self.store.conn(), self.id, &self.owner)?;
        let effect = journal::effect(self.store.conn(), self.id, ticket.sequence)?;
        journal::check_run_target(self.store.conn(), self.id, &self.owner, &effect)?;
        if effect.state != EffectState::Applied || self.completed.contains_key(&ticket.sequence) {
            return Err(RunError::broken("effect is not awaiting completion"));
        }
        self.completed.insert(ticket.sequence, obligation);
        // The observed exit joins the receipt for status readers. It is
        // evidence of what ended, never of success (R5), and a failed write
        // changes nothing about the run (K10).
        if let crate::outcome::RequiredObligation::ProcessExit(code) = obligation {
            if let Some(Value::Object(mut receipt)) = effect.receipt {
                receipt.insert("exit".into(), Value::from(code));
                let receipt = Value::Object(receipt);
                let _ = self.store.transaction(
                    &self.cx.child(std::time::Duration::from_secs(30)),
                    |txn| {
                        journal::write_effect(
                            txn,
                            self.id,
                            ticket.sequence,
                            EffectState::Applied,
                            Some(&receipt),
                            None,
                        )
                    },
                );
            }
        }
        Ok(())
    }
    /// Records natural completion/failure before cleanup. Recovery never infers
    /// success from absence, receipts, or a free gate (R5).
    pub fn finish(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        self.can_effect()?;
        let obligations = self
            .effects()?
            .iter()
            .filter(|e| e.spec.required)
            .map(|e| {
                if e.spec.kind == children::KIND {
                    self.child_obligation(e)
                } else if e.state == EffectState::Applied
                    && e.spec.kind.starts_with("kernel:")
                    && e.spec.kind != crate::custody::KIND
                    && e.spec.kind != crate::remote::KIND
                {
                    crate::outcome::RequiredObligation::Applied
                } else {
                    self.completed
                        .get(&e.sequence)
                        .copied()
                        .unwrap_or(crate::outcome::RequiredObligation::NotCompleted)
                }
            })
            .collect::<Vec<_>>();
        Outcome::new(
            Ok(()),
            workload.clone(),
            crate::outcome::CleanupOutcome::NotNeeded,
            &obligations,
        )
        .map_err(|_| RunError::broken("workload obligations incomplete"))?;
        let result = (|| {
            self.store.transaction_within(
                &self.cx,
                BusyWait::AtLeast(SETTLE_BUSY_WAIT),
                |txn| {
                    journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                    crate::fault_point!(PhaseBefore)?;
                    txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",
                        params![self.id.to_string(), serde_json::to_string(&workload)?, txn.now()])?;
                    Ok::<_, RunError>(())
                },
            )?;
            crate::fault_point!(PhaseAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    /// Stop outcomes use the independent cleanup budget, never a cancelled
    /// workload token. Natural success still requires finish's obligations.
    pub fn interrupt(
        &mut self,
        cleanup: &CleanupContext,
        workload: WorkloadOutcome,
    ) -> Result<(), RunError> {
        self.require_environment_gate()?;
        if !matches!(
            workload,
            WorkloadOutcome::Cancelled { .. }
                | WorkloadOutcome::TimedOut { .. }
                | WorkloadOutcome::NotStarted
        ) || self.phase()? == Phase::Closed
        {
            return Err(RunError::broken("invalid interruption outcome"));
        }
        let cx = settle::cleanup_effects(cleanup)?;
        let machine_lifecycle = self.machine_lifecycle();
        self.store.transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
            journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
            txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",params![self.id.to_string(),serde_json::to_string(&workload)?,txn.now()])?;
            if machine_lifecycle {
                txn.conn().execute("UPDATE requests SET state='resolved',resolved_at=?2 WHERE run_id=?1 AND state='pending'", params![self.id.to_string(),txn.now()])?;
            }
            Ok::<_,RunError>(())
        })
    }
    /// Register an action the run performs when it settles (or is dropped
    /// un-settled). A session's listener and key custody are released this way,
    /// so settlement closes the socket and wipes the secret.
    pub fn on_settle(&mut self, action: impl FnOnce() + Send + 'static) {
        self.guard.on_release(action);
    }

    /// Run everything registered with [`Run::on_settle`], exactly once.
    fn run_settle_actions(&mut self) {
        self.guard.release_actions();
    }

    pub fn settle(
        mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Outcome<Settled>, RunError> {
        self.require_environment_gate()?;
        self.run_settle_actions();
        self.settle_inner(cleanup, adapter)
    }

    /// Keep the exact owner gate on a failed settlement so its caller can
    /// reconcile the durable outcome instead of mistaking drop for closure.
    pub fn settle_recoverable(
        mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Outcome<Settled>, (RunError, Box<Self>)> {
        if let Err(error) = self.require_environment_gate() {
            return Err((error, Box::new(self)));
        }
        self.run_settle_actions();
        match self.settle_inner(cleanup, adapter) {
            Ok(outcome) => Ok(outcome),
            Err(error) => Err((error, Box::new(self))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RequestAction {
    Stop,
    Escalate,
    Settle,
    Detach,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: RunId,
    pub run: RunId,
    pub expected: Fence,
    pub action: RequestAction,
}
/// Requests never acquire the owner's gate. Only a current generation accepts
/// them; owner handling rechecks the same fence immediately before acting.
pub fn request(cx: &EffectContext, root: &StateRoot, request: &Request) -> Result<(), RunError> {
    request_bounded(cx, root, request, None, None)
}
/// One control request, with an optional bound on how many may wait at once and
/// an optional client attachment it must still hold. Both the bound and the
/// attachment row are checked in the transaction that inserts the request, so a
/// concurrent sender cannot exceed the bound and a release that lands first
/// cannot be overtaken by a control (§8.11).
pub(super) fn request_bounded(
    cx: &EffectContext,
    root: &StateRoot,
    request: &Request,
    max_pending: Option<usize>,
    attached: Option<&Attachment>,
) -> Result<(), RunError> {
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::refused("request needs Mutate"))?;
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| {
            RunError::refused("a managed run needs host authority, and this placement is a guest")
        })?;
    // Workspace controls respect their caller's budget (at most one second),
    // rather than inheriting the patient thirty-second settlement writer wait.
    let bounded = cx.child(std::time::Duration::from_secs(1));
    let (cx, wait) = if max_pending.is_some() {
        (&bounded, BusyWait::Context)
    } else {
        (cx, BusyWait::AtLeast(crate::store::STORE_BUSY_WAIT))
    };
    let mut store = Store::open_within(cx, root, wait)?;
    crate::fault_point!(RequestBefore)?;
    store.transaction_within(cx, wait, |txn| {
        journal::check_run_fence(txn.conn(), request.run, &request.expected)?;
        let record = journal::run(txn.conn(), request.run)?;
        if record.owner != request.expected.resource.name().to_string() || record.generation != request.expected.generation || record.phase == Phase::Closed {
            return Err(RunError::refused("request run is not current"));
        }
        if let Some(attachment) = attached {
            workspace::require_live_owner(&cx.operation().observe().map_err(|_| RunError::refused("observation required"))?, root, txn.conn(), request.run)?;
            if !workspace::attached_row(txn.conn(), attachment.id(), attachment)? {
                return Err(RunError::refused(
                    "this attachment is no longer attached to the run",
                ));
            }
        }
        if let Some(max_pending) = max_pending {
            let pending: i64 = txn.conn().query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
                [request.run.to_string()],
                |r| r.get(0),
            )?;
            if pending >= max_pending as i64 {
                return Err(RunError::refused(format!(
                    "this run already has {max_pending} control requests waiting"
                )));
            }
        }
        txn.conn().execute("INSERT INTO requests(request_id,run_id,expected_generation,action,state,created_at) VALUES (?1,?2,?3,?4,'pending',?5)",
            params![request.id.to_string(), request.run.to_string(), request.expected.generation.get() as i64,
                serde_json::to_string(request)?, txn.now()])?;
        Ok::<_, RunError>(())
    })?;
    crate::fault_point!(RequestAfter)?;
    Ok(())
}
/// Decode only the mailbox's exact run and generation binding, including its
/// serialized payload. Recovery may read an older generation only under the
/// live owner's gate and the successor's current fence.
fn pending_requests(
    conn: &rusqlite::Connection,
    run: RunId,
    fence: &Fence,
) -> Result<Vec<Request>, RunError> {
    let mut stmt = conn.prepare("SELECT action FROM requests WHERE run_id=?1 AND state='pending' ORDER BY created_at,request_id")?;
    let rows = stmt
        .query_map([run.to_string()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter()
        .map(|value| {
            let request: Request = serde_json::from_str(value)?;
            if request.run != run || request.expected != *fence {
                return Err(RunError::broken("request mailbox binding"));
            }
            Ok(request)
        })
        .collect()
}

impl Run<'_> {
    pub fn requests(&self) -> Result<Vec<Request>, RunError> {
        journal::check_run_fence(self.store.conn(), self.id, &self.owner)?;
        pending_requests(self.store.conn(), self.id, &self.owner)
    }

    /// The current custodian alone can read a predecessor's already-fenced
    /// escalation while holding its owner gate. New controls still target the
    /// admitted successor's current fence; stale public controls stay refused.
    pub(super) fn recovery_escalated(
        &self,
        successor: RunId,
        generation: Generation,
    ) -> Result<bool, RunError> {
        if !self.holds_owner_gate()? {
            return Err(RunError::broken("recovery control needs the owner gate"));
        }
        let fence = Fence {
            resource: self.owner.resource.clone(),
            generation,
        };
        journal::check_fence(self.store.conn(), &fence)?;
        let current = journal::run(self.store.conn(), successor)?;
        let previous = journal::run(self.store.conn(), self.id)?;
        if current.owner != previous.owner
            || current.owner != self.owner.resource.name().to_string()
            || current.generation != generation
            || current.phase != Phase::Admitted
            || previous.generation != self.owner.generation
            || previous.generation >= generation
        {
            return Err(RunError::broken("recovery control binding"));
        }
        let mut requests = pending_requests(self.store.conn(), self.id, &self.owner)?;
        requests.extend(pending_requests(self.store.conn(), successor, &fence)?);
        Ok(requests
            .iter()
            .any(|request| matches!(request.action, RequestAction::Escalate)))
    }

    pub fn resolve_request(&mut self, request: &Request) -> Result<(), RunError> {
        self.require_environment_gate()?;
        if request.run != self.id || request.expected != self.owner {
            return Err(RunError::broken("foreign request"));
        }
        self.store.transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
            journal::check_run_fence(txn.conn(), request.run, &request.expected)?;
            txn.conn().execute("UPDATE requests SET state='resolved',resolved_at=?2 WHERE request_id=?1 AND run_id=?3",
                params![request.id.to_string(), txn.now(), self.id.to_string()])?;
            Ok::<_, RunError>(())
        })
    }
}

/// Read-only status keeps interruption separate from cleanup, with the engine's
/// parser-validated recovery command. No read repairs a run or acquires a gate.
#[derive(Debug)]
pub enum RunStatus {
    Live(Phase),
    Interrupted {
        phase: Phase,
        next: Invocation,
    },
    Closed {
        workload: WorkloadOutcome,
        complete: bool,
        effects: Vec<Effect>,
    },
}
pub fn status(
    cx: &crate::ctx::ObserveContext,
    root: &StateRoot,
    reader: &crate::store::StoreReader,
    id: RunId,
    next: Invocation,
) -> crate::observe::Observed<RunStatus> {
    use crate::observe::Observed;
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }

    let result: Result<_, RunError> = (|| {
        let record = journal::run(reader.conn(), id)?;
        if record.phase == Phase::Closed {
            let effects = journal::effects(reader.conn(), id)?;
            let cleanup: Option<String> = reader.conn().query_row(
                "SELECT cleanup_outcome FROM runs WHERE run_id=?1",
                [id.to_string()],
                |r| r.get(0),
            )?;
            let complete = cleanup.as_deref() == Some("complete")
                && effects.iter().all(|e| e.state.terminal());
            let workload = record
                .workload
                .ok_or_else(|| RunError::broken("closed without workload"))?;
            return Ok(Observed::Verified(
                RunStatus::Closed {
                    workload,
                    complete,
                    effects,
                },
                settle::evidence(),
            ));
        }
        Ok(match reader.run_owner_state(cx, root, &id) {
            Observed::Verified(crate::gate::OwnerState::Held, _) => {
                Observed::Verified(RunStatus::Live(record.phase), settle::evidence())
            }
            Observed::Verified(crate::gate::OwnerState::Interrupted, _) => Observed::Verified(
                RunStatus::Interrupted {
                    phase: record.phase,
                    next,
                },
                settle::evidence(),
            ),
            Observed::Unknown(reason) => Observed::Unknown(reason),
            _ => Observed::Unknown(UnknownReason::Inconsistent(
                "run state changed during observation".into(),
            )),
        })
    })();
    match result {
        Ok(value) => value,
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}

/// Read-only journal access for the fault-suite oracles (§12.1).
#[cfg(all(test, feature = "fault-injection"))]
pub(crate) fn journal_effects(
    conn: &rusqlite::Connection,
    id: RunId,
) -> Result<Vec<Effect>, RunError> {
    journal::effects(conn, id)
}

#[cfg(test)]
pub(crate) mod tests;

impl Run<'_> {
    /// Persistent setup output: intent precedes replace, and settle retains the
    /// exact result. The caller owns a Store gate and verifies adapter ownership.
    pub fn replace_file(
        &mut self,
        dir: &crate::fs::DirFd,
        name: &crate::fs::FileName,
        bytes: &[u8],
        opts: crate::record::ReplaceOpts,
    ) -> Result<(), crate::run::RunError> {
        let ticket = self.intend(crate::run::EffectSpec {
            target: self.fence().clone(), kind: "setup-file".into(),
            payload: serde_json::json!({"path": dir.path().join(name.as_str()), "sha256": crate::id::Digest::of(bytes)}),
            settlement: crate::run::Settlement::Retain, required: false,
        })?;
        crate::record::atomic_replace(&self.cx, &self.guard, dir, name, bytes, opts)?;
        self.applied(&ticket, serde_json::json!({"retained": true}))
    }
}

impl Run<'_> {
    /// Bootstrap adapters may prepare a root-owned helper before enabling
    /// custody. No process may spawn until this registration commits.
    pub fn enable_custody(
        &mut self,
        plan: &crate::supervise::LifecyclePlan,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        let lease = crate::supervise::spawn_sentinel(self, self.root, plan, &self.next)?;
        self.register_lifecycle(lease, plan)
    }
}

/// Pure journal projection for engine status; custody receipts contain no
/// argv, environment or raw output. No missing row proves process absence.
pub fn effects(
    reader: &crate::store::StoreReader,
    id: RunId,
) -> crate::observe::Observed<Vec<Effect>> {
    match journal::effects(reader.conn(), id) {
        Ok(effects) => crate::observe::Observed::Verified(effects, settle::evidence()),
        Err(error) => {
            crate::observe::Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string()))
        }
    }
}
