//! The environment's one workspace session: which run it is, what a front end
//! prints for it, and the down it requested (052 §8.7).
//!
//! A session consumes the kernel workspace seam's records rather than keeping a
//! second copy, and it never spawns a process, binds a socket, signals a group
//! or settles another run.
use super::*;

/// One environment's recorded session, before it is validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionRow {
    pub(crate) run: RunId,
    pub(crate) session_generation: Generation,
    pub(crate) environment_generation: Generation,
}

/// The session `environment` recorded, while that record still describes it.
/// Every field is checked against the journal: a record is evidence to
/// validate, never authority.
pub(crate) fn live_session_row(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
) -> Result<Option<SessionRow>, RunError> {
    let row = conn
        .query_row(
            "SELECT run_id, session_generation, environment_generation FROM workspace_sessions \
             WHERE environment=?1",
            [environment.name().to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((run, session_generation, environment_generation)) = row else {
        return Ok(None);
    };
    let row = SessionRow {
        run: run
            .parse()
            .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
        session_generation: Generation::new(session_generation as u64),
        environment_generation: Generation::new(environment_generation as u64),
    };
    // A newer environment generation does *not* revoke this record by itself.
    // The predecessor session stays authoritative until a successor activation
    // commits its mapping, `active(E)` and its attachment in one transaction
    // (owner ruling, 2026-10-29: never revoke, replace or retarget the
    // predecessor before the successor's atomic commit succeeds). The record is
    // therefore validated against its own run and its own attachment only, and
    // a committed successor mapping is what replaces the row.
    let live = conn
        .query_row(
            "SELECT generation, phase FROM runs WHERE run_id=?1",
            [row.run.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    match live {
        Some((generation, phase))
            if generation as u64 == row.session_generation.get() && phase != "closed" => {}
        _ => return Ok(None),
    }
    // The row's mode is decoded into the typed attachment mode. The encoding is
    // `AttachMode::stored`'s, never a literal written beside a query: a session
    // is the run that borrows, which only the typed mode can say.
    let borrowing = conn
        .query_row(
            "SELECT env_generation, mode FROM attachments WHERE environment=?1 \
             AND run_id=?2 AND state<>'released'",
            params![environment.name().to_string(), row.run.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((generation, mode)) = borrowing else {
        return Ok(None);
    };
    if generation as u64 != row.environment_generation.get() {
        return Ok(None);
    }
    if AttachMode::decode_stored(&mode)? != AttachMode::Borrow {
        return Ok(None);
    }
    Ok(Some(row))
}

/// Records `session` as `environment`'s workspace session. One environment has
/// one live session: an existing record that still validates refuses the write,
/// and a stale one is replaced.
pub(crate) fn record_session(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
) -> Result<(), RunError> {
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
    store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
        let current: Option<i64> = txn
            .conn()
            .query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [session.environment().name().to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let current = current.map(|value| Generation::new(value as u64));
        if current != Some(session.environment_generation()) {
            return Err(RunError::refused(
                "the environment generation moved before the session was recorded",
            ));
        }
        if let Some(existing) = live_session_row(txn.conn(), session.environment())? {
            if existing.run != session.run() {
                return Err(RunError::refused(
                    "this environment already has a live workspace session",
                ));
            }
            return Ok(());
        }
        txn.conn().execute(
            "INSERT INTO workspace_sessions \
             (environment, run_id, session_generation, environment_generation, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5) \
             ON CONFLICT(environment) DO UPDATE SET run_id=excluded.run_id, \
              session_generation=excluded.session_generation, \
              environment_generation=excluded.environment_generation, updated_at=excluded.updated_at",
            params![
                session.environment().name().to_string(),
                session.run().to_string(),
                session.generation().get() as i64,
                session.environment_generation().get() as i64,
                txn.now(),
            ],
        )?;
        Ok::<_, RunError>(())
    })
}

pub(crate) fn session_evidence() -> Evidence {
    Evidence::new("workspace session journal".into(), None).expect("fixed evidence source")
}

pub(crate) fn decode_owner(bytes: &[u8]) -> Result<ResourceId, RunError> {
    let id = ResourceId::decode_canonical(bytes)
        .map_err(|error| RunError::broken(format!("a stored session owner ({error})")))?;
    if id.canonical() != bytes {
        return Err(RunError::broken("a stored session owner is not canonical"));
    }
    Ok(id)
}

/// Which front end opened a terminal (§8.7). All three report one managed
/// session identity; they differ only in how they render it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    Tui,
    Text,
    Headless,
}
impl TerminalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Text => "text",
            Self::Headless => "headless",
        }
    }
}

/// How one client's ownership of its work ended (§8.11): EOF without an
/// acknowledged detach, the client going away, or a signal the terminal
/// received. Each is the same kernel release, and the cause the supervisor
/// stops the owned work with differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerLoss {
    /// End of input on the terminal, with no detach acknowledged.
    EndOfInput,
    /// The terminal or its client disappeared without closing cleanly.
    Disconnected,
    /// The terminal received this signal: TERM, HUP, or an operator's own.
    Signal(i32),
}
impl OwnerLoss {
    /// The release this loss records on the owner's attachment. An EOF, a
    /// disconnect and a signal are all the client going without a detach; an
    /// acknowledged detach is not a loss and uses `Release::Detach`.
    pub fn release(self) -> Release {
        Release::Eof
    }
    /// The cause the owned work is stopped with. A signal names itself; an EOF
    /// or a disconnect is owner loss.
    pub fn cause(self) -> StopCause {
        match self {
            Self::EndOfInput | Self::Disconnected => StopCause::OwnerLost,
            Self::Signal(signal) => StopCause::Signal(signal),
        }
    }
}

