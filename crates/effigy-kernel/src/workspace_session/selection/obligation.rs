//! The durable selection obligation one activation commits (052 §8.11).
//!
//! The obligation is written by the same transaction that publishes `active(E)`,
//! the session mapping and the session's `Borrow` attachment, so there is no
//! window in which a generation is advertised and no record of what it
//! selected. Recovery, pending and abandoned reads all answer from this one row.
use super::*;
/// The obligation's own lifecycle. A state this kernel does not know is
/// refused, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionState {
    /// Committed with the generation, not yet switched.
    Pending,
    /// Every selector holds the committed target and the objects were read back.
    Switched,
    /// Completion became impossible; the publication is revoked.
    Abandoned,
}

impl SelectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Switched => "switched",
            Self::Abandoned => "abandoned",
        }
    }
    fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "switched" => Some(Self::Switched),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

/// The durable record of one generation's selection, written by the commit that
/// published it. Its id is derived from the activation it belongs to, so a retry
/// reads the same obligation instead of creating a second one.
#[derive(Debug, Clone)]
pub struct SelectionObligation {
    id: run::RequestId,
    reservation: ActivationReservationId,
    environment: ResourceId,
    environment_generation: Generation,
    run: RunId,
    session_run: RunId,
    receipt: SelectionReceipt,
    state: SelectionState,
}

