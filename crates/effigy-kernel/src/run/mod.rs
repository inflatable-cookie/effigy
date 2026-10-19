//! Durable intent, generation fencing and one recoverable settle (§5.6).
//!
//! Engines supply effect observations and idempotent undo. A `Run` is the only
//! writer of its journal; dropping it releases liveness, never claims cleanup.
mod attachments;
mod claims;
mod history;
mod journal;
mod lifecycle;
mod logs;
pub use lifecycle::{metadata, RunMetadata, SentinelStatus};
#[path = "../objects/run.rs"]
mod objects;
mod settle;
mod socket;
pub use socket::control_socket_path;

pub use attachments::{
    AttachMode, AttachmentTicket, Attachments, Holder, Liveness, TransitionPermit,
};
pub use claims::{Claims, ReservationToken};
pub use history::{effects, history, recent, summary, unclosed, RunSummary};
pub use logs::{read_log, LogRead};
pub(crate) use settle::settle_interrupted_wait;
pub use settle::{settle_interrupted, Reconciled, Settled, SettlementAdapter};

use crate::ctx::{CleanupContext, EffectContext, Interrupt};
use crate::gate::{Gate, GateError, GateGuard, Wait};
use crate::id::{Generation, ResolvedInvocation, ResourceId, RunId};
use crate::observe::UnknownReason;
use crate::outcome::{Invocation, Outcome, WorkloadOutcome};
use crate::root::StateRoot;
use crate::store::{Store, StoreError};
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
            _ => Err(RunError::Store(StoreError::Corrupt(
                "invalid run phase".into(),
            ))),
        }
    }
}
#[derive(Debug)]
pub enum RunError {
    Store(StoreError),
    Gate(GateError),
    Interrupted(Interrupt),
    HostAuthorityRequired,
    Invalid(String),
    StaleGeneration {
        expected: Fence,
        current: Option<Generation>,
    },
    PredecessorUnsettled {
        run: RunId,
        effects: Vec<Effect>,
    },
    HeldBy(Vec<Holder>),
    EnvNotActive,
    Conflict(ResourceId),
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "run refused: {self:?}")
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
        Self::Store(StoreError::Corrupt(e.to_string()))
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
#[derive(Debug)]
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
    processes: std::collections::BTreeMap<
        i64,
        std::sync::Arc<std::sync::Mutex<crate::custody::ProcessCore>>,
    >,
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
        crate::fault_point!(BeginBefore)?;
        if cx.operation().placement() != crate::ctx::Placement::Host
            || inv.spec().placement != crate::ctx::Placement::Host
        {
            return Err(RunError::HostAuthorityRequired);
        }
        if cx.operation().class() != inv.spec().class {
            return Err(RunError::Invalid(
                "invocation class differs from context".into(),
            ));
        }
        cx.operation().check("begin")?;
        let mut ids = inv.spec().gates.clone();
        ids.push(inv.spec().owner.clone());
        let mut guard = Gate::acquire(cx, root, &ids, opts.wait)?;
        crate::fault_point!(BeginGated)?;
        // Only an owner gate may span recovery callbacks or later effects (K14).
        guard.retain_owner(&inv.spec().owner);
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let predecessors = journal::predecessors(store.conn(), &inv.spec().owner)?;
        for predecessor in predecessors {
            let mut previous = Self {
                root,
                store,
                guard,
                cx: cx.clone(),
                id: predecessor.id,
                owner: Fence {
                    resource: inv.spec().owner.clone(),
                    generation: predecessor.generation,
                },
                next: opts.next.clone(),
                poisoned: false,
                completed: Default::default(),
                sentinel: None,
                processes: Default::default(),
            };
            previous.settle_inner(cleanup, adapter)?;
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
                previous
                    .store
                    .transaction(&settle::cleanup_effects(cleanup)?, |txn| {
                        txn.conn().execute(
                            "UPDATE runs SET residue_ack=?2 WHERE run_id=?1",
                            params![predecessor.id.to_string(), txn.now()],
                        )?;
                        Ok::<_, RunError>(())
                    })?;
            }
            store = previous.store;
            guard = previous.guard;
        }
        crate::fault_point!(BeginRecovered)?;
        let generation = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            txn.resource(&inv.spec().owner, "run")?;
            let generation = txn.next_generation(&inv.spec().owner)?;
            txn.conn().execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,invocation,digest,created_at,updated_at,workload_outcome) \
                VALUES (?1,?2,?3,?4,'host','admitted',?5,?6,?7,?7,?8)", params![opts.id.to_string(),
                inv.spec().owner.name().to_string(), generation.get() as i64,
                if opts.custody.is_some() {"custody"}else{"operation"}, serde_json::to_string(inv)?, inv.digest().0.as_slice(), txn.now(), if opts.custody.is_some() {Some(serde_json::to_string(&WorkloadOutcome::NotStarted)?)}else{None}])?;
            Ok::<_, RunError>(generation)
        })?;
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
            processes: Default::default(),
        };
        if let Some(plan) = opts.custody {
            let registration = crate::supervise::spawn_sentinel(&run, root, &plan, &opts.next);
            match registration.and_then(|registration| run.register_lifecycle(registration, &plan))
            {
                Ok(()) => {}
                Err(error) => {
                    run.settle_inner(cleanup, adapter)?;
                    return Err(error);
                }
            }
        }
        Ok(run)
    }
    pub(crate) fn capture_log(
        &self,
        sequence: i64,
        stream: crate::custody::Stream,
        limits: &crate::ctx::Limits,
    ) -> std::io::Result<crate::custody::capture::Log> {
        crate::custody::capture::Log::open(self.root, self.id, sequence, stream, limits)
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
    pub fn fence(&self) -> &Fence {
        &self.owner
    }
    pub fn phase(&self) -> Result<Phase, RunError> {
        Ok(journal::run(self.store.conn(), self.id)?.phase)
    }
    pub fn effects(&self) -> Result<Vec<Effect>, RunError> {
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
        // Environment mutation must go through TransitionPermit::intend.
        if spec.target.resource.kind() == crate::id::ResourceKind::Env {
            return Err(RunError::Invalid(
                "environment effect needs a transition permit".into(),
            ));
        }
        if spec.kind.starts_with("kernel:") {
            return Err(RunError::Invalid("reserved effect kind".into()));
        }
        self.intend_inner(spec)
    }
    fn intend_inner(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.can_effect()?;
        let result = (|| {
            crate::fault_point!(IntendBefore)?;
            let sequence = self.store.transaction(
                &self.cx.child(std::time::Duration::from_secs(30)),
                |txn| {
                    journal::check_fence(txn.conn(), &self.owner)?;
                    journal::check_fence(txn.conn(), &spec.target)?;
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
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
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
            Some(format!("{reason:?}")),
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
            return Err(RunError::Invalid("foreign effect ticket".into()));
        }
        let result = (|| {
            if state == EffectState::Failed {
                crate::fault_point!(FailedBefore)?;
            } else {
                crate::fault_point!(AppliedBefore)?;
            }
            self.store
                .transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_fence(txn.conn(), &self.owner)?;
                    let effect = journal::effect(txn.conn(), self.id, ticket.sequence)?;
                    journal::check_fence(txn.conn(), &effect.spec.target)?;
                    if !matches!(effect.state, EffectState::Intended | EffectState::Unknown) {
                        return Err(RunError::Invalid("effect already completed".into()));
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
    fn can_effect(&self) -> Result<(), RunError> {
        self.cx.operation().check("effect")?;
        if self.poisoned || matches!(self.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::Invalid(
                "run must settle before more effects".into(),
            ));
        }
        Ok(())
    }
    pub fn advance(&mut self, phase: Phase) -> Result<(), RunError> {
        self.can_effect()?;
        let previous = self.phase()?;
        if !matches!(
            (previous, phase),
            (Phase::Starting, Phase::Active) | (_, Phase::Stopping)
        ) {
            return Err(RunError::Invalid("invalid phase transition".into()));
        }
        if phase == Phase::Active
            && self
                .effects()?
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(RunError::Invalid("startup obligations unresolved".into()));
        }
        let result = (|| {
            crate::fault_point!(PhaseBefore)?;
            self.store
                .transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_fence(txn.conn(), &self.owner)?;
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
            return Err(RunError::Invalid("foreign completion ticket".into()));
        }
        journal::check_fence(self.store.conn(), &self.owner)?;
        let effect = journal::effect(self.store.conn(), self.id, ticket.sequence)?;
        journal::check_fence(self.store.conn(), &effect.spec.target)?;
        if effect.state != EffectState::Applied || self.completed.contains_key(&ticket.sequence) {
            return Err(RunError::Invalid(
                "effect is not awaiting completion".into(),
            ));
        }
        self.completed.insert(ticket.sequence, obligation);
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
                if e.state == EffectState::Applied
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
        .map_err(|_| RunError::Invalid("workload obligations incomplete".into()))?;
        let result = (|| {
            self.store.transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
            journal::check_fence(txn.conn(), &self.owner)?;
            crate::fault_point!(PhaseBefore)?;
            txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",
                params![self.id.to_string(), serde_json::to_string(&workload)?, txn.now()])?;
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

    /// Stop outcomes use the independent cleanup budget, never a cancelled
    /// workload token. Natural success still requires finish's obligations.
    pub fn interrupt(
        &mut self,
        cleanup: &CleanupContext,
        workload: WorkloadOutcome,
    ) -> Result<(), RunError> {
        if !matches!(
            workload,
            WorkloadOutcome::Cancelled { .. }
                | WorkloadOutcome::TimedOut { .. }
                | WorkloadOutcome::NotStarted
        ) || self.phase()? == Phase::Closed
        {
            return Err(RunError::Invalid("invalid interruption outcome".into()));
        }
        let cx = settle::cleanup_effects(cleanup)?;
        self.store.transaction(&cx,|txn| {
            journal::check_fence(txn.conn(),&self.owner)?;
            txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",params![self.id.to_string(),serde_json::to_string(&workload)?,txn.now()])?;
            Ok::<_,RunError>(())
        })
    }
    pub fn settle(
        mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Outcome<Settled>, RunError> {
        self.settle_inner(cleanup, adapter)
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
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::Invalid("request needs Mutate".into()))?;
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| RunError::HostAuthorityRequired)?;
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
    crate::fault_point!(RequestBefore)?;
    store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
        journal::check_fence(txn.conn(), &request.expected)?;
        let record = journal::run(txn.conn(), request.run)?;
        if record.owner != request.expected.resource.name().to_string() || record.generation != request.expected.generation || record.phase == Phase::Closed {
            return Err(RunError::Invalid("request run is not current".into()));
        }
        txn.conn().execute("INSERT INTO requests(request_id,run_id,expected_generation,action,state,created_at) VALUES (?1,?2,?3,?4,'pending',?5)",
            params![request.id.to_string(), request.run.to_string(), request.expected.generation.get() as i64,
                serde_json::to_string(request)?, txn.now()])?;
        Ok::<_, RunError>(())
    })?;
    crate::fault_point!(RequestAfter)?;
    Ok(())
}
impl Run<'_> {
    pub fn requests(&self) -> Result<Vec<Request>, RunError> {
        journal::check_fence(self.store.conn(), &self.owner)?;
        let mut stmt = self.store.conn().prepare("SELECT action FROM requests WHERE run_id=?1 AND state='pending' ORDER BY created_at,request_id")?;
        let rows = stmt
            .query_map([self.id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|value| Ok(serde_json::from_str(value)?))
            .collect()
    }
    pub fn resolve_request(&mut self, request: &Request) -> Result<(), RunError> {
        if request.run != self.id || request.expected != self.owner {
            return Err(RunError::Invalid("foreign request".into()));
        }
        self.store.transaction(&self.cx.child(std::time::Duration::from_secs(30)), |txn| {
            journal::check_fence(txn.conn(), &request.expected)?;
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
                .ok_or_else(|| RunError::Invalid("closed without workload".into()))?;
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