/// The durable identity of one environment's workspace session: the `Detached`
/// run, the owner generation that fences every request, and the environment
/// generation the session borrows.
///
/// Every front end — a `Tui`, a plain text client and a headless one — resolves
/// the same value, so all three report one managed session identity rather than
/// one per terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    run: RunId,
    fence: Fence,
    environment: ResourceId,
    environment_generation: Generation,
}

impl SessionRef {
    /// The supervisor's own identity for the session run it just admitted. A
    /// caller that has not admitted the run resolves it with
    /// [`SessionRef::resolve`] or [`SessionRef::of_run`] instead.
    pub fn new(
        run: RunId,
        fence: Fence,
        environment: ResourceId,
        environment_generation: Generation,
    ) -> Self {
        Self {
            run,
            fence,
            environment,
            environment_generation,
        }
    }
    pub fn run(&self) -> RunId {
        self.run
    }
    pub fn fence(&self) -> &Fence {
        &self.fence
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    pub fn environment_generation(&self) -> Generation {
        self.environment_generation
    }
    /// The session generation that keys every request id (§8.11): the session
    /// run's own owner generation, so it advances only when a new session is
    /// admitted, and a recovered request is never read against a successor.
    pub fn generation(&self) -> Generation {
        self.fence.generation
    }
    /// The kernel workspace seam's session identity, for reservation, attach
    /// and control.
    pub fn session(&self) -> run::Session {
        run::Session {
            run: self.run,
            generation: self.generation(),
        }
    }
    /// What every front end prints for this session. One session, one label.
    pub fn label(&self) -> String {
        format!(
            "workspace session {} of {} (generation {})",
            self.run,
            self.environment.label(),
            self.generation().get()
        )
    }

    /// The session of `environment`: the run its journal recorded as the
    /// workspace session, while that record still holds.
    ///
    /// A `Borrow` attachment alone does not name the session — a child run or a
    /// host `dev` hold can borrow the same environment — so the mapping is its
    /// own record, and this read validates it before use: the run is at the
    /// recorded generation and not closed, and it still borrows the environment
    /// at the recorded generation. A record that fails either check resolves as
    /// absent, so a settled session is never mistaken for a live one.
    ///
    /// A newer environment generation does *not* make the record absent. The
    /// predecessor session stays authoritative while a successor activation
    /// reserves, stages and publishes, and it is replaced only by that
    /// successor's atomic commit (owner ruling, 2026-10-29). Until then this
    /// resolves the predecessor, so a second terminal attaches to the session
    /// that is actually serving the workspace.
    pub fn resolve(reader: &StoreReader, environment: &ResourceId) -> Observed<Option<SessionRef>> {
        let result = (|| -> Result<Option<SessionRef>, RunError> {
            let Some(row) = live_session_row(reader.conn(), environment)? else {
                return Ok(None);
            };
            match Self::of_run(reader, row.run) {
                Observed::Verified(session, _) => Ok(session.filter(|session| {
                    session.generation() == row.session_generation
                        && session.environment_generation() == row.environment_generation
                })),
                Observed::Absent(_) => Ok(None),
                Observed::Unknown(reason) => Err(RunError::unavailable(reason.to_string())),
            }
        })();
        match result {
            Ok(Some(session)) => Observed::Verified(Some(session), session_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "workspace_sessions".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// The session identity recorded for `run`, whatever attached it. A retry
    /// that recovered an admission resolves its run this way, so the recovered
    /// answer names the same session the first attempt admitted.
    pub fn of_run(reader: &StoreReader, run: RunId) -> Observed<Option<SessionRef>> {
        let result = (|| -> Result<Option<SessionRef>, RunError> {
            let open: bool = reader.conn().query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE run_id=?1 AND phase NOT IN ('closed','stopping'))", [run.to_string()], |row| row.get(0))?;
            if !open {
                return Ok(None);
            }
            session_of_run(reader.conn(), run)
        })();
        match result {
            Ok(Some(session)) => Observed::Verified(Some(session), session_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "attachments".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Admits the environment's one workspace session run.
    ///
    /// The session is a `Detached` run, so a terminal can leave without ending
    /// it; a caller that plans any other placement is refused before anything
    /// is written. The run borrows the environment for its whole life, which is
    /// what a second entry finds and attaches to instead of duplicating, and the
    /// entry itself is guarded by the workspace entry's owner gate: request ids
    /// only exist inside a session and are reserved through the kernel workspace
    /// seam (`run::reserve`).
    #[allow(clippy::too_many_arguments)]
    pub fn admit<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        environment: &Fence,
        placement: Placement,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        session_placement(&opts, placement)?;
        let mut run = Run::begin(cx, root, invocation, opts, cleanup, adapter)?;
        Attachments::attach(&mut run, environment, AttachMode::Borrow, Wait::FailFast)?;
        let session = SessionRef::new(
            run.id(),
            run.fence().clone(),
            environment.resource.clone(),
            environment.generation,
        );
        if let Err(error) = record_session(cx, root, &session) {
            // A session the journal cannot name is not a session: settle it so
            // nothing live is left without its environment record.
            run.settle(cleanup, adapter)?;
            return Err(error);
        }
        Ok((run, session))
    }

    /// Admits the environment's one workspace session against a reserved
    /// successor generation E, at the identity the reservation froze.
    ///
    /// Admission takes only the frozen session's liveness slot, before Env
    /// admission, and never waits for a Task owner while holding Env. S is the
    /// frozen run fence; the Task counter advances only at atomic commit.
    ///
    /// Nothing is attached and no mapping is recorded here. The environment is
    /// not active at E yet, so the activation commit writes the mapping, the
    /// `Borrow` attachment and `active(E)` together. `E` is the reserved
    /// environment generation; the session's own generation S is
    /// `session.generation()`.
    ///
    /// The frozen claim admits under its private liveness slot while preserving
    /// the same logical Task owner. It neither takes the predecessor's gate nor
    /// advances its fence. Generic admission cannot use this overlap.
    #[allow(clippy::too_many_arguments)]
    pub fn admit_reserved<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        reservation: &WorkspaceActivationReservation,
        placement: Placement,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        Self::admit_reserved_inner(
            cx,
            root,
            invocation,
            opts,
            cleanup,
            adapter,
            reservation,
            placement,
            None,
        )
    }

    /// Admit only the session staged by this activation attempt.
    #[allow(clippy::too_many_arguments)]
    pub fn admit_reserved_for_attempt<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        reservation: &WorkspaceActivationReservation,
        placement: Placement,
        attempt: run::RequestId,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        Self::admit_reserved_inner(
            cx,
            root,
            invocation,
            opts,
            cleanup,
            adapter,
            reservation,
            placement,
            Some(attempt),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn admit_reserved_inner<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        reservation: &WorkspaceActivationReservation,
        placement: Placement,
        attempt: Option<run::RequestId>,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        session_placement(&opts, placement)?;
        let id = opts.id;
        let claim = reservation.claim_session_with_attempt(cx, root, attempt)?;
        if claim.binding.owner != invocation.spec().owner {
            return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
        }
        if claim.binding.run != id {
            return Err(RunError::refused(
                "the admitted run is not the successor session the reservation froze",
            ));
        }
        let session = SessionRef::new(
            id,
            Fence {
                resource: claim.binding.owner.clone(),
                generation: claim.binding.generation,
            },
            reservation.environment().clone(),
            reservation.generation(),
        );
        let run = Run::begin_claimed(
            cx,
            root,
            invocation,
            opts,
            || Ok(cleanup.clone()),
            adapter,
            ActivationClaim::Session(claim),
        )?;
        if run.fence() != &session.fence {
            run.settle(cleanup, adapter)?;
            return Err(RunError::broken(
                "the admitted successor session is not the frozen staging",
            ));
        }
        Ok((run, session))
    }
}

/// The independently retained supervisor side of a detached session handoff.
/// Adopting consumes the owning Run: an invoking client retains only SessionRef.
/// The supervisor must retain this value until settlement; dropping it releases
/// the private slot and all registered listener/key custody.
#[derive(Debug)]
pub struct SessionSupervisor<'a> {
    run: Run<'a>,
}

impl<'a> SessionSupervisor<'a> {
    pub fn adopt(run: Run<'a>, supervisor: &str) -> Result<Self, RunError> {
        Self::adopt_recoverable(run, supervisor).map_err(|(error, _)| error)
    }

    /// Retain custody on a registration failure so the caller can settle it.
    pub fn adopt_recoverable(
        mut run: Run<'a>,
        supervisor: &str,
    ) -> Result<Self, (RunError, Box<Run<'a>>)> {
        let result = (|| {
            crate::fault_point!(SessionSupervisorAdoptBefore)?;
            run.register_session_custody(supervisor)?;
            crate::fault_point!(SessionSupervisorAdoptAfter)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(Self { run }),
            Err(error) => Err((error, Box::new(run))),
        }
    }

    pub fn settle(
        self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<crate::outcome::Outcome<crate::run::Settled>, RunError> {
        self.run.settle(cleanup, adapter)
    }

    /// Keep the session's private slot when cleanup cannot finish.
    pub fn settle_recoverable(
        self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<crate::outcome::Outcome<crate::run::Settled>, (RunError, Box<Self>)> {
        match self.run.settle_workspace_recoverable(cleanup, adapter) {
            Ok(outcome) => Ok(outcome),
            Err((error, run)) => Err((error, Box::new(Self { run: *run }))),
        }
    }
}

impl<'a> std::ops::Deref for SessionSupervisor<'a> {
    type Target = Run<'a>;
    fn deref(&self) -> &Self::Target {
        &self.run
    }
}
impl std::ops::DerefMut for SessionSupervisor<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.run
    }
}

/// The placement a workspace session may be admitted with. A `Detached` session
/// is its own supervisor, so it plans that lifecycle rather than promising one
/// and running another.
pub(crate) fn session_placement(opts: &BeginOpts, placement: Placement) -> Result<(), RunError> {
    if placement != Placement::Detached {
        return Err(RunError::refused(
            "a workspace session is a Detached run, not an owning terminal",
        ));
    }
    if let Some(plan) = &opts.custody {
        if plan.placement != placement {
            return Err(RunError::broken(
                "a session custody plan and its placement disagree",
            ));
        }
    }
    Ok(())
}

/// A handle on one resolved session: durable identity, the kernel seam's
/// session value, and the down coordination a session requested from inside.
/// It carries no owning pipe, so a front end can hold it without owning the
/// session's lifetime.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    session: SessionRef,
}

impl SessionHandle {
    pub fn new(session: SessionRef) -> Self {
        Self { session }
    }
    pub fn identity(&self) -> &SessionRef {
        &self.session
    }
    /// The kernel workspace seam's identity for reservation, attach and
    /// control (`run::reserve`, `run::attach_secondary`, `Attachment::control`).
    pub fn session(&self) -> run::Session {
        self.session.session()
    }

    /// The down coordination for this session (§8.7): a foreign live or
    /// `Unknown` holder refuses before anything stops.
    pub fn down(
        &self,
        cx: &ObserveContext,
        root: &StateRoot,
        reader: &StoreReader,
    ) -> Observed<DownStart> {
        DownCoordination::begin(cx, root, reader, &self.session)
    }
}

/// Sends one journalled stop request to `target` and rings its doorbell. Shared
/// by a down's child stops and the session's own settle request; the request is
/// the kernel's own control path, not a second one.
pub(crate) fn request_stop(
    cx: &EffectContext,
    root: &StateRoot,
    reader: &StoreReader,
    request: &run::RequestId,
    target: RunId,
) -> Result<RunId, RunError> {
    let fence = target_fence(reader, target)?;
    let stop = request_run_id(request)?;
    // A committed request is the durable fact: the same id naming the same stop
    // is that request, not a duplicate one. The journal refuses a second row, so
    // a retried stop answers instead of erroring.
    match recorded_request(reader, stop)? {
        Some(recorded) if recorded.run == target && recorded.expected == fence => {
            if !matches!(recorded.action, run::RequestAction::Stop) {
                return Err(RunError::refused(
                    "this request id already names another action",
                ));
            }
        }
        Some(_) => {
            return Err(RunError::refused(
                "this request id already names another run",
            ))
        }
        None => run::request(
            cx,
            root,
            &run::Request {
                id: stop,
                run: target,
                expected: fence.clone(),
                action: run::RequestAction::Stop,
            },
        )?,
    }
    if let Ok(observe) = cx
        .operation()
        .child(std::time::Duration::from_secs(1))
        .observe()
    {
        let _ = run::connect_control_socket(&observe, root, &fence.resource, target);
    }
    Ok(target)
}

/// A kernel request identity as the `RunId` a journalled control request names.
/// The bits are the request id's own; nothing about them grants authority.
pub(crate) fn request_run_id(request: &run::RequestId) -> Result<RunId, RunError> {
    let bytes = request.bytes();
    let mut millis = [0u8; 8];
    millis[2..].copy_from_slice(&bytes[..6]);
    let mut random = [0u8; 10];
    random.copy_from_slice(&bytes[6..]);
    RunId::new(u64::from_be_bytes(millis), random)
        .map_err(|error| RunError::broken(format!("a control request id ({error})")))
}

/// The fence a run row was admitted under, read without acquiring its gate.
pub(crate) fn target_fence(reader: &StoreReader, target: RunId) -> Result<Fence, RunError> {
    let (generation, canonical) = reader
        .conn()
        .query_row(
            "SELECT r.generation, res.canonical FROM runs r \
             JOIN resources res ON res.name=r.owner WHERE r.run_id=?1",
            [target.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?
        .ok_or_else(|| RunError::refused("the requested run is not in this journal"))?;
    Ok(Fence {
        resource: decode_owner(&canonical)?,
        generation: Generation::new(generation as u64),
    })
}

/// A journalled control request by id, whatever its state. The row is the
/// durable record `run::request` wrote, so a retry can recognise its own.
pub(crate) fn recorded_request(reader: &StoreReader, id: RunId) -> Result<Option<run::Request>, RunError> {
    let action = reader
        .conn()
        .query_row(
            "SELECT action FROM requests WHERE request_id=?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    action
        .map(|action| serde_json::from_str(&action).map_err(RunError::from))
        .transpose()
}

/// The phase a run is in, for a caller that names a session in a diagnostic.
pub fn phase(reader: &StoreReader, run: RunId) -> Observed<Phase> {
    match run::summary(reader, run) {
        Observed::Verified(summary, evidence) => Observed::Verified(summary.phase, evidence),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

// ---------------------------------------------------------------------------
// Workspace activation reservation (§8.7, §8.11)
// ---------------------------------------------------------------------------
//
// The one ordering rule this section exists for: the workspace session is a
// Task-owned run, and `Gate(Task)` must be taken before `Gate(Env)`. The
// session's environment label is the *successor* generation E, which does not
// exist until the environment activation run is admitted — and admitting it
// means holding `Gate(Env)`. Reserving E durably breaks that circle: the
// reservation is taken and relinquished under `Gate(Env)`, the session is
// admitted and staged while Env is free, and the activation run claims E and
// later commits the mapping, `active(E)` and the session's `Borrow` attachment
// atomically.

/// The recorded workspace session a run belongs to, if any: the mapping names
/// it, or a staging froze it. This is a read of the journal's own records.
pub(crate) fn session_of_run(
    conn: &rusqlite::Connection,
    run: RunId,
) -> Result<Option<SessionRef>, RunError> {
    let mapped: Option<(i64, i64, String, String)> = conn
        .query_row(
            "SELECT s.session_generation, s.environment_generation, s.environment, r.owner \
             FROM workspace_sessions s LEFT JOIN runs r ON r.run_id=s.run_id WHERE s.run_id=?1",
            [run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((session, environment_generation, environment, owner)) = mapped {
        let owner = owner_identity(conn, &owner)?;
        let environment = owner_identity(conn, &environment)?;
        return Ok(Some(SessionRef::new(
            run,
            Fence {
                resource: owner,
                generation: Generation::new(session as u64),
            },
            environment,
            Generation::new(environment_generation as u64),
        )));
    }
    let staged: Option<(i64, String)> = conn
        .query_row(
            "SELECT generation, owner FROM workspace_session_runs WHERE run_id=?1",
            [run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((generation, owner)) = staged else {
        return Ok(None);
    };
    let owner = owner_identity(conn, &owner)?;
    let reservation: Option<(String, i64)> = conn
        .query_row(
            "SELECT environment, environment_generation FROM workspace_activation_reservations \
             WHERE session_run=?1",
            [run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((environment, environment_generation)) = reservation else {
        return Ok(None);
    };
    let environment = owner_identity(conn, &environment)?;
    Ok(Some(SessionRef::new(
        run,
        Fence {
            resource: owner,
            generation: Generation::new(generation as u64),
        },
        environment,
        Generation::new(environment_generation as u64),
    )))
}

/// The generation `owner`'s counter would advance to next, read *without* its
/// gate: a staging is a prediction that the successor's own admission confirms
/// under `Gate(owner)`, so a staging grants nothing the counter does not.
pub(crate) fn next_generation(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
) -> Result<Generation, RunError> {
    let current: Option<i64> = conn
        .query_row(
            "SELECT generation FROM resources WHERE name=?1",
            [owner.name().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    Generation::new(current.unwrap_or(0) as u64)
        .next()
        .ok_or_else(|| RunError::refused("the workspace owner generation cannot advance"))
}

/// Cancellation releases no admitted run before its complete settlement.
/// A staged session may not have admitted yet; a bound Env run necessarily did.
pub(crate) fn run_needs_settlement(
    conn: &rusqlite::Connection,
    run: RunId,
    admitted: bool,
) -> Result<bool, RunError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT phase,cleanup_outcome FROM runs WHERE run_id=?1",
            [run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match row {
        Some((phase, cleanup)) => Ok(phase != "closed" || cleanup.as_deref() != Some("complete")),
        None if admitted => Err(RunError::broken(
            "a bound activation run is missing its settlement record",
        )),
        None => Ok(false),
    }
}

/// The published session's applied fence remains usable across successor and
/// ordinary no-op transitions. This grants only borrowing, never transition
/// authority, and stops matching at the atomic mapping commit.
pub(crate) fn preserved_environment(
    conn: &rusqlite::Connection,
    fence: &Fence,
) -> Result<bool, RunError> {
    let Some(session) = live_session_row(conn, &fence.resource)? else {
        return Ok(false);
    };
    if session.environment_generation != fence.generation {
        return Ok(false);
    }
    preserved_session(conn, &fence.resource, &session)
}

pub(crate) fn preserved_session(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    session: &SessionRow,
) -> Result<bool, RunError> {
    Ok(conn
        .query_row(
            "SELECT r.state IN ('active','pending') AND r.last_applied=?2 \
             AND EXISTS(SELECT 1 FROM runs WHERE run_id=?3 AND phase IN ('admitted','starting','active')) \
             FROM resources r WHERE r.name=?1",
            params![
                environment.name().to_string(),
                session.environment_generation.get().to_string(),
                session.run.to_string()
            ],
            |row| row.get::<_, Option<bool>>(0),
        )?
        .unwrap_or(false))
}

/// A no-op publication must not advance the applied fence away from the live
/// mapped session. The caller holds Gate(Env) and checked its transition fence;
/// only the atomic successor commit may publish a different workspace fence.
pub(crate) fn preserve_active_predecessor(
    txn: &mut Txn<'_>,
    owner: &Fence,
) -> Result<Option<Fence>, RunError> {
    let Some(session) = live_session_row(txn.conn(), &owner.resource)? else {
        return Ok(None);
    };
    let applied = Fence {
        resource: owner.resource.clone(),
        generation: session.environment_generation,
    };
    if !preserved_session(txn.conn(), &owner.resource, &session)? {
        return Ok(None);
    }
    txn.conn().execute(
        "UPDATE resources SET state='active',updated_at=?2 WHERE name=?1",
        params![owner.resource.name().to_string(), txn.now()],
    )?;
    Ok(Some(applied))
}

/// A reserved session uses a private liveness slot, not another Task owner.
/// The durable binding is the authority; knowing or locking this slot grants
/// nothing without the exact run/owner/S claim. It remains stable after cancel,
/// commit and later mapping replacement, until the run settles.
pub(crate) fn session_run_gate(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
    run: RunId,
) -> Result<ResourceId, RunError> {
    // Legacy schema-1 sessions have only the shared Task owner gate. Read-only
    // reconnect must not require a writer to install the additive slot table.
    if !crate::store::has_table(conn, "workspace_session_runs")? {
        return Ok(owner.clone());
    }
    let gate: Option<Option<String>> = conn
        .query_row(
            "SELECT gate_key FROM workspace_session_runs WHERE run_id=?1 AND owner=?2",
            params![run.to_string(), owner.name().to_string()],
            |row| row.get(0),
        )
        .optional()?;
    match gate.flatten() {
        Some(name) => {
            let slot = session_slot(owner, run)?;
            if slot.name().to_string() != name {
                return Err(RunError::broken("workspace session liveness slot identity"));
            }
            Ok(slot)
        }
        None => Ok(owner.clone()),
    }
}

pub(crate) fn session_slot(owner: &ResourceId, run: RunId) -> Result<ResourceId, RunError> {
    let crate::id::Scope::Checkout(checkout) = owner.scope() else {
        return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
    };
    // The slot is in Task's rank and checkout scope, so retaining it while Env
    // is admitted preserves canonical Task-before-Env order. It is not a new
    // journal owner: runs.owner always remains the frozen workspace owner.
    crate::id::TaskId::new(
        checkout,
        "",
        crate::id::Surface::Published,
        &format!("__effigy_workspace_session__:{}:{run}", owner.name()),
        None,
    )
    .map(|task| task.resource())
    .map_err(|error| RunError::broken(format!("workspace session slot: {error}")))
}

/// Ordinary Task admission never recovers or replaces a private-gated session.
/// Explicit recovery takes its exact liveness slot. An unadmitted staging also
/// excludes competing ordinary admissions, with a second check at their commit.
pub(crate) fn refuse_session_overlap(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
) -> Result<(), RunError> {
    let mut query = conn.prepare(
        "SELECT DISTINCT res.canonical FROM workspace_session_runs s \
         JOIN resources res ON res.name=s.owner \
         LEFT JOIN workspace_activation_reservations a ON a.reservation_id=s.reservation_id \
         LEFT JOIN runs r ON r.run_id=s.run_id \
         WHERE s.gate_key IS NOT NULL AND \
         (a.state NOT IN ('committed','cancelled') OR r.phase<>'closed' OR r.cleanup_outcome IS NULL AND r.run_id IS NOT NULL \
          OR r.cleanup_outcome='incomplete')",
    )?;
    for canonical in query.query_map([], |row| row.get::<_, Vec<u8>>(0))? {
        let held = decode_owner(&canonical?)?;
        if held == *owner || held.checkout_alias_of(owner) {
            return Err(RunError::from(ActivationRefusal::SessionAlreadyStaged));
        }
    }
    Ok(())
}