impl SelectionObligation {
    fn new(
        reservation: &ActivationPlan,
        run: RunId,
        receipt: SelectionReceipt,
    ) -> Result<Self, RunError> {
        let mut material = reservation.reservation_id().bytes().to_vec();
        material.extend_from_slice(run.to_string().as_bytes());
        material.extend_from_slice(reservation.session_run().to_string().as_bytes());
        material.extend_from_slice(b"effigy.workspace-selection-obligation.v1");
        let digest = Digest::of(&material);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.0[..16]);
        Ok(Self {
            id: run::RequestId::new(bytes),
            reservation: reservation.reservation_id(),
            environment: reservation.environment().clone(),
            environment_generation: reservation.environment_generation(),
            run,
            session_run: reservation.session_run(),
            receipt,
            state: SelectionState::Pending,
        })
    }

    pub fn id(&self) -> run::RequestId {
        self.id
    }
    pub fn reservation(&self) -> ActivationReservationId {
        self.reservation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    pub fn environment_generation(&self) -> Generation {
        self.environment_generation
    }
    /// The environment activation run whose commit wrote this obligation.
    pub fn run(&self) -> RunId {
        self.run
    }
    pub fn session_run(&self) -> RunId {
        self.session_run
    }
    pub fn receipt(&self) -> &SelectionReceipt {
        &self.receipt
    }
    pub fn state(&self) -> SelectionState {
        self.state
    }
    /// The fence a switch must hold: the obligation's own environment at E.
    pub(crate) fn fence(&self) -> Fence {
        Fence {
            resource: self.environment.clone(),
            generation: self.environment_generation,
        }
    }

    /// Read the exact target back and refuse anything that is not the committed
    /// selection: every selector at its committed next value and every object
    /// hashing to its committed digest. A partial switch, a third value, a
    /// missing object and an unreadable one are each refused, so completion is
    /// always the result of an observation, never of the obligation's own
    /// record.
    pub fn observe(
        &self,
        reader: &mut dyn SelectorReader,
    ) -> Result<SelectionSwitchReceipt, RunError> {
        self.receipt.require_reader_target(reader)?;
        let expected = SelectionSwitchReceipt::expected(&self.receipt);
        let selection = &self.receipt.selection;
        let mut selectors = Vec::with_capacity(3);
        for (change, observed) in selection.switches.iter().zip(&expected.selectors) {
            match reader.read_selector(&change.path) {
                Observed::Verified(value, _) if value == observed.value => {
                    selectors.push(SelectorReadback {
                        path: change.path.clone(),
                        value,
                    });
                }
                Observed::Verified(..) | Observed::Absent(_) => {
                    return Err(ActivationRefusal::SelectionMismatch.into())
                }
                Observed::Unknown(reason) => {
                    return Err(RunError::unavailable(format!(
                        "workspace selector `{}` is unreadable: {reason}",
                        change.path
                    )))
                }
            }
        }
        for object in [
            &selection.objects.binary,
            &selection.objects.context,
            &selection.objects.credential,
        ] {
            match reader.read_object(&object.path) {
                Observed::Verified(digest, _) if digest == object.digest => {}
                Observed::Verified(..) | Observed::Absent(_) => {
                    return Err(ActivationRefusal::SelectionMismatch.into())
                }
                Observed::Unknown(reason) => {
                    return Err(RunError::unavailable(format!(
                        "workspace object `{}` is unreadable: {reason}",
                        object.path
                    )))
                }
            }
        }
        self.receipt.require_reader_target(reader)?;
        let mut receipt = expected;
        receipt.selectors = [
            selectors[0].clone(),
            selectors[1].clone(),
            selectors[2].clone(),
        ];
        Ok(receipt)
    }

    /// Refuse a switch receipt that is not the obligation's exact target. This
    /// is redundant with [`Self::observe`] on purpose: completion re-checks the
    /// readback it will record against the committed target.
    pub fn validate_switch(&self, receipt: &SelectionSwitchReceipt) -> Result<(), RunError> {
        if SelectionSwitchReceipt::expected(&self.receipt) != *receipt {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        Ok(())
    }
}

/// The three independent liveness facts recovery judges before it completes or
/// abandons a selection. Each is `Verified` only with positive evidence; an
/// unknown answer is neither live nor dead.
#[derive(Debug, Clone)]
pub struct SelectionLiveness {
    /// The committed session run is live and still the environment's mapping.
    pub session: Observed<()>,
    /// The target the publication names still verifies as that exact container
    /// and boot.
    pub target: Observed<()>,
    /// The listener that holds the session's key answers a bounded pinned
    /// handshake. No application frame, credential or admission record.
    pub listener: Observed<()>,
}

impl SelectionLiveness {
    /// All three facts proven together. Only then may a selection complete.
    pub fn complete(&self) -> bool {
        [&self.session, &self.target, &self.listener]
            .into_iter()
            .all(|fact| matches!(fact, Observed::Verified(..)))
    }
    /// A fact positively disproves the selection: nothing can complete it.
    pub fn disproven(&self) -> bool {
        let facts = [&self.session, &self.target, &self.listener];
        !facts
            .iter()
            .any(|fact| matches!(fact, Observed::Unknown(_)))
            && facts.iter().any(|fact| matches!(fact, Observed::Absent(_)))
    }
}

pub(crate) const OBLIGATIONS: &str = "workspace_selection_obligations";

pub(crate) type ObligationRow = (
    String,
    i64,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
);

pub(crate) fn decode_obligation(
    environment: &ResourceId,
    row: ObligationRow,
) -> Result<SelectionObligation, RunError> {
    let (reservation, environment_generation, run, session_run, state, receipt, _created, _reason) =
        row;
    let Some(state) = SelectionState::parse(&state) else {
        return Err(RunError::broken(format!(
            "a stored workspace selection state (`{state}`)"
        )));
    };
    let receipt: SelectionReceipt = serde_json::from_str(&receipt).map_err(|error| {
        RunError::broken(format!("a stored workspace selection receipt ({error})"))
    })?;
    Ok(SelectionObligation {
        id: derived_id(&reservation, &run, &session_run)?,
        reservation: ActivationReservationId::parse(&reservation).map_err(|error| {
            RunError::broken(format!(
                "a stored workspace selection reservation ({error})"
            ))
        })?,
        environment: environment.clone(),
        environment_generation: Generation::new(environment_generation as u64),
        run: parse_run(&run)?,
        session_run: parse_run(&session_run)?,
        receipt,
        state,
    })
}

pub(crate) fn parse_run(text: &str) -> Result<RunId, RunError> {
    text.parse()
        .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))
}

/// The obligation id is derived, never stored: it names the same activation the
/// row does, so a reader and the writer that created it agree without a second
/// authority. It grants nothing by being known.
pub(crate) fn derived_id(
    reservation: &str,
    run: &str,
    session_run: &str,
) -> Result<run::RequestId, RunError> {
    let reservation = ActivationReservationId::parse(reservation).map_err(|error| {
        RunError::broken(format!(
            "a stored workspace selection reservation ({error})"
        ))
    })?;
    let mut material = reservation.bytes().to_vec();
    material.extend_from_slice(run.as_bytes());
    material.extend_from_slice(session_run.as_bytes());
    material.extend_from_slice(b"effigy.workspace-selection-obligation.v1");
    let digest = Digest::of(&material);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.0[..16]);
    Ok(run::RequestId::new(bytes))
}

pub(crate) const OBLIGATION_COLUMNS: &str =
    "reservation_id, environment_generation, run_id, session_run, \
 state, receipt, created_at, reason";

pub(crate) fn obligation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObligationRow> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, i64>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, i64>(6)?,
        row.get::<_, Option<String>>(7)?,
    ))
}

pub(crate) fn read_obligation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    generation: Generation,
) -> Result<Option<SelectionObligation>, RunError> {
    let sql = format!(
        "SELECT {OBLIGATION_COLUMNS} FROM {OBLIGATIONS} WHERE environment=?1 AND environment_generation=?2"
    );
    let row = conn
        .query_row(
            &sql,
            params![environment.name().to_string(), generation.get() as i64],
            obligation_row,
        )
        .optional()?;
    row.map(|row| decode_obligation(environment, row))
        .transpose()
}

/// The environment's most recently committed selection, whatever its state.
///
/// A serving host fences on it before anything is announced: only a selection
/// the kernel recorded as switched may be served, and a pending or abandoned one
/// refuses. A recovery driver reads it to complete, abandon or leave it pending.
pub fn committed_selection(
    reader: &crate::store::StoreReader,
    environment: &ResourceId,
) -> Observed<Option<SelectionObligation>> {
    let sql = format!(
        "SELECT {OBLIGATION_COLUMNS} FROM {OBLIGATIONS} WHERE environment=?1 \
         ORDER BY environment_generation DESC LIMIT 1"
    );
    let row = match reader
        .conn()
        .query_row(
            &sql,
            params![environment.name().to_string()],
            obligation_row,
        )
        .optional()
    {
        Ok(row) => row,
        Err(error) => {
            return Observed::Unknown(crate::observe::UnknownReason::StoreUnreadable(
                error.to_string(),
            ))
        }
    };
    match row {
        Some(row) => match decode_obligation(environment, row) {
            Ok(obligation) => Observed::Verified(Some(obligation), super::session_evidence()),
            Err(error) => Observed::Unknown(crate::observe::UnknownReason::Inconsistent(
                error.to_string(),
            )),
        },
        None => Observed::Absent(crate::observe::AbsenceProof::NoRow {
            table: OBLIGATIONS.into(),
        }),
    }
}

/// The environment's committed but unfinished selection, whatever its owner's
/// liveness: a decision must meet it before it attaches or reserves anything.
pub(crate) fn pending_obligation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
) -> Result<Option<SelectionObligation>, RunError> {
    let sql = format!(
        "SELECT {OBLIGATION_COLUMNS} FROM {OBLIGATIONS} WHERE environment=?1 AND state='pending' \
         ORDER BY environment_generation DESC LIMIT 1"
    );
    let row = conn
        .query_row(&sql, [environment.name().to_string()], obligation_row)
        .optional()?;
    match row {
        Some(row) => {
            let obligation = decode_obligation(environment, row)?;
            Ok((obligation.state() == SelectionState::Pending).then_some(obligation))
        }
        None => Ok(None),
    }
}

/// The one unfinished selection for `environment`, if any. `Absent` means no
/// obligation was ever committed or the latest one finished; `Unknown` never
/// becomes either.
pub fn unfinished_selection(
    reader: &StoreReader,
    environment: &ResourceId,
) -> Observed<Option<SelectionObligation>> {
    let sql = format!(
        "SELECT {OBLIGATION_COLUMNS} FROM {OBLIGATIONS} WHERE environment=?1 AND state='pending' \
         ORDER BY environment_generation DESC LIMIT 1"
    );
    let row = match reader
        .conn()
        .query_row(&sql, [environment.name().to_string()], obligation_row)
    {
        Ok(row) => Some(row),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => {
            return Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string()));
        }
    };
    let Some(row) = row else {
        return Observed::Absent(AbsenceProof::NoRow {
            table: OBLIGATIONS.into(),
        });
    };
    match decode_obligation(environment, row) {
        Ok(obligation) if obligation.state == SelectionState::Pending => {
            Observed::Verified(Some(obligation), super::activation_evidence())
        }
        Ok(_) => Observed::Absent(AbsenceProof::NoRow {
            table: OBLIGATIONS.into(),
        }),
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}

/// Write the obligation row inside the commit that publishes its generation.
pub(crate) fn record_obligation(
    txn: &mut Txn<'_>,
    plan: &ActivationPlan,
    run: RunId,
    receipt: &SelectionReceipt,
) -> Result<SelectionObligation, RunError> {
    let obligation = SelectionObligation::new(plan, run, receipt.clone())?;
    txn.conn().execute(
        &format!(
            "INSERT INTO {OBLIGATIONS} (environment, environment_generation, reservation_id, run_id, \
             session_run, state, receipt, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,'pending',?6,?7,?7)"
        ),
        params![
            obligation.environment.name().to_string(),
            obligation.environment_generation.get() as i64,
            obligation.reservation.to_string(),
            obligation.run.to_string(),
            obligation.session_run.to_string(),
            serde_json::to_string(receipt)?,
            txn.now()
        ],
    )?;
    Ok(obligation)
}

impl SelectionObligation {
    /// Move this pending obligation to a terminal state, in the caller's
    /// transaction. Exactly one writer finishes a selection: a retry sees the
    /// terminal row and is answered with it, never written twice.
    pub(crate) fn finish(
        &self,
        txn: &Txn<'_>,
        state: SelectionState,
        reason: Option<&str>,
    ) -> Result<(), RunError> {
        let obligation = self;
        let stored = read_obligation(
            txn.conn(),
            obligation.environment(),
            obligation.environment_generation(),
        )?
        .ok_or_else(|| {
            RunError::broken("the workspace selection obligation vanished before it finished")
        })?;
        if stored.id != obligation.id {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        if stored.state != SelectionState::Pending {
            return Err(ActivationRefusal::SelectionSettled.into());
        }
        let changed = txn.conn().execute(
            &format!(
                "UPDATE {OBLIGATIONS} SET state=?3,reason=?4,updated_at=?5 \
                 WHERE environment=?1 AND environment_generation=?2 AND state='pending'"
            ),
            params![
                obligation.environment().name().to_string(),
                obligation.environment_generation().get() as i64,
                state.as_str(),
                reason,
                txn.now()
            ],
        )?;
        if changed != 1 {
            return Err(ActivationRefusal::SelectionSettled.into());
        }
        Ok(())
    }

    /// Whether the exact successor run reached complete settlement: closed with
    /// a complete cleanup outcome. An interrupted row that was never settled is
    /// not settled, and an in-progress stop is not settled either.
    pub(crate) fn successor_settled(&self, txn: &Txn<'_>) -> Result<bool, RunError> {
        let row: Option<(String, Option<String>)> = txn
            .conn()
            .query_row(
                "SELECT phase, cleanup_outcome FROM runs WHERE run_id=?1",
                [self.session_run.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((phase, cleanup)) => phase == "closed" && cleanup.as_deref() == Some("complete"),
            // A run row pruned after complete settlement is settled; a missing
            // row for a successor this generation committed is broken, not a
            // licence to revoke a live run.
            None => false,
        })
    }

    /// The committed generation must still be the one this obligation published
    /// and the environment must still be serving it. A recovery never
    /// re-targets a generation another activation replaced.
    fn require_publication(&self, txn: &Txn<'_>) -> Result<(), RunError> {
        let active = txn
            .conn()
            .query_row(
                "SELECT state='active' AND last_applied=?2 FROM resources WHERE name=?1",
                params![
                    self.environment.name().to_string(),
                    self.environment_generation.get().to_string()
                ],
                |row| row.get::<_, Option<bool>>(0),
            )?
            .unwrap_or(false);
        if !active {
            return Err(ActivationRefusal::ServingLost.into());
        }
        let mapped = txn
            .conn()
            .query_row(
                "SELECT run_id, environment_generation FROM workspace_sessions WHERE environment=?1",
                [self.environment.name().to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        match mapped {
            Some((run, generation))
                if run == self.session_run.to_string()
                    && generation as u64 == self.environment_generation.get() => {}
            _ => return Err(ActivationRefusal::ServingLost.into()),
        }
        Ok(())
    }

    pub(crate) fn require_serving(&self, txn: &Txn<'_>) -> Result<(), RunError> {
        self.require_publication(txn)?;
        // The session run itself must be open at its own generation: a settled
        // session serves nothing, whatever the mapping still says.
        let open: Option<(String, i64)> = txn
            .conn()
            .query_row(
                "SELECT phase, generation FROM runs WHERE run_id=?1",
                [self.session_run.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let staged: Option<i64> = txn
            .conn()
            .query_row(
                "SELECT generation FROM workspace_session_runs WHERE run_id=?1",
                [self.session_run.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match open {
            Some((phase, generation))
                if phase != "closed"
                    && phase != "stopping"
                    && generation == staged.unwrap_or(generation) => {}
            _ => return Err(ActivationRefusal::ServingLost.into()),
        }
        Ok(())
    }
}

/// Revoke a published generation whose selection can never complete. The
/// successor's mapping and the environment's `active` publication are removed;
/// the predecessor's own authority is not restored and no third value is
/// guessed.
pub(crate) fn revoke(txn: &Txn<'_>, obligation: &SelectionObligation) -> Result<(), RunError> {
    txn.conn().execute(
        "UPDATE resources SET state='pending',last_applied=NULL,updated_at=?2 WHERE name=?1",
        params![obligation.environment.name().to_string(), txn.now()],
    )?;
    txn.conn().execute(
        "DELETE FROM workspace_sessions WHERE environment=?1 AND run_id=?2",
        params![
            obligation.environment.name().to_string(),
            obligation.session_run.to_string()
        ],
    )?;
    Ok(())
}

impl WorkspaceActivationReservation {
    /// Commit the staged activation with the guest selection it published.
    ///
    /// This is the workspace path: it re-validates the stored plan, the held
    /// runs and the receipt, then writes `active(E)`, the session mapping, the
    /// session's `Borrow` attachment and a `Pending` selection obligation in
    /// one transaction, or none of them. A retry after a lost answer reads the
    /// committed obligation back instead of writing a second one, and a retry
    /// that names different bytes is refused: a committed generation's
    /// selection is frozen.
    pub fn commit_selection(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        environment_run: &Run<'_>,
        session_run: &Run<'_>,
        receipt: &SelectionReceipt,
        launch: &SessionLaunch,
    ) -> Result<CommittedActivation, RunError> {
        self.commit_with(cx, root, environment_run, session_run, receipt, launch)
    }
}

impl CommittedActivation {
    /// The selection obligation this commit wrote, if it committed one.
    pub fn selection_obligation(&self) -> Option<&SelectionObligation> {
        self.obligation.as_ref()
    }
}

/// Complete a selection from recovery, without a live activation-run handle.
///
/// Completion is possible only with all three facts proven together: the
/// session is live, the target it names still verifies and the listener holding
/// its key answered a bounded pinned handshake. Anything less is refused with
/// [`ActivationRefusal::ServingLost`] for the caller to abandon; unknown
/// liveness completes nothing. Recovery executes at the retained supervisor:
/// its exact launch pins owner/listener custody through final completion after
/// readback. A remote recovery driver must ask that owner to perform this step,
/// not manufacture a launch from a persisted lease.
#[allow(clippy::too_many_arguments)]
pub fn selection_recovered(
    cx: &EffectContext,
    root: &StateRoot,
    obligation: &SelectionObligation,
    launch: &super::SessionLaunch,
    target: &mut dyn super::TargetProbe,
    probe: &mut dyn super::ServingProbe,
    reader: &mut dyn SelectorReader,
    wait: Wait,
) -> Result<(), RunError> {
    // Nothing a caller observed earlier is trusted here, and neither is the
    // journal's own `Serving` row: recovery takes `Gate(Env)` first and then
    // revalidates, under that gate, that the committed successor session is
    // still the live owner and that the listener holding the lease's key still
    // answers a bounded pinned handshake.
    let _gate = Gate::acquire(
        cx,
        root,
        std::slice::from_ref(obligation.environment()),
        wait,
    )?;
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| RunError::broken("recovery needs an observing context"))?;
    let journal = activation_reader(&observe, root)?;
    match journal.run_owner_state(&observe, root, &obligation.session_run()) {
        Observed::Verified(crate::gate::OwnerState::Held, _) => {}
        Observed::Verified(_, _) | Observed::Absent(_) => {
            return Err(ActivationRefusal::ServingLost.into())
        }
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "the successor session's owner liveness is unknown: {reason}"
            )))
        }
    }
    let deadline = cx
        .operation()
        .remaining()
        .map(|left| left.min(std::time::Duration::from_secs(5)))
        .unwrap_or_else(|| std::time::Duration::from_secs(5));
    // The target the publication names must still be that exact container and
    // boot: a recovered generation is never completed against a replaced one.
    let publication = obligation.receipt().publication();
    match target.verify_target(
        &publication.publication.container_id,
        &publication.publication.vm_boot_id,
        deadline,
    ) {
        Observed::Verified(..) => {}
        Observed::Absent(_) => return Err(ActivationRefusal::ServingLost.into()),
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "selection liveness unknown: {reason}"
            )))
        }
    }
    let serving = obligation.receipt().serving();
    let Some(endpoint) = serving.socket() else {
        return Err(ActivationRefusal::ServingLost.into());
    };
    match probe.pinned_handshake(endpoint, serving.pin(), deadline) {
        Observed::Verified(..) => {}
        Observed::Absent(_) => return Err(ActivationRefusal::ServingLost.into()),
        Observed::Unknown(reason) => {
            return Err(RunError::unavailable(format!(
                "selection liveness unknown: {reason}"
            )))
        }
    }
    // The readback is completion, not assumption: the kernel reads every
    // selector and object itself and compares each answer with the committed
    // target.
    let receipt = obligation.observe(reader)?;
    obligation.validate_switch(&receipt)?;
    let custody = launch.hold_lease(obligation.receipt().serving())?;
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
    store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
        obligation.require_serving(txn)?;
        custody.revalidate(obligation.receipt().serving())?;
        obligation.finish(txn, SelectionState::Switched, None)
    })
}

/// Abandon a selection that can no longer be completed.
///
/// Recovery-only, and it refuses **only** while completion is still possible:
/// a live session with a verifying target and a live listener is not abandoned.
/// A live session whose listener is established dead *is*, because completing
/// it would publish a generation nothing serves. Unknown liveness is neither:
/// the obligation stays `Pending`. The revocation removes this generation's
/// publication; it never restores the predecessor.
pub fn selection_abandoned(
    cx: &EffectContext,
    root: &StateRoot,
    obligation: &SelectionObligation,
    liveness: &SelectionLiveness,
    reason: &str,
    wait: Wait,
) -> Result<(), RunError> {
    if liveness.complete() {
        return Err(ActivationRefusal::SelectionStillCompletable.into());
    }
    if !liveness.disproven() {
        // Nothing positively disproves the selection yet: an unknown listener
        // or session is not an established dead one, so the obligation stays
        // Pending rather than being abandoned on a guess.
        return Err(RunError::unavailable(
            "workspace selection liveness is unknown; it stays pending",
        ));
    }
    let _gate = Gate::acquire(
        cx,
        root,
        std::slice::from_ref(obligation.environment()),
        wait,
    )?;
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
    store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
        crate::fault_point!(SelectionAbandonBefore)?;
        // The exact successor is stopped and settled under its own private slot
        // *before* its generation is revoked, so abandonment never strands a
        // run that still owns a task slot, a listener or a holder.
        if !obligation.successor_settled(txn)? {
            return Err(ActivationRefusal::SuccessorRunning.into());
        }
        let stored = read_obligation(
            txn.conn(),
            obligation.environment(),
            obligation.environment_generation(),
        )?
        .ok_or(ActivationRefusal::SelectionMismatch)?;
        if stored.state != SelectionState::Pending
            || stored.id != obligation.id
            || serde_json::to_string(stored.receipt())?
                != serde_json::to_string(obligation.receipt())?
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        obligation.require_publication(txn)?;
        revoke(txn, obligation)?;
        obligation.finish(txn, SelectionState::Abandoned, Some(reason))?;
        Ok::<_, RunError>(())
    })?;
    crate::fault_point!(SelectionAbandonAfter)?;
    Ok(())
}
